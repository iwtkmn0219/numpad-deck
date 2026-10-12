//! 트레이 아이콘을 다시 그려야 할 때를 알린다: 작업 표시줄이 밝게/어둡게 바뀌거나, 화면
//! 배율(트레이 아이콘 크기)이 바뀔 때.
//!
//! 이런 알림(`WM_SETTINGCHANGE`, `WM_DISPLAYCHANGE`, `WM_DPICHANGED`)은 최상위 창에만 오므로
//! 전용 스레드에 보이지 않는 최상위 창을 하나 둔다 (메시지 전용 창은 방송을 받지 못한다).

use std::sync::OnceLock;
use std::thread;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetSystemMetricsForDpi, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW,
    TranslateMessage, MSG, SM_CXSMICON, WINDOW_EX_STYLE, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_SETTINGCHANGE, WNDCLASSW, WS_OVERLAPPED,
};

/// 작업 표시줄이 밝은 테마인가 (`SystemUsesLightTheme`, 앱 쪽 `AppsUseLightTheme`가 아니다).
/// 값이 없으면 어둡다 (Windows 11 기본).
pub fn taskbar_is_light() -> bool {
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut _),
            Some(&mut size),
        )
    };
    result.is_ok() && value == 1
}

/// 트레이 아이콘 크기(px): 주 모니터(작업 표시줄) 배율의 작은 아이콘 크기.
/// 100% = 16, 125% = 20, 150% = 24, 200% = 32.
pub fn tray_icon_size() -> u32 {
    unsafe {
        let monitor = MonitorFromPoint(Default::default(), MONITOR_DEFAULTTOPRIMARY);
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        let size = GetSystemMetricsForDpi(SM_CXSMICON, dpi_x);
        if size > 0 {
            size as u32
        } else {
            16
        }
    }
}

static ON_CHANGE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if matches!(msg, WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED) {
        if let Some(f) = ON_CHANGE.get() {
            f();
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// 바뀔 때마다 `on_change`를 부른다 (이 스레드에서). 한 번만 시작한다.
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    if ON_CHANGE.set(Box::new(on_change)).is_err() {
        return;
    }
    let spawned = thread::Builder::new()
        .name("deck-theme".into())
        .spawn(|| unsafe {
            let Ok(instance) = GetModuleHandleW(None) else {
                return;
            };
            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: w!("NumpadDeckThemeWatcher"),
                ..Default::default()
            };
            RegisterClassW(&class);
            // 보이지 않는 최상위 창 (ShowWindow를 부르지 않는다)
            let created = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("NumpadDeckThemeWatcher"),
                w!(""),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            );
            if created.is_err() {
                eprintln!("테마 감시 창을 만들 수 없습니다");
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    if let Err(e) = spawned {
        eprintln!("테마 감시 스레드를 만들 수 없습니다: {e}");
    }
}
