//! 코어 연결: 훅 → 덱(현재 페이지) → 액션 실행기, 설정 감시, 트레이·OSD 갱신.
//!
//! 훅 스레드는 판정만 하고 채널로 알린다. 여기서 만드는 디스패처 스레드가 그 알림을
//! 받아 페이지를 바꾸거나 실행기에 액션을 넘긴다.
//!
//! 교착 방지: 상태 잠금 안에서는 계산만 하고, 트레이·OSD 갱신은 잠금을 푼 뒤에 한다.
//! 트레이 갱신은 메인 스레드를 기다리는데, 메인 스레드(메뉴 처리)도 같은 잠금을 잡기
//! 때문이다.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use deck_core::action::{ExecEvent, Executor};
use deck_core::config::{self, Action, Config, ConfigWatcher};
use deck_core::deck::{Deck, Dispatch};
use deck_core::hook::{self, HookConfig, HookEvent, HookHandle};
use deck_core::i18n::{self, tr, Lang};
use deck_core::key::{NumpadKey, RawKey};
use deck_core::record::{Outcome, Recorder, Rejection};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::osd::{self, Osd, Payload, Show};
use crate::tray::{Mode, Tray, View};

struct State {
    config: Config,
    deck: Deck,
    /// 훅이 처음 알려 주기 전에는 `None`
    numlock_on: Option<bool>,
    paused: bool,
    config_error: Option<String>,
    /// 마지막으로 누른 버튼 키. 실행 실패를 OSD에 표시할 때 쓴다.
    last_button: Option<NumpadKey>,
    /// 단축키 녹화 중이면 `Some`
    recorder: Option<Recorder>,
    /// 마이크 음소거 버튼으로 마지막에 바꾼 상태. OSD가 버튼에 켜짐/꺼짐을 보인다.
    mic_muted: Option<bool>,
}

impl State {
    fn mode(&self) -> Mode {
        match (self.paused, self.numlock_on) {
            (true, _) => Mode::Paused,
            (false, Some(false)) => Mode::Deck,
            (false, _) => Mode::Numbers,
        }
    }

    fn view(&self) -> View {
        let page = self.deck.page();
        let name = self.config.pages.get(page).map_or("", |p| p.name.as_str());
        View {
            mode: self.mode(),
            page: format!("{}/{} {name}", page + 1, self.config.pages.len()),
            config_error: self.config_error.clone(),
        }
    }

    fn payload(&self, highlight: Option<NumpadKey>, error: Option<String>) -> Payload {
        let page = self.deck.page();
        let current = self.config.pages.get(page);
        Payload {
            seq: 0,
            mode: match self.mode() {
                Mode::Deck => "deck",
                Mode::Numbers => "numbers",
                Mode::Paused => "paused",
            },
            page,
            page_count: self.config.pages.len(),
            page_name: current.map_or_else(String::new, |p| p.name.clone()),
            buttons: current
                .map(|p| {
                    p.buttons
                        .iter()
                        .map(|(k, b)| (k.key().label().to_string(), b.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            highlight: highlight.map(|k| k.label().to_string()),
            error,
            mic_muted: self.mic_muted,
            layout: self.config.layout.clone(),
            system_keys: self.config.system_keys.clone(),
        }
    }
}

/// 편집기에 보내는 이벤트: 파일이 밖에서 바뀌어 다시 읽었다.
pub const CONFIG_CHANGED: &str = "config://changed";
/// 모든 창에 보내는 이벤트: 언어가 바뀌었다 (설정의 `language`).
pub const LANG_CHANGED: &str = "lang://changed";
/// 편집기에 보내는 이벤트: 모드(`"deck"` | `"numbers"` | `"paused"`)가 바뀌었을 수 있다 (온보딩).
pub const MODE_CHANGED: &str = "mode://changed";
/// 단축키 녹화 결과. 프런트엔드 `src/editor/api.ts`의 `RecordResult`와 모양이 같아야 한다.
///
/// 녹화는 편집기 화면이 받은 키로 한다 (전역 훅 녹화 모드를 쓰지 않는다). 편집기 창이
/// 앞에 있는 동안에는 키 다운이 저수준 키보드 훅을 거치지 않기 때문이다 (3-2 실측:
/// 훅 호출 수가 늘지 않음, 다른 프로세스의 훅도 마찬가지).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RecordResult {
    Keys {
        keys: Vec<String>,
    },
    Cancelled,
    /// `"numpadKey"` | `"unknownKey"`
    Rejected {
        reason: &'static str,
    },
}

impl From<Outcome> for RecordResult {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Keys(keys) => RecordResult::Keys { keys },
            Outcome::Cancelled => RecordResult::Cancelled,
            Outcome::Rejected(Rejection::NumpadKey) => RecordResult::Rejected {
                reason: "numpadKey",
            },
            Outcome::Rejected(Rejection::UnknownKey) => RecordResult::Rejected {
                reason: "unknownKey",
            },
        }
    }
}

/// 잠금을 푼 뒤 화면에 반영할 것.
#[derive(Default)]
struct Effects {
    tray: Option<View>,
    osd: Option<(Payload, Show)>,
    /// 편집기가 열려 있으면 보낸다.
    config_changed: Option<Config>,
    /// 띄우지 않고 OSD 내용만 바꾼다 (설정이 바뀌었을 때).
    osd_refresh: Option<Payload>,
    /// 언어가 바뀌었다 (창 제목·화면 문자열).
    lang: Option<Lang>,
}

/// 설정의 언어를 코어에 반영한다. 바뀌었으면 새 언어.
fn sync_lang(config: &Config) -> Option<Lang> {
    let lang = i18n::resolve(config.language);
    (lang != i18n::current()).then(|| {
        i18n::set(lang);
        lang
    })
}

/// 트레이와 OSD. 메인 스레드에서 만든 뒤 붙인다. 갱신할 때는 복제해 꺼내고 잠금을
/// 푼 뒤 쓴다 (트레이 갱신이 메인 스레드를 기다리는 동안 잠금을 쥐고 있지 않게).
#[derive(Default)]
struct Ui {
    tray: Option<Tray>,
    osd: Option<Arc<Osd>>,
    app: Option<AppHandle>,
}

pub struct Runtime {
    state: Arc<Mutex<State>>,
    ui: Arc<Mutex<Ui>>,
    hook: Arc<HookHandle>,
    executor: Executor,
    config_path: PathBuf,
    _watcher: Option<ConfigWatcher>,
    /// 설정 파일이 없어 새로 만들었다 = 첫 실행. 온보딩을 끝내거나 건너뛰면 꺼진다.
    onboarding: AtomicBool,
}

impl Runtime {
    /// 설정을 읽고 실행기·훅·설정 감시를 시작한다. 트레이·OSD는
    /// [`Runtime::attach_ui`]로 나중에 붙인다 (메인 스레드에서 만들어야 한다).
    pub fn start() -> Result<Runtime, String> {
        let config_path = config::default_path().ok_or("APPDATA 환경 변수가 없습니다")?;
        let first_run = !config_path.exists();
        // 첫 실행 기본 설정의 버튼 이름과 오류 메시지를 Windows 표시 언어로
        i18n::set(i18n::system());
        // 설정이 깨져 있으면 기본 설정으로 돌되 파일은 덮어쓰지 않는다.
        let (config, config_error) = match config::load_or_create(&config_path) {
            Ok(c) => (c, None),
            Err(e) => (Config::default_config(), Some(e.to_string())),
        };
        sync_lang(&config);

        let state = Arc::new(Mutex::new(State {
            config: config.clone(),
            deck: Deck::new(),
            numlock_on: None,
            paused: false,
            config_error,
            last_button: None,
            recorder: None,
            mic_muted: None,
        }));
        let ui: Arc<Mutex<Ui>> = Arc::default();

        // 지금 마이크 상태 (OSD의 마이크 버튼 표시용). 음소거 버튼과 같은 기준: 하나라도
        // 켜져 있으면 켜짐. COM 장치 목록이라 시작을 늦추지 않게 따로 읽는다.
        {
            let state = Arc::clone(&state);
            spawn_named("deck-mic-state", move || {
                let mics = deck_core::action::with_com(deck_core::action::mic::list);
                if let Ok(mics) =
                    mics.map(|m| m.into_iter().filter_map(|m| m.muted).collect::<Vec<_>>())
                {
                    if !mics.is_empty() {
                        let muted = mics.iter().all(|&m| m);
                        state.lock().unwrap().mic_muted.get_or_insert(muted);
                    }
                }
            });
        }

        let (exec_tx, exec_rx) = mpsc::channel();
        let executor = Executor::start(exec_tx).map_err(|e| e.to_string())?;
        {
            let state = Arc::clone(&state);
            let ui = Arc::clone(&ui);
            spawn_named("deck-exec-events", move || exec_loop(exec_rx, &state, &ui));
        }

        let watcher = {
            let state = Arc::clone(&state);
            let ui = Arc::clone(&ui);
            config::watch(&config_path, config, move |result| {
                let effects = {
                    let mut s = state.lock().unwrap();
                    let mut lang = None;
                    let changed = match result {
                        Ok(config) => {
                            lang = sync_lang(&config);
                            s.deck.on_config_changed(&config);
                            s.config = config.clone();
                            s.config_error = None;
                            Some(config)
                        }
                        Err(e) => {
                            eprintln!("설정 오류: {e}");
                            s.config_error = Some(e.to_string());
                            None
                        }
                    };
                    Effects {
                        tray: Some(s.view()),
                        osd_refresh: changed.is_some().then(|| s.payload(None, None)),
                        config_changed: changed,
                        lang,
                        ..Effects::default()
                    }
                };
                apply(&ui, effects);
            })
            .map_err(|e| eprintln!("설정 감시를 시작할 수 없습니다: {e}"))
            .ok()
        };

        let (hook_tx, hook_rx) = mpsc::channel();
        let mut hook_config = HookConfig::default();
        if diag() {
            // 진단: 다른 프로그램의 훅에 순서를 뺏기는지 보려고 1초마다 다시 설치한다.
            hook_config.reinstall_every = std::time::Duration::from_secs(1);
        }
        let hook = Arc::new(hook::start(hook_config, hook_tx).map_err(|e| e.to_string())?);
        start_diag();
        {
            let state = Arc::clone(&state);
            let ui = Arc::clone(&ui);
            let executor = executor.clone();
            spawn_named("deck-dispatch", move || {
                dispatch_loop(hook_rx, &state, &ui, &executor)
            });
        }

        Ok(Runtime {
            state,
            ui,
            hook,
            executor,
            config_path,
            _watcher: watcher,
            onboarding: AtomicBool::new(first_run),
        })
    }

    pub fn view(&self) -> View {
        self.state.lock().unwrap().view()
    }

    pub fn attach_ui(&self, app: AppHandle, tray: Tray, osd: Option<Arc<Osd>>) {
        {
            let mut ui = self.ui.lock().unwrap();
            ui.tray = Some(tray);
            ui.osd = osd;
            ui.app = Some(app);
        }
        let view = self.view();
        apply(
            &self.ui,
            Effects {
                tray: Some(view),
                ..Effects::default()
            },
        );
    }

    pub fn tray(&self) -> Option<Tray> {
        self.ui.lock().unwrap().tray.clone()
    }

    /// 트레이 Windows 시작 시 실행 체크를 실제 상태에 맞춘다.
    pub fn set_autostart_checked(&self, on: bool) {
        let tray = self.ui.lock().unwrap().tray.clone();
        if let Some(tray) = tray {
            tray.set_autostart(on);
        }
    }

    pub fn lang(&self) -> Lang {
        i18n::current()
    }

    pub fn mode(&self) -> Mode {
        self.state.lock().unwrap().mode()
    }

    /// 첫 실행이라 온보딩을 보여줘야 하는가.
    pub fn onboarding_pending(&self) -> bool {
        self.onboarding.load(Ordering::Relaxed)
    }

    pub fn finish_onboarding(&self) {
        self.onboarding.store(false, Ordering::Relaxed);
    }

    pub fn config(&self) -> Config {
        self.state.lock().unwrap().config.clone()
    }

    /// 편집기가 저장한다. 검증에 실패하면 저장하지 않고 이유를 돌려준다. 저장하면 파일
    /// 감시를 기다리지 않고 바로 반영한다 (감시가 같은 내용을 다시 읽어도 무해하다).
    pub fn save_config(&self, config: Config) -> Result<(), String> {
        config::save(&self.config_path, &config).map_err(|e| e.to_string())?;
        let lang = sync_lang(&config);
        let effects = {
            let mut s = self.state.lock().unwrap();
            s.deck.on_config_changed(&config);
            s.config = config;
            s.config_error = None;
            Effects {
                tray: Some(s.view()),
                osd_refresh: Some(s.payload(None, None)),
                lang,
                ..Effects::default()
            }
        };
        apply(&self.ui, effects);
        Ok(())
    }

    pub fn toggle_pause(&self) {
        let effects = {
            let mut s = self.state.lock().unwrap();
            s.paused = !s.paused;
            self.hook.set_paused(s.paused);
            Effects {
                tray: Some(s.view()),
                osd: Some((s.payload(None, None), Show::For(osd::MODE))),
                ..Effects::default()
            }
        };
        apply(&self.ui, effects);
    }

    /// 편집기의 `시험 실행`.
    pub fn test_action(&self, label: String, action: Action) {
        self.executor.submit(label, action);
    }

    /// 편집기의 단축키 녹화를 시작한다. 키는 [`Runtime::record_key`]로 받는다.
    pub fn start_recording(&self) {
        self.state.lock().unwrap().recorder = Some(Recorder::new());
    }

    pub fn cancel_recording(&self) {
        self.state.lock().unwrap().recorder = None;
    }

    /// 편집기 화면이 받은 키 하나. 녹화가 끝나면 결과를 돌려준다.
    pub fn record_key(&self, key: RawKey, down: bool) -> Option<RecordResult> {
        let mut s = self.state.lock().unwrap();
        let outcome = s.recorder.as_mut()?.on_key(key, down)?;
        s.recorder = None;
        Some(outcome.into())
    }

    /// 편집기 화면이 받은 넘패드 키 (NumLock 꺼짐). 훅이 받은 것처럼 처리한다.
    pub fn press_deck_key(&self, key: NumpadKey) {
        let effects = {
            let mut s = self.state.lock().unwrap();
            if s.paused {
                return;
            }
            on_hook_event(&mut s, HookEvent::Fire(key), &self.executor)
        };
        apply(&self.ui, effects);
    }

    /// 재생 중인 효과음을 모두 멈춘다 (트레이 메뉴).
    pub fn stop_sounds(&self) {
        self.executor
            .submit(tr("효과음 멈추기", "Stop sounds"), Action::StopSounds);
    }

    pub fn config_path(&self) -> &std::path::Path {
        &self.config_path
    }

    /// 설정 파일이 있는 폴더를 탐색기로 연다 (백업·공유용).
    pub fn open_config_folder(&self) {
        if let Some(dir) = self.config_path.parent() {
            self.executor.submit(
                tr("설정 폴더 열기", "Open config folder"),
                Action::Open {
                    target: dir.to_string_lossy().into_owned(),
                },
            );
        }
    }

    pub fn open_config(&self) {
        self.executor.submit(
            tr("설정 파일 열기", "Open config file"),
            Action::Open {
                target: self.config_path.to_string_lossy().into_owned(),
            },
        );
    }
}

fn dispatch_loop(
    events: Receiver<HookEvent>,
    state: &Mutex<State>,
    ui: &Mutex<Ui>,
    executor: &Executor,
) {
    for event in events {
        if diag() && !matches!(event, HookEvent::Reinstalled { .. }) {
            eprintln!("[진단] 디스패처 받음: {event:?}");
        }
        let effects = {
            let mut s = state.lock().unwrap();
            on_hook_event(&mut s, event, executor)
        };
        apply(ui, effects);
        DISPATCHED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// 진단 모드 (환경 변수 NPDK_DIAG). 켜면 1초마다 훅·디스패처 상태를 콘솔에 찍는다.
fn diag() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("NPDK_DIAG").is_some())
}

static DISPATCHED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static APPLYING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 진단 스레드: 훅이 받은 키 수, 디스패처가 처리한 수, 화면 갱신 중인지.
pub fn start_diag() {
    if !diag() {
        return;
    }
    spawn_named("deck-diag", || {
        use std::sync::atomic::Ordering::Relaxed;
        let mut last = (0, 0);
        loop {
            thread::sleep(std::time::Duration::from_secs(1));
            let now = (hook::hook_calls(), DISPATCHED.load(Relaxed));
            if now != last || APPLYING.load(Relaxed) {
                eprintln!(
                    "[진단] 훅 호출 {} · 디스패처 처리 {} · 화면 갱신 중 {}",
                    now.0,
                    now.1,
                    APPLYING.load(Relaxed)
                );
                last = now;
            }
        }
    });
}

fn on_hook_event(s: &mut State, event: HookEvent, executor: &Executor) -> Effects {
    match event {
        HookEvent::Fire(key) => {
            let State { config, deck, .. } = &mut *s;
            match deck.on_press(key, config) {
                Dispatch::Button { button, .. } => {
                    s.last_button = Some(key);
                    // 마이크 음소거는 바뀐 상태를 받은 뒤(`ExecEvent::MicMuted`) 띄운다. 지금 띄우면
                    // 바뀌기 전 상태 색이 잠깐 보인다. 실패하면 실패 알림이 띄운다.
                    let wait_for_mic =
                        matches!(&button, Some(b) if matches!(b.action, Action::MicMute));
                    if let Some(button) = button {
                        executor.submit(button.label, button.action);
                    }
                    Effects {
                        osd: (!wait_for_mic)
                            .then(|| (s.payload(Some(key), None), Show::For(osd::BUTTON))),
                        ..Effects::default()
                    }
                }
                Dispatch::Page { .. } => Effects {
                    tray: Some(s.view()),
                    osd: Some((s.payload(None, None), Show::For(osd::PAGE))),
                    ..Effects::default()
                },
                Dispatch::ToggleOsd => Effects {
                    osd: Some((s.payload(None, None), Show::Toggle)),
                    ..Effects::default()
                },
            }
        }
        HookEvent::NumLock(on) => {
            let previous = s.numlock_on.replace(on);
            // 시작할 때 받는 첫 알림에는 OSD를 띄우지 않는다.
            let changed = previous.is_some_and(|p| p != on);
            Effects {
                tray: Some(s.view()),
                osd: (changed && !s.paused).then(|| (s.payload(None, None), Show::For(osd::MODE))),
                ..Effects::default()
            }
        }
        HookEvent::Reinstalled { count, reason } => {
            if !diag() {
                eprintln!("훅 재설치 {count}회 ({reason:?})");
            }
            Effects::default()
        }
        // 앱은 훅 녹화 모드를 쓰지 않는다 (편집기 화면이 키를 받는다).
        HookEvent::Recorded { .. } => Effects::default(),
        HookEvent::Trace(_) => Effects::default(),
    }
}

/// 실행 결과: 실패하면 OSD에 이유를, 마이크를 바꾸면 켜짐/꺼짐을 보여준다.
fn exec_loop(events: Receiver<ExecEvent>, state: &Mutex<State>, ui: &Mutex<Ui>) {
    for event in events {
        if let ExecEvent::MicMuted(muted) = event {
            let effects = {
                let mut s = state.lock().unwrap();
                s.mic_muted = Some(muted);
                Effects {
                    osd: Some((s.payload(s.last_button, None), Show::For(osd::BUTTON))),
                    ..Effects::default()
                }
            };
            apply(ui, effects);
        } else if let ExecEvent::Failed { label, error } = event {
            eprintln!("\"{label}\" 실패: {error}");
            let effects = {
                let s = state.lock().unwrap();
                Effects {
                    osd: Some((
                        s.payload(
                            s.last_button,
                            Some(tr(
                                format!("\"{label}\" 실패: {error}"),
                                format!("\"{label}\" failed: {error}"),
                            )),
                        ),
                        Show::For(osd::ERROR),
                    )),
                    ..Effects::default()
                }
            };
            apply(ui, effects);
        }
    }
}

fn apply(ui: &Mutex<Ui>, effects: Effects) {
    APPLYING.store(true, std::sync::atomic::Ordering::Relaxed);
    apply_inner(ui, effects);
    APPLYING.store(false, std::sync::atomic::Ordering::Relaxed);
}

fn apply_inner(ui: &Mutex<Ui>, effects: Effects) {
    let (tray, osd, app) = {
        let ui = ui.lock().unwrap();
        (ui.tray.clone(), ui.osd.clone(), ui.app.clone())
    };
    if let Some(app) = &app {
        if let Some(config) = effects.config_changed {
            let _ = app.emit_to(crate::editor::LABEL, CONFIG_CHANGED, config);
        }
        if let Some(view) = &effects.tray {
            let _ = app.emit_to(crate::editor::LABEL, MODE_CHANGED, view.mode);
        }
        if let Some(lang) = effects.lang {
            let _ = app.emit(LANG_CHANGED, lang);
            crate::editor::retitle(app);
        }
    }
    if let (Some(tray), Some(view)) = (tray, effects.tray) {
        tray.update(&view);
    }
    if let Some(osd) = osd {
        if let Some(payload) = effects.osd_refresh {
            osd.refresh(payload);
        }
        if let Some((payload, how)) = effects.osd {
            osd.show(payload, how);
        }
    }
}

fn spawn_named(name: &str, f: impl FnOnce() + Send + 'static) {
    thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .expect("스레드를 만들 수 없습니다");
}
