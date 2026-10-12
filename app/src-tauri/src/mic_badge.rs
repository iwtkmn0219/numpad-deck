//! 마이크 꺼짐 배지: 마이크가 꺼져 있는 동안 주 모니터 위쪽 가운데에 계속 보인다.
//!
//! OSD처럼 포커스를 뺏지 않고 클릭이 통과한다 (`osd.rs` 참고). 마이크 버튼이 없는
//! 사용자에게 WebView 메모리를 쓰지 않도록 처음 보여야 할 때 창을 만들고, 그 뒤로는
//! 숨기고 보이기만 한다. 글자는 화면이 `lang://changed`로 직접 바꾼다.

use std::sync::Mutex;

use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, SetWindowPos, ShowWindowAsync,
    GWL_EXSTYLE, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SW_HIDE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

const LABEL: &str = "mic-badge";
/// 작업 영역 위에서 띄우는 거리 (물리 픽셀)
const MARGIN: i32 = 12;

pub struct MicBadge {
    app: AppHandle,
    /// 만든 창의 `HWND` (스레드 사이로 옮기려고 정수). 아직 안 만들었으면 `None`.
    hwnd: Mutex<Option<isize>>,
}

impl MicBadge {
    pub fn new(app: &AppHandle) -> MicBadge {
        MicBadge {
            app: app.clone(),
            hwnd: Mutex::new(None),
        }
    }

    /// 보이거나 숨긴다. 처음 보일 때 창을 만든다 (메인 스레드를 기다리므로 상태 잠금을
    /// 쥔 채 부르지 말 것 — `runtime::apply`에서만 부른다).
    pub fn set_visible(&self, visible: bool) {
        let mut slot = self.hwnd.lock().unwrap();
        if slot.is_none() {
            if !visible {
                return;
            }
            match create(&self.app) {
                Ok(h) => *slot = Some(h),
                Err(e) => {
                    eprintln!("마이크 배지 창을 만들 수 없습니다: {e}");
                    return;
                }
            }
        }
        let hwnd = slot.expect("위에서 만들었다");
        if visible {
            place_and_show(hwnd);
        } else {
            unsafe {
                let _ = ShowWindowAsync(HWND(hwnd as *mut _), SW_HIDE);
            }
        }
    }
}

fn create(app: &AppHandle) -> tauri::Result<isize> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("mic-badge.html".into()))
        .title("Numpad Deck Mic")
        // 영어 문구 + 아이콘이 들어가는 크기. 투명하고 클릭이 통과한다.
        .inner_size(220.0, 56.0)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .additional_browser_args(crate::osd::BROWSER_ARGS)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    let hwnd = window.hwnd()?.0 as isize;
    unsafe {
        let h = HWND(hwnd as *mut _);
        let style = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(
            h,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize,
        );
    }
    Ok(hwnd)
}

/// 주 모니터 작업 영역의 위쪽 가운데에 띄운다.
fn place_and_show(hwnd: isize) {
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let monitor = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let w = rect.right - rect.left;
        let work = info.rcWork;
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            work.left + (work.right - work.left - w) / 2,
            work.top + MARGIN,
            0,
            0,
            SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
        );
    }
}
