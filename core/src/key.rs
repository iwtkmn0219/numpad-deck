//! 넘패드 키 식별.
//!
//! 키보드는 NumLock 상태와 무관하게 같은 스캔 코드를 보내고, 가상 키(VK)는 OS가
//! NumLock 상태를 보고 정한다. 그래서 같은 물리 키가 두 가지 VK로 들어온다.
//!
//! - NumLock ON: `VK_NUMPAD7` 등
//! - NumLock OFF (또는 NumLock ON + Shift): 확장 플래그 없는 `VK_HOME` 등
//!
//! [`identify`]는 두 형태를 모두 같은 [`NumpadKey`]로 돌려준다. 키 다운과 업 사이에
//! NumLock이 바뀌어도 같은 키로 짝지을 수 있어야 하기 때문이다. NumLock 상태 판정은
//! 이 모듈의 책임이 아니다.

/// NumLock을 제외한 넘패드 키 16개.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumpadKey {
    N0,
    N1,
    N2,
    N3,
    N4,
    N5,
    N6,
    N7,
    N8,
    N9,
    Decimal,
    Divide,
    Multiply,
    Subtract,
    Add,
    Enter,
}

impl NumpadKey {
    pub const ALL: [NumpadKey; 16] = [
        NumpadKey::N0,
        NumpadKey::N1,
        NumpadKey::N2,
        NumpadKey::N3,
        NumpadKey::N4,
        NumpadKey::N5,
        NumpadKey::N6,
        NumpadKey::N7,
        NumpadKey::N8,
        NumpadKey::N9,
        NumpadKey::Decimal,
        NumpadKey::Divide,
        NumpadKey::Multiply,
        NumpadKey::Subtract,
        NumpadKey::Add,
        NumpadKey::Enter,
    ];

    /// `ALL` 안에서의 위치. 고정 크기 배열 인덱스로 쓴다.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// 키캡에 적힌 글자.
    pub const fn label(self) -> &'static str {
        match self {
            NumpadKey::N0 => "0",
            NumpadKey::N1 => "1",
            NumpadKey::N2 => "2",
            NumpadKey::N3 => "3",
            NumpadKey::N4 => "4",
            NumpadKey::N5 => "5",
            NumpadKey::N6 => "6",
            NumpadKey::N7 => "7",
            NumpadKey::N8 => "8",
            NumpadKey::N9 => "9",
            NumpadKey::Decimal => ".",
            NumpadKey::Divide => "/",
            NumpadKey::Multiply => "*",
            NumpadKey::Subtract => "-",
            NumpadKey::Add => "+",
            NumpadKey::Enter => "Enter",
        }
    }

    /// NumLock OFF일 때 이 키가 만드는 VK. 확장 플래그 없이 주입하면 넘패드 입력과
    /// 구분되지 않으므로, 자기 입력 재가로채기 테스트에 쓴다.
    pub const fn numlock_off_vk(self) -> u16 {
        match self {
            NumpadKey::N0 => vk::INSERT,
            NumpadKey::N1 => vk::END,
            NumpadKey::N2 => vk::DOWN,
            NumpadKey::N3 => vk::NEXT,
            NumpadKey::N4 => vk::LEFT,
            NumpadKey::N5 => vk::CLEAR,
            NumpadKey::N6 => vk::RIGHT,
            NumpadKey::N7 => vk::HOME,
            NumpadKey::N8 => vk::UP,
            NumpadKey::N9 => vk::PRIOR,
            NumpadKey::Decimal => vk::DELETE,
            NumpadKey::Divide => vk::DIVIDE,
            NumpadKey::Multiply => vk::MULTIPLY,
            NumpadKey::Subtract => vk::SUBTRACT,
            NumpadKey::Add => vk::ADD,
            NumpadKey::Enter => vk::RETURN,
        }
    }
}

/// 훅이 받은 키 정보 중 식별에 필요한 부분.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawKey {
    pub vk: u16,
    /// `LLKHF_EXTENDED`
    pub extended: bool,
}

/// 넘패드 키면 `Some`, 아니면 `None`.
pub fn identify(raw: RawKey) -> Option<NumpadKey> {
    use NumpadKey::*;
    let key = match (raw.vk, raw.extended) {
        // NumLock ON 형태
        (vk::NUMPAD0, _) => N0,
        (vk::NUMPAD1, _) => N1,
        (vk::NUMPAD2, _) => N2,
        (vk::NUMPAD3, _) => N3,
        (vk::NUMPAD4, _) => N4,
        (vk::NUMPAD5, _) => N5,
        (vk::NUMPAD6, _) => N6,
        (vk::NUMPAD7, _) => N7,
        (vk::NUMPAD8, _) => N8,
        (vk::NUMPAD9, _) => N9,
        (vk::DECIMAL, _) => Decimal,

        // NumLock OFF 형태. 전용 이동 키·Insert·Delete는 확장 플래그가 있다.
        (vk::INSERT, false) => N0,
        (vk::END, false) => N1,
        (vk::DOWN, false) => N2,
        (vk::NEXT, false) => N3,
        (vk::LEFT, false) => N4,
        (vk::CLEAR, _) => N5,
        (vk::RIGHT, false) => N6,
        (vk::HOME, false) => N7,
        (vk::UP, false) => N8,
        (vk::PRIOR, false) => N9,
        (vk::DELETE, false) => Decimal,

        // NumLock 영향을 받지 않는 넘패드 전용 VK
        (vk::DIVIDE, _) => Divide,
        (vk::MULTIPLY, _) => Multiply,
        (vk::SUBTRACT, _) => Subtract,
        (vk::ADD, _) => Add,

        // 넘패드 Enter만 확장 플래그가 있다.
        (vk::RETURN, true) => Enter,

        _ => return None,
    };
    Some(key)
}

/// VK만 보고 짐작한 NumLock 상태. 숫자 키 11개에만 의미가 있다.
///
/// `VK_NUMPADx`면 확실히 ON이다. 이동 키 형태면 OFF이거나 "ON + Shift"라서
/// 확정할 수 없다. 1단계에서 `GetKeyState` 결과와 비교하는 진단용이다.
pub fn numlock_hint(raw: RawKey) -> Option<bool> {
    match raw.vk {
        vk::NUMPAD0..=vk::NUMPAD9 | vk::DECIMAL => Some(true),
        vk::INSERT
        | vk::END
        | vk::DOWN
        | vk::NEXT
        | vk::LEFT
        | vk::CLEAR
        | vk::RIGHT
        | vk::HOME
        | vk::UP
        | vk::PRIOR
        | vk::DELETE
            if !raw.extended =>
        {
            Some(false)
        }
        _ => None,
    }
}

/// 이 모듈이 쓰는 가상 키 코드. `windows` 크레이트 없이 테스트할 수 있게 직접 둔다.
pub mod vk {
    pub const CLEAR: u16 = 0x0C;
    pub const RETURN: u16 = 0x0D;
    pub const SHIFT: u16 = 0x10;
    pub const PRIOR: u16 = 0x21;
    pub const NEXT: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const INSERT: u16 = 0x2D;
    pub const DELETE: u16 = 0x2E;
    pub const NUMPAD0: u16 = 0x60;
    pub const NUMPAD1: u16 = 0x61;
    pub const NUMPAD2: u16 = 0x62;
    pub const NUMPAD3: u16 = 0x63;
    pub const NUMPAD4: u16 = 0x64;
    pub const NUMPAD5: u16 = 0x65;
    pub const NUMPAD6: u16 = 0x66;
    pub const NUMPAD7: u16 = 0x67;
    pub const NUMPAD8: u16 = 0x68;
    pub const NUMPAD9: u16 = 0x69;
    pub const MULTIPLY: u16 = 0x6A;
    pub const ADD: u16 = 0x6B;
    pub const SUBTRACT: u16 = 0x6D;
    pub const DECIMAL: u16 = 0x6E;
    pub const DIVIDE: u16 = 0x6F;
    pub const NUMLOCK: u16 = 0x90;
    pub const LSHIFT: u16 = 0xA0;
    pub const RSHIFT: u16 = 0xA1;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(vk: u16, extended: bool) -> RawKey {
        RawKey { vk, extended }
    }

    #[test]
    fn numlock_off_navigation_form_is_numpad_only_without_extended_flag() {
        assert_eq!(identify(raw(vk::HOME, false)), Some(NumpadKey::N7));
        assert_eq!(identify(raw(vk::HOME, true)), None);
        assert_eq!(identify(raw(vk::INSERT, false)), Some(NumpadKey::N0));
        assert_eq!(identify(raw(vk::INSERT, true)), None);
        assert_eq!(identify(raw(vk::DELETE, false)), Some(NumpadKey::Decimal));
        assert_eq!(identify(raw(vk::DELETE, true)), None);
    }

    #[test]
    fn numlock_on_and_off_forms_map_to_same_key() {
        for key in NumpadKey::ALL {
            let off = raw(
                key.numlock_off_vk(),
                key == NumpadKey::Enter || key == NumpadKey::Divide,
            );
            assert_eq!(identify(off), Some(key), "{key:?}");
        }
        assert_eq!(identify(raw(vk::NUMPAD7, false)), Some(NumpadKey::N7));
        assert_eq!(identify(raw(vk::DECIMAL, false)), Some(NumpadKey::Decimal));
    }

    #[test]
    fn enter_needs_extended_flag() {
        assert_eq!(identify(raw(vk::RETURN, true)), Some(NumpadKey::Enter));
        assert_eq!(identify(raw(vk::RETURN, false)), None);
    }

    #[test]
    fn clear_is_numpad_five_either_way() {
        assert_eq!(identify(raw(vk::CLEAR, false)), Some(NumpadKey::N5));
        assert_eq!(identify(raw(vk::CLEAR, true)), Some(NumpadKey::N5));
    }

    #[test]
    fn unrelated_keys_are_ignored() {
        assert_eq!(identify(raw(vk::SHIFT, false)), None);
        assert_eq!(identify(raw(vk::NUMLOCK, true)), None);
        assert_eq!(identify(raw(0x41, false)), None); // 'A'
    }

    #[test]
    fn hint_only_for_digit_keys() {
        assert_eq!(numlock_hint(raw(vk::NUMPAD7, false)), Some(true));
        assert_eq!(numlock_hint(raw(vk::HOME, false)), Some(false));
        assert_eq!(numlock_hint(raw(vk::HOME, true)), None);
        assert_eq!(numlock_hint(raw(vk::ADD, false)), None);
        assert_eq!(numlock_hint(raw(vk::RETURN, true)), None);
    }

    #[test]
    fn indices_are_dense() {
        for (i, key) in NumpadKey::ALL.iter().enumerate() {
            assert_eq!(key.index(), i);
        }
    }
}
