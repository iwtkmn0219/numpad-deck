//! 입력 주입과 자기 입력 판별.
//!
//! 액션 실행기가 `SendInput`으로 보낸 입력은 훅을 다시 지나간다. 이걸 다시 가로채면
//! 무한 반복이 되므로, 보낼 때 `dwExtraInfo`에 서명을 넣고 훅은 서명이 있는
//! 주입 입력(`LLKHF_INJECTED`)을 그대로 통과시킨다.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};

/// `dwExtraInfo`에 넣는 서명. ASCII "NPDK".
pub const SIGNATURE: usize = 0x4E50_444B;

/// 이 앱이 주입한 입력인지. `LLKHF_INJECTED`와 함께 확인해야 한다.
pub fn is_ours(extra_info: usize) -> bool {
    extra_info == SIGNATURE
}

/// 키 하나를 눌렀다 뗀다.
pub fn tap_vk(vk: u16, extended: bool) -> windows::core::Result<()> {
    let base = if extended {
        KEYEVENTF_EXTENDEDKEY
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    let inputs = [key_input(vk, base), key_input(vk, base | KEYEVENTF_KEYUP)];
    send(&inputs)
}

fn key_input(vk: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SIGNATURE,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> windows::core::Result<()> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        // UIPI로 막히면 여기로 온다 (관리자 권한 창이 포그라운드일 때).
        Err(windows::core::Error::from_thread())
    }
}
