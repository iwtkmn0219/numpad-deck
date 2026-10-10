//! 코어 연결: 훅 → 덱(현재 페이지) → 액션 실행기, 설정 감시, 트레이·OSD 갱신.
//!
//! 훅 스레드는 판정만 하고 채널로 알린다. 여기서 만드는 디스패처 스레드가 그 알림을
//! 받아 페이지를 바꾸거나 실행기에 액션을 넘긴다.
//!
//! 교착 방지: 상태 잠금 안에서는 계산만 하고, 트레이·OSD 갱신은 잠금을 푼 뒤에 한다.
//! 트레이 갱신은 메인 스레드를 기다리는데, 메인 스레드(메뉴 처리)도 같은 잠금을 잡기
//! 때문이다.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use deck_core::action::{ExecEvent, Executor};
use deck_core::config::{self, Action, Config, ConfigWatcher};
use deck_core::deck::{Deck, Dispatch};
use deck_core::hook::{self, HookConfig, HookEvent, HookHandle};
use deck_core::key::NumpadKey;
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
                        .map(|(k, b)| (k.key().label().to_string(), b.label.clone()))
                        .collect()
                })
                .unwrap_or_default(),
            highlight: highlight.map(|k| k.label().to_string()),
            error,
        }
    }
}

/// 편집기에 보내는 이벤트: 파일이 밖에서 바뀌어 다시 읽었다.
pub const CONFIG_CHANGED: &str = "config://changed";
/// 편집기에 보내는 이벤트: 단축키 녹화가 끝났다.
pub const HOTKEY_RECORDED: &str = "hotkey://recorded";

/// 단축키 녹화 결과. 프런트엔드 `src/editor/api.ts`의 `RecordResult`와 모양이 같아야 한다.
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
    Timeout,
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
    recorded: Option<RecordResult>,
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
}

impl Runtime {
    /// 설정을 읽고 실행기·훅·설정 감시를 시작한다. 트레이·OSD는
    /// [`Runtime::attach_ui`]로 나중에 붙인다 (메인 스레드에서 만들어야 한다).
    pub fn start() -> Result<Runtime, String> {
        let config_path = config::default_path().ok_or("APPDATA 환경 변수가 없습니다")?;
        // 설정이 깨져 있으면 기본 설정으로 돌되 파일은 덮어쓰지 않는다.
        let (config, config_error) = match config::load_or_create(&config_path) {
            Ok(c) => (c, None),
            Err(e) => (Config::default_config(), Some(e.to_string())),
        };

        let state = Arc::new(Mutex::new(State {
            config: config.clone(),
            deck: Deck::new(),
            numlock_on: None,
            paused: false,
            config_error,
            last_button: None,
            recorder: None,
        }));
        let ui: Arc<Mutex<Ui>> = Arc::default();

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
                    let changed = match result {
                        Ok(config) => {
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
                        config_changed: changed,
                        ..Effects::default()
                    }
                };
                apply(&ui, effects);
            })
            .map_err(|e| eprintln!("설정 감시를 시작할 수 없습니다: {e}"))
            .ok()
        };

        let (hook_tx, hook_rx) = mpsc::channel();
        let hook =
            Arc::new(hook::start(HookConfig::default(), hook_tx).map_err(|e| e.to_string())?);
        {
            let state = Arc::clone(&state);
            let ui = Arc::clone(&ui);
            let executor = executor.clone();
            let hook = Arc::clone(&hook);
            spawn_named("deck-dispatch", move || {
                dispatch_loop(hook_rx, &state, &ui, &executor, &hook)
            });
        }

        Ok(Runtime {
            state,
            ui,
            hook,
            executor,
            config_path,
            _watcher: watcher,
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

    pub fn config(&self) -> Config {
        self.state.lock().unwrap().config.clone()
    }

    /// 편집기가 저장한다. 검증에 실패하면 저장하지 않고 이유를 돌려준다. 저장하면 파일
    /// 감시를 기다리지 않고 바로 반영한다 (감시가 같은 내용을 다시 읽어도 무해하다).
    pub fn save_config(&self, config: Config) -> Result<(), String> {
        config::save(&self.config_path, &config).map_err(|e| e.to_string())?;
        let effects = {
            let mut s = self.state.lock().unwrap();
            s.deck.on_config_changed(&config);
            s.config = config;
            s.config_error = None;
            Effects {
                tray: Some(s.view()),
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

    /// 단축키 녹화를 시작한다. 결과는 [`HOTKEY_RECORDED`] 이벤트로 편집기에 간다.
    pub fn start_recording(&self) {
        eprintln!("녹화 시작");
        self.state.lock().unwrap().recorder = Some(Recorder::new());
        self.hook.start_recording();
        // 훅이 시간 초과로 녹화를 끝내면 편집기에 알린다.
        let state = Arc::clone(&self.state);
        let ui = Arc::clone(&self.ui);
        let hook = Arc::clone(&self.hook);
        spawn_named("deck-record-watch", move || loop {
            thread::sleep(std::time::Duration::from_millis(200));
            let mut s = state.lock().unwrap();
            if s.recorder.is_none() {
                return;
            }
            if !hook.recording() {
                eprintln!("녹화 시간 초과");
                s.recorder = None;
                drop(s);
                apply(
                    &ui,
                    Effects {
                        recorded: Some(RecordResult::Timeout),
                        ..Effects::default()
                    },
                );
                return;
            }
        });
    }

    pub fn cancel_recording(&self) {
        eprintln!("녹화 취소 (편집기)");
        self.hook.stop_recording();
        self.state.lock().unwrap().recorder = None;
    }

    pub fn open_config(&self) {
        self.executor.submit(
            "설정 파일 열기",
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
    hook: &HookHandle,
) {
    for event in events {
        let effects = {
            let mut s = state.lock().unwrap();
            on_hook_event(&mut s, event, executor, hook)
        };
        apply(ui, effects);
    }
}

fn on_hook_event(
    s: &mut State,
    event: HookEvent,
    executor: &Executor,
    hook: &HookHandle,
) -> Effects {
    match event {
        HookEvent::Fire(key) => {
            let State { config, deck, .. } = &mut *s;
            match deck.on_press(key, config) {
                Dispatch::Button { button, .. } => {
                    s.last_button = Some(key);
                    if let Some(button) = button {
                        executor.submit(button.label, button.action);
                    }
                    Effects {
                        osd: Some((s.payload(Some(key), None), Show::For(osd::BUTTON))),
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
            eprintln!("훅 재설치 {count}회 ({reason:?})");
            Effects::default()
        }
        HookEvent::Recorded { key, down } => {
            let outcome = s.recorder.as_mut().and_then(|r| r.on_key(key, down));
            eprintln!(
                "녹화: {} vk=0x{:02X}{} → {:?}",
                if down { "↓" } else { "↑" },
                key.vk,
                if key.extended { " ext" } else { "" },
                outcome
            );
            match outcome {
                Some(outcome) => {
                    hook.stop_recording();
                    s.recorder = None;
                    Effects {
                        recorded: Some(outcome.into()),
                        ..Effects::default()
                    }
                }
                None => Effects::default(),
            }
        }
        HookEvent::Trace(_) => Effects::default(),
    }
}

/// 실행 결과: 실패하면 OSD에 이유를 보여준다.
fn exec_loop(events: Receiver<ExecEvent>, state: &Mutex<State>, ui: &Mutex<Ui>) {
    for event in events {
        if let ExecEvent::Failed { label, error } = event {
            eprintln!("\"{label}\" 실패: {error}");
            let effects = {
                let s = state.lock().unwrap();
                Effects {
                    osd: Some((
                        s.payload(s.last_button, Some(format!("\"{label}\" 실패: {error}"))),
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
    let (tray, osd, app) = {
        let ui = ui.lock().unwrap();
        (ui.tray.clone(), ui.osd.clone(), ui.app.clone())
    };
    if let Some(app) = &app {
        if let Some(config) = effects.config_changed {
            let _ = app.emit_to(crate::editor::LABEL, CONFIG_CHANGED, config);
        }
        if let Some(result) = effects.recorded {
            let _ = app.emit_to(crate::editor::LABEL, HOTKEY_RECORDED, result);
        }
    }
    if let (Some(tray), Some(view)) = (tray, effects.tray) {
        tray.update(&view);
    }
    if let (Some(osd), Some((payload, how))) = (osd, effects.osd) {
        osd.show(payload, how);
    }
}

fn spawn_named(name: &str, f: impl FnOnce() + Send + 'static) {
    thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .expect("스레드를 만들 수 없습니다");
}
