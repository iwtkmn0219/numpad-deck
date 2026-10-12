//! 단축키 액션에 쓰는 키 이름 ↔ 가상 키.
//!
//! 설정 파일에는 `["Ctrl", "Shift", "S"]`처럼 사람이 읽는 이름으로 적는다. 대소문자는
//! 가리지 않고, 흔한 별칭(`Control`, `Escape`, `PgUp` 등)도 받는다. 3단계 편집기는
//! 단축키를 녹화해 [`name_of`]가 돌려주는 이름으로 저장한다.

/// 단축키를 이루는 키 하나.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyKey {
    pub vk: u16,
    /// 주입할 때 `KEYEVENTF_EXTENDEDKEY`를 붙여야 하는 키. 붙이지 않으면 이동 키가
    /// 넘패드 키로 해석된다.
    pub extended: bool,
}

impl HotkeyKey {
    pub fn is_modifier(self) -> bool {
        MODIFIER_VKS.contains(&self.vk)
    }
}

/// 왼쪽·오른쪽 구분 없는 보조 키 이름이 가리키는 VK.
const CTRL: u16 = 0xA2; // VK_LCONTROL
const SHIFT: u16 = 0xA0; // VK_LSHIFT
const ALT: u16 = 0xA4; // VK_LMENU
const WIN: u16 = 0x5B; // VK_LWIN

/// 왼쪽·오른쪽 보조 키 8개.
pub const MODIFIER_VKS: [u16; 8] = [0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0x5B, 0x5C];

/// (이름, VK, 확장 키). 같은 VK가 여러 번 나오면 첫 이름이 대표 이름이다.
const TABLE: &[(&str, u16, bool)] = &[
    ("Ctrl", CTRL, false),
    ("Control", CTRL, false),
    ("LCtrl", 0xA2, false),
    ("RCtrl", 0xA3, true),
    ("Shift", SHIFT, false),
    ("LShift", 0xA0, false),
    ("RShift", 0xA1, false),
    ("Alt", ALT, false),
    ("LAlt", 0xA4, false),
    ("RAlt", 0xA5, true),
    ("Win", WIN, true),
    ("Windows", WIN, true),
    ("LWin", 0x5B, true),
    ("RWin", 0x5C, true),
    ("Esc", 0x1B, false),
    ("Escape", 0x1B, false),
    ("Tab", 0x09, false),
    ("Enter", 0x0D, false),
    ("Return", 0x0D, false),
    ("Space", 0x20, false),
    ("Backspace", 0x08, false),
    ("Delete", 0x2E, true),
    ("Del", 0x2E, true),
    ("Insert", 0x2D, true),
    ("Ins", 0x2D, true),
    ("Home", 0x24, true),
    ("End", 0x23, true),
    ("PageUp", 0x21, true),
    ("PgUp", 0x21, true),
    ("PageDown", 0x22, true),
    ("PgDn", 0x22, true),
    ("Left", 0x25, true),
    ("Up", 0x26, true),
    ("Right", 0x27, true),
    ("Down", 0x28, true),
    ("PrintScreen", 0x2C, true),
    ("PrtSc", 0x2C, true),
    ("Pause", 0x13, false),
    ("CapsLock", 0x14, false),
    ("ScrollLock", 0x91, false),
    ("Menu", 0x5D, true),
    ("Apps", 0x5D, true),
    // 한국어 키보드
    ("HanEng", 0x15, false), // VK_HANGUL
    ("Hanja", 0x19, false),
    // 문장 부호 (US·한국어 배열 기준)
    (";", 0xBA, false),
    ("=", 0xBB, false),
    (",", 0xBC, false),
    ("-", 0xBD, false),
    (".", 0xBE, false),
    ("/", 0xBF, false),
    ("`", 0xC0, false),
    ("[", 0xDB, false),
    ("\\", 0xDC, false),
    ("]", 0xDD, false),
    ("'", 0xDE, false),
];

/// 이름으로 키를 찾는다. 글자 A–Z, 숫자 0–9, F1–F24는 표 밖에서 처리한다.
pub fn parse(name: &str) -> Option<HotkeyKey> {
    let name = name.trim();
    if let Some(&(_, vk, extended)) = TABLE.iter().find(|(n, ..)| n.eq_ignore_ascii_case(name)) {
        return Some(HotkeyKey { vk, extended });
    }
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let c = c.to_ascii_uppercase();
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(HotkeyKey {
                vk: c as u16,
                extended: false,
            });
        }
    }
    let n: u16 = name
        .strip_prefix('F')
        .or_else(|| name.strip_prefix('f'))?
        .parse()
        .ok()?;
    (1..=24).contains(&n).then_some(HotkeyKey {
        vk: 0x70 + n - 1,
        extended: false,
    })
}

/// 가상 키의 대표 이름. 녹화한 단축키를 저장할 때 쓴다.
pub fn name_of(vk: u16) -> Option<String> {
    match vk {
        0x30..=0x39 | 0x41..=0x5A => Some((vk as u8 as char).to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x70 + 1)),
        _ => TABLE
            .iter()
            .find(|&&(_, v, _)| v == vk)
            .map(|(n, ..)| n.to_string()),
    }
}

/// 설정 검증용. 모르는 이름이 있으면 그 이름을 담은 오류 메시지를 돌려준다.
pub fn parse_all(names: &[String]) -> Result<Vec<HotkeyKey>, String> {
    names
        .iter()
        .map(|n| {
            parse(n).ok_or_else(|| {
                crate::i18n::tr(
                    format!("키 이름 \"{n}\"을 알 수 없습니다."),
                    format!("Unknown key name \"{n}\"."),
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_and_aliases() {
        assert_eq!(parse("ctrl"), parse("Control"));
        assert_eq!(parse("Win").unwrap().vk, 0x5B);
        assert!(parse("Win").unwrap().extended);
        assert!(parse("Shift").unwrap().is_modifier());
        assert!(!parse("S").unwrap().is_modifier());
    }

    #[test]
    fn letters_digits_function_keys() {
        assert_eq!(parse("s").unwrap().vk, b'S' as u16);
        assert_eq!(parse("7").unwrap().vk, b'7' as u16);
        assert_eq!(parse("F1").unwrap().vk, 0x70);
        assert_eq!(parse("f24").unwrap().vk, 0x87);
        assert_eq!(parse("F25"), None);
        assert_eq!(parse("F0"), None);
    }

    #[test]
    fn navigation_keys_are_extended() {
        for name in ["Left", "Home", "Delete", "PageDown", "Insert"] {
            assert!(parse(name).unwrap().extended, "{name}");
        }
        assert!(!parse("Esc").unwrap().extended);
    }

    #[test]
    fn unknown_names() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("Ctrl+S"), None);
        assert_eq!(parse("한"), None);
        let err = parse_all(&["Ctrl".into(), "Foo".into()]).unwrap_err();
        assert!(err.contains("\"Foo\""));
    }

    #[test]
    fn names_round_trip() {
        for vk in (0x30..=0x39).chain(0x41..=0x5A).chain(0x70..=0x87) {
            let name = name_of(vk).unwrap();
            assert_eq!(parse(&name).unwrap().vk, vk, "{name}");
        }
        for &(name, vk, _) in TABLE {
            let canonical = name_of(vk).unwrap();
            assert_eq!(parse(&canonical).unwrap().vk, vk, "{name} → {canonical}");
        }
        assert_eq!(name_of(0xA2).as_deref(), Some("Ctrl"));
    }
}
