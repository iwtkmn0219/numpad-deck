//! 단축키 녹화: 녹화 모드에서 훅이 넘겨준 키 이벤트를 단축키 이름 배열로 만든다.
//!
//! - 보조 키(Ctrl·Shift·Alt·Win)를 누른 채 다른 키를 누르면 끝난다: `["Ctrl", "Shift", "S"]`.
//! - 보조 키만 눌렀다 모두 떼면 그 보조 키들로 끝난다: `["Win"]`.
//! - 보조 키 없이 Esc만 누르면 취소.
//! - 넘패드 키(덱 키)와 이름이 없는 키는 녹화하지 않는다.
//!
//! 왼쪽·오른쪽 보조 키는 구분하지 않는다. 보조 키는 항상 Ctrl, Shift, Alt, Win 순서로 적는다.

use crate::key::{identify, vk, RawKey};
use crate::keys;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Keys(Vec<String>),
    Cancelled,
    Rejected(Rejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// 넘패드 키는 덱 키라 단축키로 쓸 수 없다.
    NumpadKey,
    /// 이름이 없는 키 (특수 키 등)
    UnknownKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Modifier {
    Ctrl,
    Shift,
    Alt,
    Win,
}

impl Modifier {
    fn from_vk(code: u16) -> Option<Modifier> {
        match code {
            0x11 | 0xA2 | 0xA3 => Some(Modifier::Ctrl),
            vk::SHIFT | vk::LSHIFT | vk::RSHIFT => Some(Modifier::Shift),
            0x12 | 0xA4 | 0xA5 => Some(Modifier::Alt),
            0x5B | 0x5C => Some(Modifier::Win),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Modifier::Ctrl => "Ctrl",
            Modifier::Shift => "Shift",
            Modifier::Alt => "Alt",
            Modifier::Win => "Win",
        }
    }
}

const ESC: u16 = 0x1B;

#[derive(Debug, Default)]
pub struct Recorder {
    /// 지금 눌려 있는 보조 키
    held: Vec<Modifier>,
    /// 이번 녹화에서 눌렀던 보조 키 전부 (보조 키만으로 끝날 때 쓴다)
    pressed: Vec<Modifier>,
}

impl Recorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// 키 이벤트 하나를 넣는다. 녹화가 끝나면 결과를 돌려준다.
    pub fn on_key(&mut self, raw: RawKey, down: bool) -> Option<Outcome> {
        if let Some(m) = Modifier::from_vk(raw.vk) {
            if down {
                if !self.held.contains(&m) {
                    self.held.push(m);
                }
                if !self.pressed.contains(&m) {
                    self.pressed.push(m);
                }
                return None;
            }
            self.held.retain(|&h| h != m);
            if self.held.is_empty() && !self.pressed.is_empty() {
                return Some(Outcome::Keys(names(&self.pressed, None)));
            }
            return None;
        }
        if !down {
            return None;
        }
        if raw.vk == ESC && self.held.is_empty() {
            return Some(Outcome::Cancelled);
        }
        if identify(raw).is_some() || raw.vk == vk::NUMLOCK {
            return Some(Outcome::Rejected(Rejection::NumpadKey));
        }
        match keys::name_of(raw.vk) {
            Some(name) => Some(Outcome::Keys(names(&self.held, Some(name)))),
            None => Some(Outcome::Rejected(Rejection::UnknownKey)),
        }
    }
}

fn names(mods: &[Modifier], key: Option<String>) -> Vec<String> {
    let mut sorted = mods.to_vec();
    sorted.sort();
    sorted
        .into_iter()
        .map(|m| m.name().to_string())
        .chain(key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(r: &mut Recorder, code: u16, extended: bool, down: bool) -> Option<Outcome> {
        r.on_key(RawKey { vk: code, extended }, down)
    }

    fn keys(list: &[&str]) -> Option<Outcome> {
        Some(Outcome::Keys(list.iter().map(|s| s.to_string()).collect()))
    }

    #[test]
    fn modifiers_then_key() {
        let mut r = Recorder::new();
        assert_eq!(press(&mut r, 0xA0, false, true), None); // LShift
        assert_eq!(press(&mut r, 0xA2, false, true), None); // LCtrl
        assert_eq!(
            press(&mut r, b'S' as u16, false, true),
            keys(&["Ctrl", "Shift", "S"])
        );
    }

    #[test]
    fn right_modifiers_are_generic() {
        let mut r = Recorder::new();
        press(&mut r, 0xA3, true, true); // RCtrl
        assert_eq!(press(&mut r, 0x70, false, true), keys(&["Ctrl", "F1"]));
    }

    #[test]
    fn modifier_only() {
        let mut r = Recorder::new();
        press(&mut r, 0x5B, true, true); // LWin
        assert_eq!(press(&mut r, 0x5B, true, false), keys(&["Win"]));
    }

    #[test]
    fn modifier_only_combination_ends_when_all_released() {
        let mut r = Recorder::new();
        press(&mut r, 0xA2, false, true);
        press(&mut r, 0xA4, false, true);
        assert_eq!(press(&mut r, 0xA2, false, false), None);
        assert_eq!(press(&mut r, 0xA4, false, false), keys(&["Ctrl", "Alt"]));
    }

    #[test]
    fn single_key() {
        let mut r = Recorder::new();
        assert_eq!(press(&mut r, 0x2C, true, true), keys(&["PrintScreen"]));
    }

    #[test]
    fn esc_cancels_but_ctrl_esc_records() {
        let mut r = Recorder::new();
        assert_eq!(press(&mut r, ESC, false, true), Some(Outcome::Cancelled));
        let mut r = Recorder::new();
        press(&mut r, 0xA2, false, true);
        press(&mut r, 0xA0, false, true);
        assert_eq!(
            press(&mut r, ESC, false, true),
            keys(&["Ctrl", "Shift", "Esc"])
        );
    }

    #[test]
    fn numpad_keys_are_rejected_but_navigation_keys_are_not() {
        let mut r = Recorder::new();
        // 확장 플래그 없는 Home = 넘패드 7
        assert_eq!(
            press(&mut r, vk::HOME, false, true),
            Some(Outcome::Rejected(Rejection::NumpadKey))
        );
        let mut r = Recorder::new();
        assert_eq!(press(&mut r, vk::HOME, true, true), keys(&["Home"]));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let mut r = Recorder::new();
        assert_eq!(
            press(&mut r, 0xB3, true, true), // 미디어 재생 키
            Some(Outcome::Rejected(Rejection::UnknownKey))
        );
    }

    #[test]
    fn recorded_names_parse_back() {
        let mut r = Recorder::new();
        press(&mut r, 0xA2, false, true);
        let Some(Outcome::Keys(names)) = press(&mut r, 0xBF, false, true) else {
            panic!()
        };
        assert!(keys::parse_all(&names).is_ok(), "{names:?}");
    }
}
