//! v1.0 고정 키 배치.
//!
//! ```text
//! [ / ][ * ][ - ]          / 이전 페이지   * 다음 페이지
//! [ 7 ][ 8 ][ 9 ][ + ]     - OSD 띄우기/숨기기
//! [ 4 ][ 5 ][ 6 ][   ]     . 첫 페이지로
//! [ 1 ][ 2 ][ 3 ][Ent]     나머지 12개는 페이지마다 바뀌는 버튼
//! [   0    ][ . ][   ]
//! ```

use crate::key::NumpadKey;

/// 페이지당 버튼 수.
pub const BUTTONS_PER_PAGE: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// 현재 페이지의 버튼. 값은 설정 파일의 버튼 순서(0..12).
    Button(usize),
    PrevPage,
    NextPage,
    ToggleOsd,
    FirstPage,
}

/// 버튼 순서. 위에서 아래, 왼쪽에서 오른쪽으로 읽는다.
pub const BUTTON_KEYS: [NumpadKey; BUTTONS_PER_PAGE] = [
    NumpadKey::N7,
    NumpadKey::N8,
    NumpadKey::N9,
    NumpadKey::Add,
    NumpadKey::N4,
    NumpadKey::N5,
    NumpadKey::N6,
    NumpadKey::N1,
    NumpadKey::N2,
    NumpadKey::N3,
    NumpadKey::Enter,
    NumpadKey::N0,
];

pub fn role(key: NumpadKey) -> Role {
    match key {
        NumpadKey::Divide => Role::PrevPage,
        NumpadKey::Multiply => Role::NextPage,
        NumpadKey::Subtract => Role::ToggleOsd,
        NumpadKey::Decimal => Role::FirstPage,
        _ => {
            let index = BUTTON_KEYS
                .iter()
                .position(|&k| k == key)
                .expect("시스템 키가 아니면 버튼이다");
            Role::Button(index)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_buttons_and_four_system_keys() {
        let roles: Vec<Role> = NumpadKey::ALL.iter().map(|&k| role(k)).collect();
        let buttons = roles
            .iter()
            .filter(|r| matches!(r, Role::Button(_)))
            .count();
        assert_eq!(buttons, BUTTONS_PER_PAGE);
        assert_eq!(roles.len() - buttons, 4);
    }

    #[test]
    fn button_indices_round_trip() {
        for (i, &key) in BUTTON_KEYS.iter().enumerate() {
            assert_eq!(role(key), Role::Button(i));
        }
    }

    #[test]
    fn system_keys() {
        assert_eq!(role(NumpadKey::Divide), Role::PrevPage);
        assert_eq!(role(NumpadKey::Multiply), Role::NextPage);
        assert_eq!(role(NumpadKey::Subtract), Role::ToggleOsd);
        assert_eq!(role(NumpadKey::Decimal), Role::FirstPage);
    }
}
