//! 키 다운/업을 가로챌지 통과시킬지 결정한다.
//!
//! 규칙:
//! - 덱 모드(NumLock OFF + 넘패드 키)의 키 다운만 가로채고 버튼을 실행한다.
//! - 키 다운을 막았으면 그 키의 업도 막고, 통과시켰으면 업도 통과시킨다.
//!   키를 누른 채 NumLock이 바뀌어도 어긋나지 않도록 "가로챈 키"를 기록해 두고
//!   업은 그 기록만 보고 판정한다.
//! - 누르고 있는 동안 반복되는 다운은 막되 버튼을 다시 실행하지 않는다.
//!
//! 훅 콜백 안에서 불리므로 할당이나 블로킹 없이 끝나야 한다.

use crate::key::NumpadKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// 손대지 않고 다음 훅으로 넘긴다.
    Pass,
    /// 막는다. 버튼은 실행하지 않는다 (자동 반복, 짝이 되는 키 업).
    Swallow,
    /// 막고 버튼을 실행한다.
    Fire(NumpadKey),
}

impl Decision {
    pub fn swallows(self) -> bool {
        !matches!(self, Decision::Pass)
    }
}

/// 이 시간보다 오래 다운이 끊겼다가 다시 오면 자동 반복이 아니라 새로 누른 것으로 본다.
///
/// Windows 자동 반복은 첫 지연이 최대 1초, 이후 간격은 그보다 짧다. 키 업을 놓친
/// 경우(보안 데스크톱 전환 등)에 키가 영원히 "눌린 상태"로 남지 않게 하는 안전장치다.
pub const STALE_HOLD_MS: u32 = 1500;

#[derive(Debug, Default)]
pub struct KeyGate {
    /// 가로챈 키마다 마지막 다운 시각 (`KBDLLHOOKSTRUCT::time`, ms).
    held: [Option<u32>; 16],
}

impl KeyGate {
    pub fn new() -> Self {
        Self::default()
    }

    /// 키 다운. `key`가 `None`이면 넘패드 키가 아니다.
    pub fn on_down(&mut self, key: Option<NumpadKey>, numlock_on: bool, time_ms: u32) -> Decision {
        let Some(key) = key else {
            return Decision::Pass;
        };
        let slot = &mut self.held[key.index()];
        if let Some(last) = *slot {
            if time_ms.wrapping_sub(last) <= STALE_HOLD_MS {
                *slot = Some(time_ms);
                return Decision::Swallow;
            }
        }
        if numlock_on {
            *slot = None;
            return Decision::Pass;
        }
        *slot = Some(time_ms);
        Decision::Fire(key)
    }

    /// 키 업. 다운을 가로챈 키만 막는다.
    pub fn on_up(&mut self, key: Option<NumpadKey>) -> Decision {
        let Some(key) = key else {
            return Decision::Pass;
        };
        match self.held[key.index()].take() {
            Some(_) => Decision::Swallow,
            None => Decision::Pass,
        }
    }

    /// 지금 가로채고 있는 키가 있는지.
    pub fn is_holding(&self) -> bool {
        self.held.iter().any(Option::is_some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use NumpadKey::*;

    #[test]
    fn deck_mode_press_fires_once_and_swallows_up() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(Some(N7), false, 0), Decision::Fire(N7));
        assert_eq!(g.on_up(Some(N7)), Decision::Swallow);
        assert!(!g.is_holding());
    }

    #[test]
    fn numlock_on_passes_both_edges() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(Some(N7), true, 0), Decision::Pass);
        assert_eq!(g.on_up(Some(N7)), Decision::Pass);
    }

    #[test]
    fn non_numpad_keys_pass() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(None, false, 0), Decision::Pass);
        assert_eq!(g.on_up(None), Decision::Pass);
    }

    #[test]
    fn auto_repeat_is_swallowed_without_firing() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(Some(Add), false, 0), Decision::Fire(Add));
        assert_eq!(g.on_down(Some(Add), false, 500), Decision::Swallow);
        assert_eq!(g.on_down(Some(Add), false, 533), Decision::Swallow);
        assert_eq!(g.on_up(Some(Add)), Decision::Swallow);
        assert_eq!(g.on_down(Some(Add), false, 600), Decision::Fire(Add));
    }

    #[test]
    fn numlock_turned_on_while_held_still_swallows_up() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(Some(N8), false, 0), Decision::Fire(N8));
        // NumLock ON이 된 뒤의 반복 다운도 같은 누름의 일부다.
        assert_eq!(g.on_down(Some(N8), true, 500), Decision::Swallow);
        assert_eq!(g.on_up(Some(N8)), Decision::Swallow);
    }

    #[test]
    fn numlock_turned_off_while_held_passes_up() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(Some(N8), true, 0), Decision::Pass);
        assert_eq!(g.on_up(Some(N8)), Decision::Pass);
    }

    #[test]
    fn missed_key_up_recovers_after_stale_period() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(Some(N1), false, 0), Decision::Fire(N1));
        // 키 업을 놓쳤다.
        assert_eq!(
            g.on_down(Some(N1), false, STALE_HOLD_MS + 1),
            Decision::Fire(N1)
        );
    }

    #[test]
    fn timer_wraparound_is_handled() {
        let mut g = KeyGate::new();
        assert_eq!(
            g.on_down(Some(N1), false, u32::MAX - 10),
            Decision::Fire(N1)
        );
        assert_eq!(g.on_down(Some(N1), false, 20), Decision::Swallow);
    }

    #[test]
    fn keys_are_tracked_independently() {
        let mut g = KeyGate::new();
        assert_eq!(g.on_down(Some(N1), false, 0), Decision::Fire(N1));
        assert_eq!(g.on_down(Some(N2), false, 10), Decision::Fire(N2));
        assert_eq!(g.on_up(Some(N1)), Decision::Swallow);
        assert!(g.is_holding());
        assert_eq!(g.on_up(Some(N2)), Decision::Swallow);
        assert!(!g.is_holding());
    }
}
