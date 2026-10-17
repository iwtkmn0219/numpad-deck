//! 1단계(M1) 검증용 콘솔 프로그램.
//!
//! 훅을 걸고 넘패드·Shift·NumLock 이벤트와 판정 결과를 출력한다. 확인할 것:
//! - NumLock OFF일 때만 넘패드 키가 버튼으로 실행되는가
//! - 전용 이동 키·메인 Enter는 통과하는가
//! - `GetKeyState`로 읽은 NumLock 상태가 VK로 짐작한 상태와 일치하는가
//! - Shift + 넘패드에서 가짜 Shift 이벤트가 어떻게 보이는가
//! - 콜백 처리 시간이 충분히 짧은가
//!
//! 사용법: deck-probe [--quiet] [--loop-test] [--reinstall-secs N] [--config [--config-file PATH] [--run]] | --mics [--mute-test] | --record | --apps | --clipboard
//!   --quiet           판정 기록 없이 버튼 실행과 NumLock 불일치만 출력.
//!                     통계(불일치 횟수, 최대 콜백)는 그대로 집계한다.
//!   --loop-test       버튼이 실행될 때마다 넘패드 5(VK_CLEAR)를 주입해
//!                     자기 입력을 다시 가로채지 않는지 확인
//!   --reinstall-secs  훅 재설치 주기 (기본 60초)
//!   --config          기본 설정 파일(config::default_path)을 읽고 감시한다.
//!                     없으면 기본 설정으로 만든다. 넘패드를 누를 때마다
//!                     페이지 전환과 실행할 버튼을 출력한다.
//!   --config-file     --config와 함께: 기본 위치 대신 이 설정 파일을 쓴다.
//!   --run             --config와 함께: 버튼의 액션을 실제로 실행한다.
//!   --mics            녹음 장치 목록과 기본 장치·음소거 상태를 출력하고 끝낸다.
//!   --mute-test       --mics와 함께: 장치마다 음소거 → 3초 대기 → 원래대로를 차례로 한다.
//!   --record          단축키 녹화를 한 번 해 보고 결과를 출력한 뒤 끝낸다.
//!   --apps            설치된 앱 목록(편집기의 앱 고르기와 같은 것)을 출력한다.
//!   --clipboard       문장 붙여넣기의 클립보드 보관을 해 보고 결과만 출력한다(내용은 출력하지 않음).
//!
//! 끝내려면 Ctrl+C.

use std::process::ExitCode;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use deck_core::action::{ExecEvent, Executor};
use deck_core::config::{self, Config};
use deck_core::deck::{Deck, Dispatch};
use deck_core::gate::Decision;
use deck_core::hook::{self, HookConfig, HookEvent, Reinstall, Trace};
use deck_core::inject;
use deck_core::key::{vk, NumpadKey};
use deck_core::layout::{self, Role};

struct Args {
    quiet: bool,
    loop_test: bool,
    reinstall_every: Duration,
    config: bool,
    run: bool,
    config_file: Option<std::path::PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        quiet: false,
        loop_test: false,
        config: false,
        run: false,
        config_file: None,
        reinstall_every: HookConfig::default().reinstall_every,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--quiet" => args.quiet = true,
            "--loop-test" => args.loop_test = true,
            "--config" => args.config = true,
            "--run" => args.run = true,
            "--config-file" => {
                let path = it
                    .next()
                    .ok_or("--config-file 뒤에 파일 경로가 필요합니다")?;
                args.config_file = Some(path.into());
            }
            "--reinstall-secs" => {
                let secs: u64 = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--reinstall-secs 뒤에 초 단위 숫자가 필요합니다")?;
                args.reinstall_every = Duration::from_secs(secs);
            }
            other => return Err(format!("알 수 없는 인자: {other}")),
        }
    }
    Ok(args)
}

#[derive(Default)]
struct Stats {
    events: u64,
    fires: u64,
    numlock_mismatches: u64,
    max_elapsed: Duration,
}

/// 자기 입력 무시가 깨지면 버튼이 끝없이 실행된다. 1초에 이만큼 넘으면 중단한다.
const RUNAWAY_FIRES_PER_SEC: usize = 30;

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    if raw.iter().any(|a| a == "--mics") {
        return deck_core::action::with_com(|| mics(raw.iter().any(|a| a == "--mute-test")));
    }
    if raw.iter().any(|a| a == "--record") {
        return record_once();
    }
    if raw.iter().any(|a| a == "--clipboard") {
        return match deck_core::clipboard::snapshot() {
            Ok(snapshot) => {
                let (formats, bytes) = snapshot.summary();
                println!("보관 성공: 형식 {formats}개, {bytes}바이트");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("보관 실패: {e}");
                ExitCode::FAILURE
            }
        };
    }
    if raw.iter().any(|a| a == "--apps") {
        return match deck_core::action::with_com(deck_core::action::apps::list) {
            Ok(apps) => {
                for a in &apps {
                    println!("{}  →  {}", a.name, a.target);
                }
                println!("{}개", apps.len());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    println!("Numpad Deck 프로브 — NumLock을 끄고 넘패드를 눌러 보세요. 끝내려면 Ctrl+C.");
    println!("훅 재설치 주기: {}초", args.reinstall_every.as_secs());
    if args.loop_test {
        println!("자기 입력 테스트: 버튼마다 넘패드 5(VK_CLEAR)를 주입합니다.");
    }
    println!();

    if (args.run || args.config_file.is_some()) && !args.config {
        eprintln!("--run과 --config-file은 --config와 함께 써야 합니다");
        return ExitCode::from(2);
    }
    let executor = if args.run {
        match start_executor() {
            Ok(executor) => Some(executor),
            Err(e) => {
                eprintln!("액션 실행기를 시작할 수 없습니다: {e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };

    let deck_config = if args.config {
        match start_config(args.config_file.clone()) {
            Ok(pair) => Some(pair),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };

    let (tx, rx) = mpsc::channel();
    let config = HookConfig {
        // 통계를 내려면 --quiet여도 기록은 받아야 한다. 출력만 줄인다.
        trace: true,
        reinstall_every: args.reinstall_every,
    };
    let _hook = match hook::start(config, tx) {
        Ok(handle) => handle,
        Err(e) => {
            eprintln!("훅 설치 실패: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut stats = Stats::default();
    let mut recent_fires: Vec<Instant> = Vec::new();

    for event in rx {
        match event {
            HookEvent::Fire(key) => {
                stats.fires += 1;
                println!("▶ 버튼 실행: {:<5} → {}", key.label(), describe_role(key));
                if let Some((state, _)) = &deck_config {
                    let mut state = state.lock().unwrap();
                    let DeckState { config, deck } = &mut *state;
                    let dispatch = deck.on_press(key, config);
                    println!("  {}", describe_dispatch(&dispatch, config));
                    if let (
                        Some(executor),
                        Dispatch::Button {
                            button: Some(button),
                            ..
                        },
                    ) = (&executor, dispatch)
                    {
                        executor.submit(button.label, button.action);
                    }
                }

                let now = Instant::now();
                recent_fires.retain(|t| now.duration_since(*t) < Duration::from_secs(1));
                recent_fires.push(now);
                if recent_fires.len() > RUNAWAY_FIRES_PER_SEC {
                    eprintln!("✖ 1초에 {RUNAWAY_FIRES_PER_SEC}번 넘게 실행됨 — 자기 입력을 다시 가로채고 있습니다.");
                    return ExitCode::FAILURE;
                }

                if args.loop_test {
                    if let Err(e) = inject::tap_vk(vk::CLEAR, false) {
                        eprintln!("  주입 실패: {e}");
                    }
                }
            }
            HookEvent::Trace(t) => {
                stats.events += 1;
                stats.max_elapsed = stats.max_elapsed.max(t.elapsed);
                let mismatch =
                    matches!((t.numlock_on, t.numlock_hint), (Some(a), Some(b)) if a != b);
                if mismatch {
                    stats.numlock_mismatches += 1;
                }
                if !args.quiet || mismatch {
                    println!("{}", format_trace(&t, mismatch));
                }
            }
            HookEvent::Recorded { .. } => {}
            HookEvent::NumLock(on) => {
                println!(
                    "· NumLock {}",
                    if on {
                        "ON (숫자 모드)"
                    } else {
                        "OFF (덱 모드)"
                    }
                );
            }
            HookEvent::Reinstalled { count, reason } => {
                let why = match reason {
                    Reinstall::Periodic => "",
                    Reinstall::SessionUnlock => " (잠금 해제)",
                    Reinstall::Resume => " (절전 복귀)",
                };
                println!(
                    "· 훅 재설치 {count}회{why} | 기록 {} · 실행 {} · NumLock 불일치 {} · 최대 콜백 {}µs",
                    stats.events,
                    stats.fires,
                    stats.numlock_mismatches,
                    stats.max_elapsed.as_micros()
                );
            }
        }
    }
    ExitCode::SUCCESS
}

/// 실행기를 띄우고 결과를 출력하는 스레드를 붙인다.
fn start_executor() -> std::io::Result<Executor> {
    let (tx, rx) = mpsc::channel();
    let executor = Executor::start(tx)?;
    std::thread::spawn(move || {
        for event in rx {
            match event {
                ExecEvent::Done { label } => println!("  ✔ \"{label}\" 완료"),
                ExecEvent::Failed { label, error } => println!("  ✖ \"{label}\" 실패: {error}"),
                ExecEvent::MicMuted(muted) => {
                    println!("  🎙 마이크 {}", if muted { "음소거" } else { "켜짐" })
                }
            }
        }
    });
    Ok(executor)
}

/// 단축키 녹화를 한 번 해 본다. 녹화 중에는 모든 키가 막힌다 (5초 동안 키가 없으면 끝).
fn record_once() -> ExitCode {
    use deck_core::record::{Outcome, Recorder};
    let (tx, rx) = mpsc::channel();
    let hook = match hook::start(HookConfig::default(), tx) {
        Ok(handle) => handle,
        Err(e) => {
            eprintln!("훅 설치 실패: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "단축키를 눌러 보세요 (예: Ctrl+Shift+S, Win). Esc = 취소, 5초 동안 입력이 없으면 끝."
    );
    hook.start_recording();
    let mut recorder = Recorder::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(HookEvent::Recorded { key, down }) => {
                println!(
                    "  {} vk=0x{:02X}{}",
                    if down { "↓" } else { "↑" },
                    key.vk,
                    if key.extended { " ext" } else { "" }
                );
                if let Some(outcome) = recorder.on_key(key, down) {
                    hook.stop_recording();
                    match outcome {
                        Outcome::Keys(keys) => println!("녹화됨: {}", keys.join(" + ")),
                        Outcome::Cancelled => println!("취소됨"),
                        Outcome::Rejected(why) => println!("녹화할 수 없는 키: {why:?}"),
                    }
                    return ExitCode::SUCCESS;
                }
            }
            Ok(_) => {}
            Err(_) if !hook.recording() => {
                println!("시간 초과로 녹화를 끝냈습니다.");
                return ExitCode::SUCCESS;
            }
            Err(_) => {}
        }
    }
}

/// 녹음 장치 진단. 오디오 인터페이스처럼 기본 장치가 아닌 마이크가 음소거되는지 본다.
fn mics(mute_test: bool) -> ExitCode {
    use deck_core::action::mic;
    let list = match mic::list() {
        Ok(list) => list,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    println!("연결된 녹음 장치 {}개", list.len());
    for (i, m) in list.iter().enumerate() {
        let mut roles = Vec::new();
        if m.default_console {
            roles.push("기본 장치");
        }
        if m.default_communications {
            roles.push("기본 통신 장치");
        }
        let muted = match m.muted {
            Some(true) => "음소거됨",
            Some(false) => "켜짐",
            None => "상태 모름",
        };
        println!(
            "  {}. {} [{}] {} · 하드웨어 음소거 {}",
            i + 1,
            m.name,
            if roles.is_empty() {
                "-".into()
            } else {
                roles.join(", ")
            },
            muted,
            if m.hardware_mute {
                "지원"
            } else {
                "미지원"
            }
        );
    }
    if mute_test {
        println!();
        println!(
            "장치마다 3초씩 음소거합니다. 그동안 그 마이크에 소리를 내며 앱의 입력 레벨을 보세요."
        );
        for (i, m) in list.iter().enumerate() {
            let Some(was) = m.muted else { continue };
            println!("  {}. {} 음소거 중…", i + 1, m.name);
            if let Err(e) = mic::set_mute(&m.id, true) {
                println!("     실패: {e}");
                continue;
            }
            std::thread::sleep(Duration::from_secs(3));
            let _ = mic::set_mute(&m.id, was);
            println!("     원래대로 ({})", if was { "음소거됨" } else { "켜짐" });
        }
    }
    ExitCode::SUCCESS
}

struct DeckState {
    config: Config,
    deck: Deck,
}

/// 설정을 읽고 감시를 시작한다. 바뀔 때마다 현재 설정을 갈아 끼우고 결과를 출력한다.
fn start_config(
    file: Option<std::path::PathBuf>,
) -> Result<(Arc<Mutex<DeckState>>, config::ConfigWatcher), String> {
    let path = match file {
        // 상대 경로면 폴더 감시가 실패하므로 절대 경로로 바꾼다.
        Some(path) => std::path::absolute(&path).map_err(|e| e.to_string())?,
        None => config::default_path().ok_or("APPDATA 환경 변수가 없습니다")?,
    };
    let initial = config::load_or_create(&path).map_err(|e| e.to_string())?;
    println!("설정 파일: {}", path.display());
    print_config_summary(&initial);
    println!();

    let state = Arc::new(Mutex::new(DeckState {
        config: initial.clone(),
        deck: Deck::new(),
    }));
    let shared = Arc::clone(&state);
    let watcher = config::watch(&path, initial, move |result| match result {
        Ok(config) => {
            println!("✔ 설정 다시 읽음");
            print_config_summary(&config);
            let mut state = shared.lock().unwrap();
            if let Some(dispatch) = state.deck.on_config_changed(&config) {
                println!("  {}", describe_dispatch(&dispatch, &config));
            }
            state.config = config;
        }
        Err(e) => println!("✖ {e}\n  직전 설정을 계속 씁니다."),
    })
    .map_err(|e| e.to_string())?;
    Ok((state, watcher))
}

fn describe_dispatch(dispatch: &Dispatch, config: &Config) -> String {
    let page_name = |page: usize| {
        let name = config.pages.get(page).map_or("", |p| p.name.as_str());
        format!("페이지 {}/{} \"{name}\"", page + 1, config.pages.len())
    };
    match dispatch {
        Dispatch::Button {
            page,
            button: Some(button),
            ..
        } => format!("{} · \"{}\" 실행", page_name(*page), button.label),
        Dispatch::Button { page, .. } => format!("{} · 빈 자리", page_name(*page)),
        Dispatch::Page { page } => format!("→ {}", page_name(*page)),
        Dispatch::ToggleOsd => "OSD 띄우기/숨기기".into(),
    }
}

fn print_config_summary(config: &Config) {
    for (i, page) in config.pages.iter().enumerate() {
        let labels: Vec<String> = page
            .buttons
            .iter()
            .map(|(key, button)| format!("{}={}", key.key().label(), button.label))
            .collect();
        println!(
            "  페이지 {} \"{}\": {}",
            i + 1,
            page.name,
            labels.join(", ")
        );
    }
}

fn describe_role(key: NumpadKey) -> String {
    match layout::role(key) {
        Role::Button(i) => format!("버튼 {}", i + 1),
        Role::PrevPage => "이전 페이지".into(),
        Role::NextPage => "다음 페이지".into(),
        Role::ToggleOsd => "OSD 띄우기/숨기기".into(),
        Role::FirstPage => "첫 페이지".into(),
    }
}

fn format_trace(t: &Trace, mismatch: bool) -> String {
    let edge = if t.down { "↓" } else { "↑" };
    let key = t.key.map_or("-", NumpadKey::label);
    let decision = match t.decision {
        Decision::Pass => "통과",
        Decision::Swallow => "막음",
        Decision::Fire(_) => "실행",
    };
    let numlock = match t.numlock_on {
        Some(true) => "ON ",
        Some(false) => "OFF",
        None => "   ",
    };
    let mut flags = String::new();
    if t.extended {
        flags.push_str(" ext");
    }
    if t.injected {
        flags.push_str(if t.ours { " inj:self" } else { " inj" });
    }
    if t.scan & 0x200 != 0 {
        flags.push_str(" 가짜");
    }
    let warn = if mismatch {
        format!(
            "  ⚠ NumLock 불일치: GetKeyState={numlock} VK추정={}",
            if t.numlock_hint == Some(true) {
                "ON"
            } else {
                "OFF"
            }
        )
    } else {
        String::new()
    };
    format!(
        "  {edge} vk=0x{:02X} sc=0x{:02X}{flags:<14} 키={key:<5} NumLock={numlock} {decision} {:>4}µs t={}{warn}",
        t.vk,
        t.scan,
        t.elapsed.as_micros(),
        t.time_ms
    )
}
