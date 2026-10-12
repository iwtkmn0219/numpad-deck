//! 사용자에게 보이는 코어 메시지(설정 오류, 액션 실패)의 언어.
//!
//! 오류는 실행기 스레드·설정 감시 스레드 등 여러 곳에서 만들어지므로 언어를 전역으로 둔다.
//! 앱이 시작할 때와 설정의 `language`가 바뀔 때 [`set`]한다. 정하기 전에는 한국어다.

use std::sync::atomic::{AtomicU8, Ordering};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub enum Lang {
    Ko,
    En,
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        0 => Lang::Ko,
        _ => Lang::En,
    }
}

/// 지금 언어에 맞는 쪽.
pub fn tr(ko: impl Into<String>, en: impl Into<String>) -> String {
    match current() {
        Lang::Ko => ko.into(),
        Lang::En => en.into(),
    }
}

/// Windows 표시 언어. 한국어가 아니면 영어.
pub fn system() -> Lang {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        // 주 언어 ID(아래 10비트) 0x12 = 한국어
        if unsafe { GetUserDefaultUILanguage() } & 0x3ff == 0x12 {
            return Lang::Ko;
        }
        Lang::En
    }
    #[cfg(not(windows))]
    Lang::En
}

/// 설정의 `language`(없으면 자동 = Windows 표시 언어).
pub fn resolve(setting: Option<Lang>) -> Lang {
    setting.unwrap_or_else(system)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_setting_wins() {
        assert_eq!(resolve(Some(Lang::En)), Lang::En);
        assert_eq!(resolve(Some(Lang::Ko)), Lang::Ko);
    }

    #[test]
    fn lang_is_lowercase_in_json() {
        assert_eq!(serde_json::to_string(&Lang::En).unwrap(), "\"en\"");
        assert_eq!(serde_json::from_str::<Lang>("\"ko\"").unwrap(), Lang::Ko);
    }
}
