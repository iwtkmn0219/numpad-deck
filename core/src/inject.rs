//! 입력 주입과 자기 입력 판별.
//!
//! 액션 실행기가 `SendInput`으로 보낸 입력은 훅을 다시 지나간다. 이걸 다시 가로채면
//! 무한 반복이 되므로, 보낼 때 `dwExtraInfo`에 서명을 넣고 훅은 서명이 있는
//! 주입 입력(`LLKHF_INJECTED`)을 그대로 통과시킨다.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC,
    VIRTUAL_KEY,
};

use crate::keys::{HotkeyKey, MODIFIER_VKS};

/// `dwExtraInfo`에 넣는 서명. ASCII "NPDK".
pub const SIGNATURE: usize = 0x4E50_444B;

/// 할당되지 않은 가상 키. Alt·Win을 뗄 때 앞에 끼워서, 앱이 "Alt만 눌렀다 뗐다"로
/// 보고 메뉴를 열거나 시작 메뉴가 뜨는 것을 막는다.
const MASK_VK: u16 = 0xE8;

const ALT_VKS: [u16; 2] = [0xA4, 0xA5];
const WIN_VKS: [u16; 2] = [0x5B, 0x5C];
/// 오른쪽 Ctrl·Alt, Win 키는 확장 키다.
const EXTENDED_MODIFIERS: [u16; 4] = [0xA3, 0xA5, 0x5B, 0x5C];

/// 이 앱이 주입한 입력인지. `LLKHF_INJECTED`와 함께 확인해야 한다.
pub fn is_ours(extra_info: usize) -> bool {
    extra_info == SIGNATURE
}

/// 키 하나를 눌렀다 뗀다.
pub fn tap_vk(vk: u16, extended: bool) -> windows::core::Result<()> {
    let key = HotkeyKey { vk, extended };
    send(&[key_input(key, false), key_input(key, true)])
}

/// 단축키를 보낸다. 적힌 순서대로 누르고 거꾸로 뗀다.
///
/// 사용자가 누르고 있던 보조 키는 먼저 뗀다. 그대로 두면 `Ctrl+C` 버튼이
/// `Ctrl+Shift+C`로 전송된다. 끝난 뒤 다시 누르지는 않는다. 그 사이 사용자가 손을
/// 뗐다면 다시 누른 키가 눌린 채로 남기 때문이다.
pub fn hotkey(keys: &[HotkeyKey]) -> windows::core::Result<()> {
    let mut inputs = release_held_modifiers();
    // ["Win"]만 적었다면 시작 메뉴를 여는 게 의도이므로 여기에는 마스크 키를 넣지 않는다.
    inputs.extend(keys.iter().map(|&k| key_input(k, false)));
    inputs.extend(keys.iter().rev().map(|&k| key_input(k, true)));
    send(&inputs)
}

/// 글자를 입력한다. 한글 입력기 상태와 무관하게 그대로 입력되도록 유니코드로 보낸다.
/// 줄바꿈과 탭은 앱이 키로 처리하도록 Enter·Tab 키로 보낸다.
///
/// 한 번에 몰아 보내지 않고 [`type_plan`]대로 나눠 사이에 [`TYPE_PAUSE`]만큼 쉰다. 엑셀은 칸의
/// 첫 글자로 편집을 시작하는 사이 뒤 글자가 오면 첫 글자를 앞에서 친 글자로 바꾸고, 새 메모장은
/// Enter와 뒤 글자가 붙어 오면 순서를 섞는다(직접 입력 진단 `deck-probe --type-test`로 실측).
/// 그래서 줄마다 약 90ms(3줄 약 0.2초, 10줄 약 0.9초) 동안 부른 스레드를 막는다.
pub fn text(text: &str) -> windows::core::Result<()> {
    let mut inputs = release_held_modifiers();
    for piece in type_plan(text) {
        match piece {
            TypePiece::Chars(chars) => {
                for c in chars.chars() {
                    let mut units = [0u16; 2];
                    for &unit in c.encode_utf16(&mut units).iter() {
                        inputs.push(unicode_input(unit, false));
                        inputs.push(unicode_input(unit, true));
                    }
                }
            }
            TypePiece::Key(vk) => {
                let key = HotkeyKey {
                    vk,
                    extended: false,
                };
                inputs.push(key_input(key, false));
                inputs.push(key_input(key, true));
            }
            TypePiece::Pause => {
                send(&std::mem::take(&mut inputs))?;
                std::thread::sleep(TYPE_PAUSE);
            }
        }
    }
    send(&inputs)
}

/// [`text`]에서 나눠 보낸 조각 사이에 쉬는 시간. 30ms로 엑셀·새 메모장이 정상이었다.
const TYPE_PAUSE: std::time::Duration = std::time::Duration::from_millis(30);

/// 글자 입력의 한 조각.
#[derive(Debug, PartialEq, Eq)]
enum TypePiece<'a> {
    /// 유니코드로 이어 보내는 글자.
    Chars(&'a str),
    /// 앱이 키로 처리하는 Enter(`0x0D`)·Tab(`0x09`).
    Key(u16),
    /// 앞까지 보내고 쉰다.
    Pause,
}

/// 글자를 보내는 차례. 칸·줄을 바꾸는 Enter·Tab은 앞뒤로 쉬고, 줄(칸)의 첫 글자는 따로 보낸
/// 뒤 쉰다. 나머지 글자는 한 번에 보낸다(`\r`은 버린다).
fn type_plan<'a>(text: &'a str) -> Vec<TypePiece<'a>> {
    let mut plan = Vec::new();
    // 이어 보낼 글자 묶음의 시작 위치.
    let mut run: Option<usize> = None;
    let mut line_start = true;
    let flush = |plan: &mut Vec<TypePiece<'a>>, run: &mut Option<usize>, end: usize| {
        if let Some(start) = run.take() {
            plan.push(TypePiece::Chars(&text[start..end]));
        }
    };
    for (i, c) in text.char_indices() {
        match c {
            '\r' => flush(&mut plan, &mut run, i),
            '\n' | '\t' => {
                flush(&mut plan, &mut run, i);
                let vk = if c == '\n' { 0x0D } else { 0x09 };
                if plan.last() != Some(&TypePiece::Pause) {
                    plan.push(TypePiece::Pause);
                }
                plan.extend([TypePiece::Key(vk), TypePiece::Pause]);
                line_start = true;
            }
            _ if line_start => {
                plan.extend([
                    TypePiece::Chars(&text[i..i + c.len_utf8()]),
                    TypePiece::Pause,
                ]);
                line_start = false;
            }
            _ => {
                run.get_or_insert(i);
            }
        }
    }
    flush(&mut plan, &mut run, text.len());
    // 처음·끝의 쉼은 기다릴 것이 없다.
    if plan.first() == Some(&TypePiece::Pause) {
        plan.remove(0);
    }
    while plan.last() == Some(&TypePiece::Pause) {
        plan.pop();
    }
    plan
}

/// 붙여넣기(Ctrl+V)를 보낸다. 사용자가 누르고 있던 보조 키는 먼저 뗀다. 그대로 두면
/// Shift가 섞여 `Ctrl+Shift+V`(서식 없이 붙여넣기 등 다른 동작)가 된다.
pub fn paste_shortcut() -> windows::core::Result<()> {
    let ctrl = HotkeyKey {
        vk: 0xA2, // VK_LCONTROL
        extended: false,
    };
    let v = HotkeyKey {
        vk: 0x56, // 'V'
        extended: false,
    };
    let mut inputs = release_held_modifiers();
    inputs.extend([
        key_input(ctrl, false),
        key_input(v, false),
        key_input(v, true),
        key_input(ctrl, true),
    ]);
    send(&inputs)
}

/// 지금 눌려 있는 보조 키를 떼는 입력. Alt·Win이 있으면 마스크 키를 앞에 넣는다.
fn release_held_modifiers() -> Vec<INPUT> {
    let held: Vec<u16> = MODIFIER_VKS
        .iter()
        .copied()
        .filter(|&vk| unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000 != 0)
        .collect();
    let mut inputs = Vec::new();
    if held.iter().any(|&vk| needs_mask(vk)) {
        inputs.extend(mask_tap());
    }
    inputs.extend(held.into_iter().map(|vk| {
        key_input(
            HotkeyKey {
                vk,
                extended: EXTENDED_MODIFIERS.contains(&vk),
            },
            true,
        )
    }));
    inputs
}

fn needs_mask(vk: u16) -> bool {
    ALT_VKS.contains(&vk) || WIN_VKS.contains(&vk)
}

fn mask_tap() -> [INPUT; 2] {
    let mask = HotkeyKey {
        vk: MASK_VK,
        extended: false,
    };
    [key_input(mask, false), key_input(mask, true)]
}

fn key_input(key: HotkeyKey, up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if key.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    // 스캔 코드도 채워야 스캔 코드를 읽는 앱(게임, 원격 데스크톱 등)에서 동작한다.
    let scan = unsafe { MapVirtualKeyW(key.vk as u32, MAPVK_VK_TO_VSC) } as u16;
    keyboard_input(key.vk, scan, flags)
}

fn unicode_input(unit: u16, up: bool) -> INPUT {
    let mut flags = KEYEVENTF_UNICODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    keyboard_input(0, unit, flags)
}

fn keyboard_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SIGNATURE,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> windows::core::Result<()> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        // UIPI로 막히면 여기로 온다 (관리자 권한 창이 포그라운드일 때).
        Err(windows::core::Error::from_thread())
    }
}

#[cfg(test)]
mod tests {
    use super::TypePiece::{Chars, Key, Pause};
    use super::*;

    #[test]
    fn single_line_sends_first_char_alone() {
        assert_eq!(
            type_plan("안녕하세요."),
            [Chars("안"), Pause, Chars("녕하세요.")]
        );
        assert_eq!(type_plan("a"), [Chars("a")]);
        assert_eq!(type_plan(""), []);
    }

    #[test]
    fn enter_and_tab_are_keys_with_pauses_around() {
        assert_eq!(
            type_plan("ab\ncd\tef"),
            [
                Chars("a"),
                Pause,
                Chars("b"),
                Pause,
                Key(0x0D),
                Pause,
                Chars("c"),
                Pause,
                Chars("d"),
                Pause,
                Key(0x09),
                Pause,
                Chars("e"),
                Pause,
                Chars("f"),
            ]
        );
    }

    #[test]
    fn carriage_returns_are_dropped() {
        assert_eq!(
            type_plan("ab\r\ncd\r"),
            [
                Chars("a"),
                Pause,
                Chars("b"),
                Pause,
                Key(0x0D),
                Pause,
                Chars("c"),
                Pause,
                Chars("d")
            ]
        );
        assert_eq!(type_plan("a\rb"), [Chars("a"), Pause, Chars("b")]);
        assert_eq!(type_plan("\r"), []);
        assert_eq!(
            type_plan("\t가나"),
            [Key(0x09), Pause, Chars("가"), Pause, Chars("나")]
        );
    }

    #[test]
    fn empty_lines_and_edges_have_no_double_or_edge_pauses() {
        assert_eq!(
            type_plan("\na\n\n"),
            [
                Key(0x0D),
                Pause,
                Chars("a"),
                Pause,
                Key(0x0D),
                Pause,
                Key(0x0D)
            ]
        );
    }

    #[test]
    fn first_char_keeps_surrogate_pairs_whole() {
        assert_eq!(type_plan("😀x"), [Chars("😀"), Pause, Chars("x")]);
    }

    #[test]
    fn plan_keeps_every_char_in_order() {
        let text = "안녕\r\n주문 (2026-10-18 02:40)\t😀끝\n";
        let mut rebuilt = String::new();
        for piece in type_plan(text) {
            match piece {
                Chars(s) => rebuilt.push_str(s),
                Key(0x0D) => rebuilt.push('\n'),
                Key(_) => rebuilt.push('\t'),
                Pause => {}
            }
        }
        assert_eq!(rebuilt, text.replace('\r', ""));
    }
}
