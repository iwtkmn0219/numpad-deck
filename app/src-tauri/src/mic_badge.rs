//! 마이크 꺼짐 배지: 마이크가 꺼져 있는 동안 주 모니터 위쪽 가운데에 계속 보인다.
//!
//! OSD처럼 포커스를 뺏지 않고 클릭이 통과한다 (`osd.rs` 참고). 마이크 버튼이 없는
//! 사용자에게 WebView 메모리를 쓰지 않도록 처음 보여야 할 때 창을 만들고, 그 뒤로는
//! 숨기고 보이기만 한다. 글자는 화면이 `lang://changed`로 직접 바꾼다.
//!
//! 부르는 쪽이 메인 스레드일 수 있다 (편집기 저장 command). Windows에서 메인 스레드가
//! 창을 만들면 교착하므로 창은 항상 따로 띄운 스레드에서 만들고, 만드는 동안 잠금을
//! 쥐지 않는다.

use std::sync::{Arc, Mutex};

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

#[derive(Default)]
struct Inner {
    /// 만든 창의 `HWND` (스레드 사이로 옮기려고 정수). 아직 안 만들었으면 `None`.
    hwnd: Option<isize>,
    /// 창을 만드는 스레드가 돌고 있다.
    creating: bool,
    /// 마지막으로 요청받은 보임 여부
    wanted: bool,
    /// 창에 실제로 반영한 보임 여부
    applied: Option<bool>,
    /// 마지막으로 받은 순번. 이보다 작거나 같은 요청은 오래된 것이라 무시한다.
    seq: u64,
}

pub struct MicBadge {
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
}

impl MicBadge {
    pub fn new(app: &AppHandle) -> MicBadge {
        MicBadge {
            app: app.clone(),
            inner: Arc::default(),
        }
    }

    /// 보이거나 숨긴다. 어느 스레드에서 불러도 막히지 않는다: 처음 보일 때 창은 따로
    /// 띄운 스레드가 만든다. `seq`가 더 큰 요청만 받아들인다.
    pub fn set_visible(&self, seq: u64, visible: bool) {
        let mut inner = self.inner.lock().unwrap();
        if seq <= inner.seq {
            return;
        }
        inner.seq = seq;
        inner.wanted = visible;
        if let Some(hwnd) = inner.hwnd {
            sync(&mut inner, hwnd);
        } else if visible && !inner.creating {
            inner.creating = true;
            drop(inner);
            let app = self.app.clone();
            let shared = Arc::clone(&self.inner);
            let spawned = std::thread::Builder::new()
                .name("deck-mic-badge".into())
                .spawn(move || {
                    let created = create(&app); // 잠금 없이
                    let mut inner = shared.lock().unwrap();
                    match created {
                        Ok(hwnd) => {
                            inner.hwnd = Some(hwnd);
                            sync(&mut inner, hwnd);
                        }
                        Err(e) => {
                            eprintln!("마이크 배지 창을 만들 수 없습니다: {e}");
                            inner.creating = false;
                        }
                    }
                });
            if spawned.is_err() {
                self.inner.lock().unwrap().creating = false;
            }
        }
    }

    /// 보이는 중이면 위치를 다시 잡는다 (모니터·배율·작업 영역이 바뀐 뒤).
    pub fn reposition(&self) {
        let inner = self.inner.lock().unwrap();
        if let (Some(hwnd), Some(true)) = (inner.hwnd, inner.applied) {
            place_and_show(hwnd);
        }
    }
}

/// 요청받은 상태와 창이 다르면 맞춘다 (비동기 호출이라 잠금 안에서 불러도 막히지 않는다).
fn sync(inner: &mut Inner, hwnd: isize) {
    if inner.applied == Some(inner.wanted) {
        return;
    }
    inner.applied = Some(inner.wanted);
    if inner.wanted {
        place_and_show(hwnd);
    } else {
        unsafe {
            let _ = ShowWindowAsync(HWND(hwnd as *mut _), SW_HIDE);
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
    // 반쯤 만든 창이 남으면 같은 이름으로 다시 만들 수 없으므로 실패하면 지운다
    let hwnd = match window
        .set_ignore_cursor_events(true)
        .and_then(|()| window.hwnd())
    {
        Ok(h) => h.0 as isize,
        Err(e) => {
            let _ = window.destroy();
            return Err(e);
        }
    };
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
