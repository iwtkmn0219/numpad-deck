//! Numpad Deck 코어.
//!
//! - [`key`]: 가상 키 + 확장 플래그로 넘패드 키를 식별한다 (순수 로직).
//! - [`gate`]: 키 다운/업을 가로챌지 통과시킬지 결정한다 (순수 로직).
//! - [`layout`]: v1.0 고정 키 배치 (버튼 12개 + 시스템 키 4개).
//! - [`config`]: 설정 파일(JSON) 스키마, 읽기·쓰기, 변경 감시.
//! - [`deck`]: 현재 페이지와 디스패처 (순수 로직).
//! - [`keys`]: 단축키의 키 이름 ↔ 가상 키 (순수 로직).
//! - [`presets`]: 프리셋 페이지 (한국어·영어), 첫 실행 기본 설정.
//! - [`record`]: 단축키 녹화 결과 만들기 (순수 로직).
//! - [`text_template`]: 문장 자리표시자(`{날짜}` 등) 펼치기 (순수 로직).
//! - [`action`]: 액션 실행기 (Windows 전용).
//! - [`clipboard`]: 문장 붙여넣기용 클립보드 보관·기록 제외 넣기·되돌리기 (Windows 전용).
//! - [`hook`]: 저수준 키보드 훅 전용 스레드 (Windows 전용).
//! - [`inject`]: 서명을 붙인 입력 주입과 자기 입력 판별 (Windows 전용).
//! - [`share`]: 페이지 공유 파일(.npdeck) 형식과 경로 이식성 (Windows 전용).

pub mod config;
pub mod deck;
pub mod gate;
pub mod i18n;
pub mod key;
pub mod keys;
pub mod layout;
pub mod presets;
pub mod record;
pub mod text_template;

#[cfg(windows)]
pub mod action;
#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod hook;
#[cfg(windows)]
pub mod inject;
#[cfg(windows)]
pub mod share;
