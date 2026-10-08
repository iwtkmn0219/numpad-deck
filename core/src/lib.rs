//! Numpad Deck 코어.
//!
//! - [`key`]: 가상 키 + 확장 플래그로 넘패드 키를 식별한다 (순수 로직).
//! - [`gate`]: 키 다운/업을 가로챌지 통과시킬지 결정한다 (순수 로직).
//! - [`layout`]: v1.0 고정 키 배치 (버튼 12개 + 시스템 키 4개).
//! - [`hook`]: 저수준 키보드 훅 전용 스레드 (Windows 전용).
//! - [`inject`]: 서명을 붙인 입력 주입과 자기 입력 판별 (Windows 전용).

pub mod gate;
pub mod key;
pub mod layout;

#[cfg(windows)]
pub mod hook;
#[cfg(windows)]
pub mod inject;
