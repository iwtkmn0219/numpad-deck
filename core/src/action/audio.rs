//! 효과음 재생.
//!
//! 출력 장치를 계속 열어 두면 Windows가 "오디오 재생 중"으로 보고 절전에 들어가지
//! 않는다. 그래서 재생할 때만 열고, 모든 소리가 끝난 뒤 [`IDLE_CLOSE`]가 지나면 닫는다.
//! 장치는 전용 스레드가 갖고, 파일 열기와 디코딩은 호출한 쪽에서 해서 오류를 바로
//! 돌려준다.

use std::fs::File;
use std::io::BufReader;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use rodio::source::{Amplify, Source};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};

use super::shell;
use crate::i18n::tr;

const IDLE_CLOSE: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(250);

type Sound = Amplify<Decoder<BufReader<File>>>;

enum Message {
    Play(Sound),
    /// 재생 중인 소리를 모두 멈춘다.
    StopAll,
}

/// 재생 스레드 핸들. 복제해서 여러 스레드(묶음)에서 쓸 수 있다.
#[derive(Clone)]
pub struct Audio {
    tx: Sender<Message>,
}

impl Audio {
    pub fn start() -> Audio {
        let (tx, rx) = mpsc::channel::<Message>();
        let _ = thread::Builder::new()
            .name("deck-audio".into())
            .spawn(move || {
                let mut sink: Option<MixerDeviceSink> = None;
                let mut players: Vec<Player> = Vec::new();
                let mut idle_since = Instant::now();
                loop {
                    match rx.recv_timeout(POLL) {
                        Ok(Message::StopAll) => players.clear(),
                        Ok(Message::Play(sound)) => {
                            if sink.is_none() {
                                match DeviceSinkBuilder::open_default_sink() {
                                    Ok(mut s) => {
                                        s.log_on_drop(false);
                                        sink = Some(s);
                                    }
                                    Err(e) => {
                                        eprintln!("소리 장치를 열 수 없습니다: {e}");
                                        continue;
                                    }
                                }
                            }
                            let player = Player::connect_new(sink.as_ref().unwrap().mixer());
                            player.append(sound);
                            players.push(player);
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    players.retain(|p| !p.empty());
                    if !players.is_empty() {
                        idle_since = Instant::now();
                    } else if sink.is_some() && idle_since.elapsed() >= IDLE_CLOSE {
                        sink = None;
                    }
                }
            });
        Audio { tx }
    }

    /// 파일을 열어 재생을 시작한다. 끝날 때까지 기다리지 않는다. 연타하면 겹쳐 재생한다.
    pub fn play(&self, file: &str, volume: f32) -> Result<(), String> {
        let path = shell::expand_env(file.trim());
        let f = File::open(&path).map_err(|e| {
            tr(
                format!("소리 파일 \"{path}\"을 열 수 없습니다: {e}"),
                format!("Cannot open sound file \"{path}\": {e}"),
            )
        })?;
        let decoder = Decoder::try_from(f).map_err(|e| {
            tr(
                format!("\"{path}\"은 재생할 수 없는 형식입니다: {e}"),
                format!("\"{path}\" is not a playable format: {e}"),
            )
        })?;
        self.tx
            .send(Message::Play(decoder.amplify(volume)))
            .map_err(|_| {
                tr(
                    "소리 재생 스레드가 멈췄습니다.",
                    "The sound thread has stopped.",
                )
            })
    }

    /// 재생 중인 효과음을 모두 멈춘다.
    pub fn stop_all(&self) {
        let _ = self.tx.send(Message::StopAll);
    }
}
