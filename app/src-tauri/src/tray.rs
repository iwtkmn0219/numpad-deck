//! 트레이 아이콘과 메뉴.
//!
//! 아이콘은 넘패드 3×3 모양을 코드로 그린다. 모드마다 모양이 달라서 색을 구분하기
//! 어려운 사용자도 알아볼 수 있게 한다.
//!
//! - 숫자 모드(NumLock ON): 테두리만 있는 칸
//! - 덱 모드(NumLock OFF): 파랗게 채운 칸
//! - 일시정지: 회색으로 채운 칸 + 가운데 줄 비움
//!
//! 오른쪽 클릭 메뉴는 직접 띄운다. `tray-icon`은 메뉴 아래쪽을 커서(작업 표시줄 위)에
//! 맞추는데, 전체 화면 앱에서 Win 키로 작업 표시줄을 불러오면 작업 표시줄이 메뉴보다 위에
//! 그려져 아래 항목(종료)이 가려진다. 그래서 메뉴를 작업 영역 안(작업 표시줄 바깥)에 둔다.

use deck_core::i18n::tr;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, ContextMenu, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, PostMessageW, SetForegroundWindow, TrackPopupMenuEx, HMENU, TPM_BOTTOMALIGN,
    TPM_LEFTALIGN, TPM_RIGHTALIGN, TPM_TOPALIGN, WM_NULL,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Numbers,
    Deck,
    Paused,
}

/// 트레이에 보여줄 상태.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub mode: Mode,
    /// 예: "1/2 미디어 · 시스템"
    pub page: String,
    /// 설정 파일 오류. 있으면 메뉴 맨 위에 보인다.
    pub config_error: Option<String>,
}

pub const MENU_EDITOR: &str = "editor";
pub const MENU_PAUSE: &str = "pause";
pub const MENU_STOP_SOUNDS: &str = "stop-sounds";
pub const MENU_OPEN_CONFIG: &str = "open-config";
pub const MENU_AUTOSTART: &str = "autostart";
pub const MENU_QUIT: &str = "quit";

#[derive(Clone)]
pub struct Tray {
    icon: TrayIcon,
    status: MenuItem<Wry>,
    error: MenuItem<Wry>,
    pause: CheckMenuItem<Wry>,
    /// 언어가 바뀌면 글자를 다시 쓰는 항목들
    editor: MenuItem<Wry>,
    stop_sounds: MenuItem<Wry>,
    open: MenuItem<Wry>,
    /// Windows 시작 시 실행 (체크)
    autostart: CheckMenuItem<Wry>,
    quit: MenuItem<Wry>,
}

impl Tray {
    /// `autostart`: 지금 Windows 시작 시 실행이 켜져 있는가
    pub fn create(app: &AppHandle, view: &View, autostart: bool) -> tauri::Result<Tray> {
        let status = MenuItem::with_id(app, "status", "", false, None::<&str>)?;
        let error = MenuItem::with_id(app, "error", "", false, None::<&str>)?;
        // 글자는 `update`가 지금 언어로 쓴다.
        let editor = MenuItem::with_id(app, MENU_EDITOR, "", true, None::<&str>)?;
        let pause = CheckMenuItem::with_id(app, MENU_PAUSE, "", true, false, None::<&str>)?;
        let stop_sounds = MenuItem::with_id(app, MENU_STOP_SOUNDS, "", true, None::<&str>)?;
        let open = MenuItem::with_id(app, MENU_OPEN_CONFIG, "", true, None::<&str>)?;
        let autostart =
            CheckMenuItem::with_id(app, MENU_AUTOSTART, "", true, autostart, None::<&str>)?;
        let quit = MenuItem::with_id(app, MENU_QUIT, "", true, None::<&str>)?;
        let menu = Menu::with_items(
            app,
            &[
                &status,
                &error,
                &PredefinedMenuItem::separator(app)?,
                &editor,
                &pause,
                &stop_sounds,
                &open,
                &PredefinedMenuItem::separator(app)?,
                &autostart,
                &quit,
            ],
        )?;
        let popup = menu.clone();
        let icon = TrayIconBuilder::with_id("main")
            .icon(icon(view.mode))
            // 메뉴를 붙여야 메뉴 명령이 트레이 창으로 전달된다. 띄우는 것은 직접 한다.
            .menu(&menu)
            // Windows 관례: 왼쪽 클릭 = 편집기, 오른쪽 클릭 = 메뉴
            .show_menu_on_left_click(false)
            .on_tray_icon_event(move |tray, event| {
                if let TrayIconEvent::Click {
                    button,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    match button {
                        MouseButton::Left => crate::editor::open(tray.app_handle()),
                        MouseButton::Right => show_menu(tray, &popup),
                        _ => {}
                    }
                }
            })
            .build(app)?;
        icon.with_inner_tray_icon(|t| t.set_show_menu_on_right_click(false))?;
        let tray = Tray {
            icon,
            status,
            error,
            pause,
            editor,
            stop_sounds,
            open,
            autostart,
            quit,
        };
        tray.update(view);
        Ok(tray)
    }

    /// Windows 시작 시 실행 체크를 실제 상태에 맞춘다 (메뉴를 누르면 체크가 먼저 바뀌므로).
    pub fn set_autostart(&self, on: bool) {
        let _ = self.autostart.set_checked(on);
    }

    /// 아이콘·툴팁·메뉴를 상태에 맞춘다. 어느 스레드에서 불러도 된다.
    pub fn update(&self, view: &View) {
        let mode = match view.mode {
            Mode::Numbers => tr("숫자 모드 (NumLock 켜짐)", "Number mode (NumLock on)"),
            Mode::Deck => tr("덱 모드", "Deck mode"),
            Mode::Paused => tr("일시정지됨", "Paused"),
        };
        let _ = self.editor.set_text(tr("설정 열기", "Open settings"));
        let _ = self.pause.set_text(tr("일시정지", "Pause"));
        let _ = self
            .stop_sounds
            .set_text(tr("효과음 멈추기", "Stop sounds"));
        let _ = self.open.set_text(tr("설정 파일 열기", "Open config file"));
        let _ = self
            .autostart
            .set_text(tr("Windows 시작 시 실행", "Start with Windows"));
        let _ = self.quit.set_text(tr("종료", "Quit"));
        let status = format!("{mode} · {}", view.page);
        let _ = self.icon.set_icon(Some(icon(view.mode)));
        let _ = self
            .icon
            .set_tooltip(Some(format!("Numpad Deck — {status}")));
        let _ = self.status.set_text(status);
        let _ = self.pause.set_checked(view.mode == Mode::Paused);
        match &view.config_error {
            Some(e) => {
                // 메뉴 한 줄에 다 들어가지 않으므로 앞부분만 보인다.
                let short: String = e.chars().take(60).collect();
                let _ = self.error.set_text(tr(
                    format!("⚠ 설정 오류: {short}"),
                    format!("⚠ Config error: {short}"),
                ));
            }
            None => {
                let _ = self.error.set_text(tr("설정 정상", "Config OK"));
            }
        }
    }
}

const SIZE: u32 = 32;

/// 오른쪽 클릭 메뉴를 작업 영역 안에 띄운다. 메인 스레드에서 불린다(트레이 이벤트).
fn show_menu(tray: &TrayIcon, menu: &Menu<Wry>) {
    let Ok(owner) = tray.with_inner_tray_icon(|t| t.window_handle() as isize) else {
        return;
    };
    let Ok(hmenu) = menu.hpopupmenu() else { return };
    unsafe {
        let owner = HWND(owner as *mut _);
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut flags = TPM_BOTTOMALIGN | TPM_LEFTALIGN;
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let (work, full) = (info.rcWork, info.rcMonitor);
            // 작업 표시줄이 있는 쪽에서 메뉴를 작업 영역 경계에 붙인다.
            flags = if work.bottom < full.bottom && pt.y >= work.bottom {
                pt.y = work.bottom;
                TPM_BOTTOMALIGN | TPM_LEFTALIGN
            } else if work.top > full.top && pt.y < work.top {
                pt.y = work.top;
                TPM_TOPALIGN | TPM_LEFTALIGN
            } else if work.left > full.left && pt.x < work.left {
                pt.x = work.left;
                TPM_BOTTOMALIGN | TPM_LEFTALIGN
            } else if work.right < full.right && pt.x >= work.right {
                pt.x = work.right;
                TPM_BOTTOMALIGN | TPM_RIGHTALIGN
            } else {
                TPM_BOTTOMALIGN | TPM_LEFTALIGN
            };
        }
        // 메뉴 바깥을 누르면 닫히도록 트레이 창을 앞으로 (Windows 트레이 메뉴 관례)
        let _ = SetForegroundWindow(owner);
        let _ = TrackPopupMenuEx(HMENU(hmenu as *mut _), flags.0, pt.x, pt.y, owner, None);
        let _ = PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0));
    }
}

/// 32×32 RGBA 아이콘을 그린다.
pub fn icon(mode: Mode) -> Image<'static> {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let blue = [0x3B, 0x82, 0xF6, 0xFF];
    let gray = [0x9C, 0xA3, 0xAF, 0xFF];
    let dim = [0x6B, 0x72, 0x80, 0xFF];
    // 칸 8px, 간격 3px, 여백 2px: 2 + 8 + 3 + 8 + 3 + 8 = 32
    for row in 0..3 {
        for col in 0..3 {
            let x0 = 2 + col * 11;
            let y0 = 2 + row * 11;
            for y in y0..y0 + 8 {
                for x in x0..x0 + 8 {
                    let edge = x == x0 || x == x0 + 7 || y == y0 || y == y0 + 7;
                    // 모서리 한 픽셀은 비워 둥글게 보이게 한다.
                    let corner = (x == x0 || x == x0 + 7) && (y == y0 || y == y0 + 7);
                    if corner {
                        continue;
                    }
                    let color = match mode {
                        Mode::Deck => Some(blue),
                        Mode::Numbers => {
                            // 테두리 2px
                            let thick = x <= x0 + 1 || x >= x0 + 6 || y <= y0 + 1 || y >= y0 + 6;
                            (edge || thick).then_some(gray)
                        }
                        Mode::Paused => (row != 1).then_some(dim),
                    };
                    if let Some(c) = color {
                        let i = ((y * SIZE + x) * 4) as usize;
                        rgba[i..i + 4].copy_from_slice(&c);
                    }
                }
            }
        }
    }
    Image::new_owned(rgba, SIZE, SIZE)
}
