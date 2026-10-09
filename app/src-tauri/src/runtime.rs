//! 코어 연결: 훅 → 덱(현재 페이지) → 액션 실행기, 설정 감시, 트레이 갱신.
//!
//! 훅 스레드는 판정만 하고 채널로 알린다. 여기서 만드는 디스패처 스레드가 그 알림을
//! 받아 페이지를 바꾸거나 실행기에 액션을 넘긴다.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

use deck_core::action::{ExecEvent, Executor};
use deck_core::config::{self, Action, Config, ConfigWatcher};
use deck_core::deck::{Deck, Dispatch};
use deck_core::hook::{self, HookConfig, HookEvent, HookHandle};

use crate::tray::{Mode, Tray, View};

struct State {
    config: Config,
    deck: Deck,
    numlock_on: bool,
    paused: bool,
    config_error: Option<String>,
}

impl State {
    fn view(&self) -> View {
        let mode = match (self.paused, self.numlock_on) {
            (true, _) => Mode::Paused,
            (false, true) => Mode::Numbers,
            (false, false) => Mode::Deck,
        };
        let page = self.deck.page();
        let name = self.config.pages.get(page).map_or("", |p| p.name.as_str());
        View {
            mode,
            page: format!("{}/{} {name}", page + 1, self.config.pages.len()),
            config_error: self.config_error.clone(),
        }
    }
}

pub struct Runtime {
    state: Arc<Mutex<State>>,
    tray: Arc<Mutex<Option<Tray>>>,
    hook: HookHandle,
    executor: Executor,
    config_path: PathBuf,
    _watcher: Option<ConfigWatcher>,
}

impl Runtime {
    /// 설정을 읽고 실행기·훅·설정 감시를 시작한다. 트레이는 [`Runtime::attach_tray`]로
    /// 나중에 붙인다 (트레이는 메인 스레드에서 만들어야 한다).
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
            numlock_on: true,
            paused: false,
            config_error,
        }));
        let tray: Arc<Mutex<Option<Tray>>> = Arc::new(Mutex::new(None));

        let (exec_tx, exec_rx) = mpsc::channel();
        let executor = Executor::start(exec_tx).map_err(|e| e.to_string())?;
        spawn_named("deck-exec-log", move || log_exec_events(exec_rx));

        let watcher = {
            let state = Arc::clone(&state);
            let tray = Arc::clone(&tray);
            config::watch(&config_path, config, move |result| {
                let mut s = state.lock().unwrap();
                match result {
                    Ok(config) => {
                        s.deck.on_config_changed(&config);
                        s.config = config;
                        s.config_error = None;
                    }
                    Err(e) => {
                        eprintln!("설정 오류: {e}");
                        s.config_error = Some(e.to_string());
                    }
                }
                refresh(&tray, &s);
            })
            .map_err(|e| eprintln!("설정 감시를 시작할 수 없습니다: {e}"))
            .ok()
        };

        let (hook_tx, hook_rx) = mpsc::channel();
        let hook = hook::start(HookConfig::default(), hook_tx).map_err(|e| e.to_string())?;
        {
            let state = Arc::clone(&state);
            let tray = Arc::clone(&tray);
            let executor = executor.clone();
            spawn_named("deck-dispatch", move || {
                dispatch_loop(hook_rx, &state, &tray, &executor)
            });
        }

        Ok(Runtime {
            state,
            tray,
            hook,
            executor,
            config_path,
            _watcher: watcher,
        })
    }

    pub fn view(&self) -> View {
        self.state.lock().unwrap().view()
    }

    pub fn attach_tray(&self, tray: Tray) {
        *self.tray.lock().unwrap() = Some(tray);
        refresh(&self.tray, &self.state.lock().unwrap());
    }

    pub fn toggle_pause(&self) {
        let mut s = self.state.lock().unwrap();
        s.paused = !s.paused;
        self.hook.set_paused(s.paused);
        refresh(&self.tray, &s);
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
    tray: &Mutex<Option<Tray>>,
    executor: &Executor,
) {
    for event in events {
        let mut s = state.lock().unwrap();
        match event {
            HookEvent::Fire(key) => {
                let State { config, deck, .. } = &mut *s;
                match deck.on_press(key, config) {
                    Dispatch::Button {
                        button: Some(button),
                        ..
                    } => executor.submit(button.label, button.action),
                    Dispatch::Button { .. } => {}
                    Dispatch::Page { .. } => refresh(tray, &s),
                    // OSD는 2-5에서 붙인다.
                    Dispatch::ToggleOsd => {}
                }
            }
            HookEvent::NumLock(on) => {
                s.numlock_on = on;
                refresh(tray, &s);
            }
            HookEvent::Reinstalled { count, reason } => {
                eprintln!("훅 재설치 {count}회 ({reason:?})");
            }
            HookEvent::Trace(_) => {}
        }
    }
}

fn refresh(tray: &Mutex<Option<Tray>>, state: &State) {
    if let Some(tray) = tray.lock().unwrap().as_ref() {
        tray.update(&state.view());
    }
}

fn log_exec_events(events: Receiver<ExecEvent>) {
    for event in events {
        match event {
            ExecEvent::Failed { label, error } => eprintln!("\"{label}\" 실패: {error}"),
            ExecEvent::Done { .. } | ExecEvent::MicMuted(_) => {}
        }
    }
}

fn spawn_named(name: &str, f: impl FnOnce() + Send + 'static) {
    thread::Builder::new()
        .name(name.into())
        .spawn(f)
        .expect("스레드를 만들 수 없습니다");
}
