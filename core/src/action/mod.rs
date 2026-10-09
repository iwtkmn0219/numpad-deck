//! 액션 실행기.
//!
//! 훅 스레드와 분리된 전용 스레드에서 큐에 들어온 액션을 하나씩 실행한다. 묶음
//! (`sequence`)은 지연이 길 수 있으므로 별도 스레드에서 돌려, 그동안 누른 다른 버튼이
//! 기다리지 않게 한다. 액션이 실패해도 실행기는 계속 돌고, 실패는 이벤트로 알린다.

mod audio;
pub mod mic;
mod shell;
mod window;

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

use crate::config::{Action, MediaCommand};
use crate::inject;
use crate::keys;

pub use audio::Audio;

#[derive(Debug, Clone)]
pub enum ExecEvent {
    Done {
        label: String,
    },
    Failed {
        label: String,
        error: String,
    },
    /// 마이크 음소거를 바꿨다. OSD 표시용.
    MicMuted(bool),
}

struct Job {
    label: String,
    action: Action,
}

/// 실행기 핸들. 버리면 큐가 닫히고 스레드가 끝난다.
pub struct Executor {
    jobs: Sender<Job>,
}

impl Executor {
    pub fn start(events: Sender<ExecEvent>) -> io::Result<Executor> {
        let (jobs, rx) = mpsc::channel();
        thread::Builder::new()
            .name("deck-action".into())
            .spawn(move || worker(rx, events))?;
        Ok(Executor { jobs })
    }

    pub fn submit(&self, label: impl Into<String>, action: Action) {
        let _ = self.jobs.send(Job {
            label: label.into(),
            action,
        });
    }
}

fn worker(jobs: Receiver<Job>, events: Sender<ExecEvent>) {
    let _com = ComGuard::init();
    let audio = Audio::start();
    for job in jobs {
        match job.action {
            Action::Sequence { steps } => {
                let events = events.clone();
                let audio = audio.clone();
                let spawned =
                    thread::Builder::new()
                        .name("deck-sequence".into())
                        .spawn(move || {
                            let _com = ComGuard::init();
                            let result = steps
                                .iter()
                                .try_for_each(|step| run_one(step, &audio, &events));
                            report(&events, job.label, result);
                        });
                if let Err(e) = spawned {
                    eprintln!("묶음 스레드를 만들 수 없습니다: {e}");
                }
            }
            action => {
                let result = run_one(&action, &audio, &events);
                report(&events, job.label, result);
            }
        }
    }
}

fn report(events: &Sender<ExecEvent>, label: String, result: Result<(), String>) {
    let _ = events.send(match result {
        Ok(()) => ExecEvent::Done { label },
        Err(error) => ExecEvent::Failed { label, error },
    });
}

/// 액션 하나를 실행한다. 묶음 안의 단계도 여기로 온다.
fn run_one(action: &Action, audio: &Audio, events: &Sender<ExecEvent>) -> Result<(), String> {
    match action {
        Action::Hotkey { keys } => {
            let keys = keys::parse_all(keys)?;
            inject::hotkey(&keys).map_err(blocked)
        }
        Action::Text { text } => inject::text(text).map_err(blocked),
        Action::Media { command } => inject::tap_vk(media_vk(*command), true).map_err(blocked),
        Action::Open { target } => shell::open(target),
        Action::Launch { target, args } => window::launch_or_focus(target, args.as_deref()),
        Action::MicMute => {
            let muted = mic::toggle_mute()?;
            let _ = events.send(ExecEvent::MicMuted(muted));
            Ok(())
        }
        Action::Sound { file, volume } => audio.play(file, volume.unwrap_or(1.0)),
        Action::Delay { ms } => {
            thread::sleep(Duration::from_millis(*ms));
            Ok(())
        }
        Action::Sequence { .. } => Err("묶음 안에 다시 묶음을 넣을 수 없습니다.".into()),
    }
}

/// `SendInput`이 실패하는 경우는 사실상 관리자 권한 창에 막힌 경우뿐이다.
fn blocked(e: windows::core::Error) -> String {
    format!("입력을 보내지 못했습니다. 관리자 권한으로 실행 중인 창에는 보낼 수 없습니다. ({e})")
}

fn media_vk(command: MediaCommand) -> u16 {
    match command {
        MediaCommand::VolumeMute => 0xAD,
        MediaCommand::VolumeDown => 0xAE,
        MediaCommand::VolumeUp => 0xAF,
        MediaCommand::Next => 0xB0,
        MediaCommand::Prev => 0xB1,
        MediaCommand::Stop => 0xB2,
        MediaCommand::PlayPause => 0xB3,
    }
}

/// 이 스레드에 COM을 초기화한 채로 `f`를 실행한다. 실행기 밖에서 [`mic`] 진단 함수를
/// 부를 때 쓴다.
pub fn with_com<T>(f: impl FnOnce() -> T) -> T {
    let _com = ComGuard::init();
    f()
}

/// 이 스레드에서 COM을 쓴다. 셸 실행과 Core Audio에 필요하다.
struct ComGuard(bool);

impl ComGuard {
    fn init() -> Self {
        let ok = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
        ComGuard(ok)
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}
