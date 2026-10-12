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
pub const MENU_UPDATE: &str = "update";
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
    /// 업데이트 확인/설치. 글자는 상태에 따라 바뀐다.
    update: MenuItem<Wry>,
    update_status: std::sync::Arc<std::sync::Mutex<crate::update::Status>>,
    /// 마지막 모드 (작업 표시줄 밝기·배율이 바뀌면 같은 모드로 다시 그린다)
    mode: std::sync::Arc<std::sync::Mutex<Mode>>,
    quit: MenuItem<Wry>,
}

impl Tray {
    /// `autostart`: 지금 Windows 시작 시 실행이 켜져 있는가, `autostart_available`: 바꿀 수
    /// 있는가 (개발 빌드는 못 바꾼다)
    pub fn create(
        app: &AppHandle,
        view: &View,
        autostart: bool,
        autostart_available: bool,
    ) -> tauri::Result<Tray> {
        let status = MenuItem::with_id(app, "status", "", false, None::<&str>)?;
        let error = MenuItem::with_id(app, "error", "", false, None::<&str>)?;
        // 글자는 `update`가 지금 언어로 쓴다.
        let editor = MenuItem::with_id(app, MENU_EDITOR, "", true, None::<&str>)?;
        let pause = CheckMenuItem::with_id(app, MENU_PAUSE, "", true, false, None::<&str>)?;
        let stop_sounds = MenuItem::with_id(app, MENU_STOP_SOUNDS, "", true, None::<&str>)?;
        let open = MenuItem::with_id(app, MENU_OPEN_CONFIG, "", true, None::<&str>)?;
        let autostart = CheckMenuItem::with_id(
            app,
            MENU_AUTOSTART,
            "",
            autostart_available,
            autostart,
            None::<&str>,
        )?;
        let update = MenuItem::with_id(app, MENU_UPDATE, "", crate::update::RELEASE, None::<&str>)?;
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
                &update,
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
            update,
            update_status: std::sync::Arc::new(std::sync::Mutex::new(if crate::update::RELEASE {
                crate::update::Status::Idle
            } else {
                crate::update::Status::Unavailable
            })),
            quit,
            mode: std::sync::Arc::new(std::sync::Mutex::new(view.mode)),
        };
        tray.update(view);
        Ok(tray)
    }

    /// Windows 시작 시 실행 체크를 실제 상태에 맞춘다 (메뉴를 누르면 체크가 먼저 바뀌므로).
    pub fn set_autostart(&self, on: bool) {
        let _ = self.autostart.set_checked(on);
    }

    /// 아이콘만 다시 그린다 (작업 표시줄 밝기·배율이 바뀌었을 때).
    pub fn refresh_icon(&self) {
        let mode = *self.mode.lock().unwrap();
        let _ = self.icon.set_icon(Some(icon(mode)));
    }

    /// 업데이트 메뉴를 상태에 맞춘다.
    pub fn set_update(&self, status: &crate::update::Status) {
        *self.update_status.lock().unwrap() = status.clone();
        self.render_update();
    }

    fn render_update(&self) {
        use crate::update::Status;
        let status = self.update_status.lock().unwrap().clone();
        let text = match &status {
            Status::Idle | Status::Failed { .. } | Status::Unavailable => {
                tr("업데이트 확인", "Check for updates")
            }
            Status::Checking => tr("업데이트 확인 중…", "Checking for updates…"),
            Status::Latest => tr("최신 버전이에요", "You're up to date"),
            Status::Available { version } => tr(
                format!("업데이트 설치 ({version})"),
                format!("Install update ({version})"),
            ),
            Status::Installing => tr("업데이트 설치 중…", "Installing update…"),
        };
        let enabled =
            crate::update::RELEASE && !matches!(status, Status::Checking | Status::Installing);
        let _ = self.update.set_text(text);
        let _ = self.update.set_enabled(enabled);
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
        self.render_update();
        let _ = self.quit.set_text(tr("종료", "Quit"));
        let status = format!("{mode} · {}", view.page);
        *self.mode.lock().unwrap() = view.mode;
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

/// 지금 작업 표시줄 배율·밝기에 맞춘 트레이 아이콘.
pub fn icon(mode: Mode) -> Image<'static> {
    draw(
        mode,
        crate::theme::tray_icon_size(),
        crate::theme::taskbar_is_light(),
    )
}

/// 크기별 그림 (Windows가 줄이면 칸 사이가 번지므로 그 크기로 직접 그린다):
/// (기준 크기, 시작 여백, 칸, 간격, 칸 네 귀퉁이 1px 비움, 숫자 모드 테두리 두께)
const SPECS: [(u32, u32, u32, u32, bool, u32); 4] = [
    (16, 0, 4, 2, false, 1),
    (20, 1, 4, 3, false, 1),
    (24, 1, 6, 2, true, 1),
    (32, 2, 8, 3, true, 2),
];

/// 3×3 칸: 덱 모드 = 아홉 칸 채움, 숫자 모드 = 테두리만, 일시정지 = 가운데 줄 빔. 바탕 투명.
/// 색은 작업 표시줄 밝기에 맞춘다 (밝은 작업 표시줄에서 회색이 묻히지 않게).
fn draw(mode: Mode, size: u32, light: bool) -> Image<'static> {
    // 표에 없는 크기는 바로 아래 크기 그림을 가운데에 놓는다
    let &(base, margin, cell, gap, cut, thick) = SPECS
        .iter()
        .rev()
        .find(|s| s.0 <= size)
        .unwrap_or(&SPECS[0]);
    let size = size.max(base);
    let offset = (size - base) / 2;
    let color: [u8; 4] = match (mode, light) {
        (Mode::Deck, true) => [0x25, 0x63, 0xEB, 0xFF],
        (Mode::Deck, false) => [0x3B, 0x82, 0xF6, 0xFF],
        (Mode::Numbers, true) => [0x67, 0x62, 0x5B, 0xFF],
        (Mode::Numbers, false) => [0xA8, 0xA2, 0x9A, 0xFF],
        (Mode::Paused, true) => [0xA3, 0x9D, 0x94, 0xFF],
        (Mode::Paused, false) => [0x6B, 0x66, 0x5F, 0xFF],
    };
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    for row in 0..3 {
        for col in 0..3 {
            let x0 = offset + margin + col * (cell + gap);
            let y0 = offset + margin + row * (cell + gap);
            let x1 = x0 + cell - 1;
            let y1 = y0 + cell - 1;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    // 귀퉁이 한 픽셀을 비워 둥글게 보이게 한다 (24px 이상)
                    if cut && (x == x0 || x == x1) && (y == y0 || y == y1) {
                        continue;
                    }
                    let fill = match mode {
                        Mode::Deck => true,
                        Mode::Numbers => {
                            x < x0 + thick || x > x1 - thick || y < y0 + thick || y > y1 - thick
                        }
                        Mode::Paused => row != 1,
                    };
                    if fill && x < size && y < size {
                        let i = ((y * size + x) * 4) as usize;
                        rgba[i..i + 4].copy_from_slice(&color);
                    }
                }
            }
        }
    }
    Image::new_owned(rgba, size, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(img: &Image<'_>, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * img.width() + x) * 4) as usize;
        img.rgba()[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn draws_at_each_tray_size_without_scaling() {
        for size in [16, 20, 24, 28, 32, 40] {
            let img = draw(Mode::Deck, size, false);
            assert_eq!((img.width(), img.height()), (size, size), "{size}");
        }
        // 16px: 칸 4px, 간격 2px, 여백 0 → (0,0)은 채움, (4,0)은 간격(투명)
        let img = draw(Mode::Deck, 16, true);
        assert_eq!(pixel(&img, 0, 0), [0x25, 0x63, 0xEB, 0xFF]);
        assert_eq!(pixel(&img, 4, 0)[3], 0);
        // 24px: 귀퉁이 1px 비움
        let img = draw(Mode::Deck, 24, false);
        assert_eq!(pixel(&img, 1, 1)[3], 0);
        assert_eq!(pixel(&img, 2, 1)[3], 0xFF);
    }

    #[test]
    fn numbers_mode_is_outline_and_paused_skips_middle_row() {
        // 32px 숫자 모드: 칸(2..10) 테두리 2px, 가운데는 비움
        let img = draw(Mode::Numbers, 32, false);
        assert_eq!(pixel(&img, 3, 5)[3], 0xFF);
        assert_eq!(pixel(&img, 5, 5)[3], 0);
        // 일시정지: 가운데 줄(y 13..21) 빔
        let img = draw(Mode::Paused, 32, false);
        assert_eq!(pixel(&img, 5, 16)[3], 0);
        assert_eq!(pixel(&img, 5, 5)[3], 0xFF);
    }
}
