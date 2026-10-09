//! OSD: 넘패드 모양 그리드를 화면 우측 하단에 잠깐 띄운다.
//!
//! 포커스를 뺏지 않고 클릭은 아래 창으로 통과해야 한다. 그래서 창 하나를 미리 만들어
//! 숨겨 두고, Win32로 직접 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`를 붙인 뒤
//! `SWP_NOACTIVATE`로 띄운다. 클릭 통과(`WS_EX_TRANSPARENT | WS_EX_LAYERED`)는 Tauri의
//! `set_ignore_cursor_events`가 붙인다.
//!
//! 창 조작은 모두 비동기(`SWP_ASYNCWINDOWPOS`, `ShowWindowAsync`)로 한다. 메인 스레드가
//! 무언가를 기다리는 중이어도 이 호출이 막히지 않게 하기 위해서다.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowLongPtrW, GetWindowRect, SetWindowLongPtrW, SetWindowPos,
    ShowWindowAsync, GWL_EXSTYLE, HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

pub const EVENT: &str = "osd://update";
const LABEL: &str = "osd";
/// 작업 영역 가장자리에서 띄우는 거리 (물리 픽셀)
const MARGIN: i32 = 12;

pub const PAGE: Duration = Duration::from_millis(2500);
pub const BUTTON: Duration = Duration::from_millis(800);
pub const MODE: Duration = Duration::from_millis(1000);
pub const ERROR: Duration = Duration::from_millis(3000);

/// 프런트엔드 `src/osd/types.ts`의 `OsdPayload`와 같은 모양.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Payload {
    pub seq: u64,
    pub mode: &'static str,
    pub page: usize,
    pub page_count: usize,
    pub page_name: String,
    pub buttons: BTreeMap<String, String>,
    pub highlight: Option<String>,
    pub error: Option<String>,
}

/// 띄우는 방법.
#[derive(Debug, Clone, Copy)]
pub enum Show {
    /// 이 시간 뒤에 숨긴다. `-` 키로 고정해 둔 동안에는 숨기지 않는다.
    For(Duration),
    /// `-` 키: 고정해서 띄우거나 숨긴다.
    Toggle,
}

pub struct Osd {
    app: AppHandle,
    /// `HWND`는 스레드 사이로 옮길 수 없어 정수로 둔다.
    hwnd: isize,
    /// 띄울 때마다 늘어난다. 숨김 예약이 그 사이 새로 띄운 OSD를 숨기지 않게 한다.
    generation: Arc<AtomicU64>,
    pinned: Arc<AtomicBool>,
    seq: AtomicU64,
}

impl Osd {
    /// 숨긴 OSD 창을 만든다. 메인 스레드에서 불러야 한다.
    pub fn create(app: &AppHandle) -> tauri::Result<Osd> {
        let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("osd.html".into()))
            .title("Numpad Deck OSD")
            // 오류 문구가 길어져도 패널이 잘리지 않게 넉넉히 잡는다. 투명하고 클릭이 통과한다.
            .inner_size(360.0, 480.0)
            .resizable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .visible(false)
            // GPU 가속을 끈다. 작은 OSD에는 필요 없고, 끄면 WebView2 상주 메모리가 약
            // 216MB → 109MB로 준다 (실측). 앞부분은 wry 기본 인자로, 덮어쓰므로 다시 넣는다.
            // 3단계 편집기 창도 같은 인자를 써야 한다 (인자가 다르면 데이터 폴더도 달라야 함).
            .additional_browser_args(
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu --disable-gpu-compositing",
            )
            .build()?;
        window.set_ignore_cursor_events(true)?;
        let hwnd = window.hwnd()?.0 as isize;
        unsafe {
            let hwnd = HWND(hwnd as *mut _);
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                style | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize,
            );
        }
        Ok(Osd {
            app: app.clone(),
            hwnd,
            generation: Arc::new(AtomicU64::new(0)),
            pinned: Arc::new(AtomicBool::new(false)),
            seq: AtomicU64::new(0),
        })
    }

    pub fn show(&self, mut payload: Payload, how: Show) {
        let duration = match how {
            Show::Toggle => {
                let pin = !self.pinned.load(Ordering::Relaxed);
                self.pinned.store(pin, Ordering::Relaxed);
                if !pin {
                    self.hide();
                    return;
                }
                None
            }
            Show::For(_) if self.pinned.load(Ordering::Relaxed) => None,
            Show::For(d) => Some(d),
        };
        payload.seq = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.app.emit_to(LABEL, EVENT, payload);
        self.place_and_show();

        let generation = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(d) = duration {
            let current = Arc::clone(&self.generation);
            let pinned = Arc::clone(&self.pinned);
            let hwnd = self.hwnd;
            thread::spawn(move || {
                thread::sleep(d);
                if current.load(Ordering::Relaxed) == generation && !pinned.load(Ordering::Relaxed)
                {
                    hide_window(hwnd);
                }
            });
        }
    }

    pub fn hide(&self) {
        self.pinned.store(false, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Relaxed);
        hide_window(self.hwnd);
    }

    /// 마우스가 있는 모니터의 작업 영역(작업 표시줄 제외) 우측 하단에 띄운다.
    fn place_and_show(&self) {
        unsafe {
            let hwnd = HWND(self.hwnd as *mut _);
            let mut cursor = POINT::default();
            let _ = GetCursorPos(&mut cursor);
            let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return;
            }
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
            let work = info.rcWork;
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                work.right - w - MARGIN,
                work.bottom - h - MARGIN,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_ASYNCWINDOWPOS,
            );
        }
    }
}

fn hide_window(hwnd: isize) {
    unsafe {
        let _ = ShowWindowAsync(HWND(hwnd as *mut _), SW_HIDE);
    }
}
