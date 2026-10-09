//! 1단계(M1) 검증용 콘솔 프로그램.
//!
//! 훅을 걸고 넘패드·Shift·NumLock 이벤트와 판정 결과를 출력한다. 확인할 것:
//! - NumLock OFF일 때만 넘패드 키가 버튼으로 실행되는가
//! - 전용 이동 키·메인 Enter는 통과하는가
//! - `GetKeyState`로 읽은 NumLock 상태가 VK로 짐작한 상태와 일치하는가
//! - Shift + 넘패드에서 가짜 Shift 이벤트가 어떻게 보이는가
//! - 콜백 처리 시간이 충분히 짧은가
//!
//! 사용법: deck-probe [--quiet] [--loop-test] [--reinstall-secs N] [--config]
//!   --quiet           판정 기록 없이 버튼 실행과 NumLock 불일치만 출력.
//!                     통계(불일치 횟수, 최대 콜백)는 그대로 집계한다.
//!   --loop-test       버튼이 실행될 때마다 넘패드 5(VK_CLEAR)를 주입해
//!                     자기 입력을 다시 가로채지 않는지 확인
//!   --reinstall-secs  훅 재설치 주기 (기본 60초)
//!   --config          기본 설정 파일(config::default_path)을 읽고 감시한다.
//!                     없으면 기본 설정으로 만든다. 버튼 실행 때 첫 페이지의
//!                     버튼 이름을 함께 출력한다 (페이지 전환·액션은 아직 없음).
//!
//! 끝내려면 Ctrl+C.

use std::process::ExitCode;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use deck_core::config::{self, Config};
use deck_core::gate::Decision;
use deck_core::hook::{self, HookConfig, HookEvent, Trace};
use deck_core::inject;
use deck_core::key::{vk, NumpadKey};
use deck_core::layout::{self, Role};

struct Args {
    quiet: bool,
    loop_test: bool,
    reinstall_every: Duration,
    config: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        quiet: false,
        loop_test: false,
        config: false,
        reinstall_every: HookConfig::default().reinstall_every,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--quiet" => args.quiet = true,
            "--loop-test" => args.loop_test = true,
            "--config" => args.config = true,
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

    let deck_config = if args.config {
        match start_config() {
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
                let button = deck_config.as_ref().and_then(|(current, _)| {
                    let config = current.lock().unwrap();
                    config.button(0, key).map(|b| b.label.clone())
                });
                match button {
                    Some(label) => println!(
                        "▶ 버튼 실행: {:<5} → {} \"{label}\"",
                        key.label(),
                        describe_role(key)
                    ),
                    None => println!("▶ 버튼 실행: {:<5} → {}", key.label(), describe_role(key)),
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
            HookEvent::Reinstalled { count } => {
                println!(
                    "· 훅 재설치 {count}회 | 기록 {} · 실행 {} · NumLock 불일치 {} · 최대 콜백 {}µs",
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

/// 설정을 읽고 감시를 시작한다. 바뀔 때마다 현재 설정을 갈아 끼우고 결과를 출력한다.
fn start_config() -> Result<(Arc<Mutex<Config>>, config::ConfigWatcher), String> {
    let path = config::default_path().ok_or("APPDATA 환경 변수가 없습니다")?;
    let initial = config::load_or_create(&path).map_err(|e| e.to_string())?;
    println!("설정 파일: {}", path.display());
    print_config_summary(&initial);
    println!();

    let current = Arc::new(Mutex::new(initial.clone()));
    let shared = Arc::clone(&current);
    let watcher = config::watch(&path, initial, move |result| match result {
        Ok(config) => {
            println!("✔ 설정 다시 읽음");
            print_config_summary(&config);
            *shared.lock().unwrap() = config;
        }
        Err(e) => println!(
            "✖ {e}
  직전 설정을 계속 씁니다."
        ),
    })
    .map_err(|e| e.to_string())?;
    Ok((current, watcher))
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
