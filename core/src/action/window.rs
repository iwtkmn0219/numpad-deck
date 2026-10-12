//! 프로그램 실행·포커스.
//!
//! - 실행 중이 아니면 실행한다.
//! - 실행 중이면 그 앱의 가장 최근 창을 앞으로 가져온다.
//! - 이미 맨 앞이면 같은 앱의 다음 창으로 넘긴다. 창이 하나면 그대로 둔다.
//!
//! 앱 판별은 창을 만든 프로세스의 실행 파일로 한다. 설정에 전체 경로를 적었으면
//! 경로로, 파일 이름만 적었으면(`mspaint.exe`) 파일 이름으로 비교한다.

use std::collections::HashMap;
use std::path::Path;

use windows::core::{BOOL, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongW,
    GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SetForegroundWindow,
    ShowWindow, GWL_EXSTYLE, GW_OWNER, SW_RESTORE, WS_EX_TOOLWINDOW,
};

use super::shell;
use crate::inject;

pub fn launch_or_focus(target: &str, args: Option<&str>) -> Result<(), String> {
    let expanded = shell::expand_env(target.trim());
    // 스토어 앱(shell:AppsFolder\...)과 URL은 실행 중인 창을 찾을 수 없으니 그냥 연다.
    // 스토어 앱은 대개 이미 열린 창을 스스로 앞으로 가져온다.
    if expanded.to_ascii_lowercase().starts_with("shell:") || expanded.contains("://") {
        return shell::execute(&expanded, args);
    }

    let windows = app_windows(&expanded);
    let Some(&first) = windows.first() else {
        return shell::execute(&expanded, args);
    };
    let foreground = unsafe { GetForegroundWindow() };
    let next = if windows.contains(&foreground) {
        if windows.len() == 1 {
            return Ok(());
        }
        // Z 순서상 가장 뒤의 창을 올리면 버튼을 누를 때마다 모든 창을 차례로 돈다.
        *windows.last().unwrap()
    } else {
        first
    };
    activate(next)
}

/// 대상 앱의 최상위 창들. Z 순서(앞에서 뒤)대로.
fn app_windows(target: &str) -> Vec<HWND> {
    let matches = Matcher::new(target);
    let mut images: HashMap<u32, Option<String>> = HashMap::new();
    top_level_windows()
        .into_iter()
        .filter(|&hwnd| {
            let mut pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
            images
                .entry(pid)
                .or_insert_with(|| process_image(pid))
                .as_deref()
                .is_some_and(|image| matches.is(image))
        })
        .collect()
}

struct Matcher {
    /// 소문자, 역슬래시로 통일
    full_path: Option<String>,
    file_name: String,
}

impl Matcher {
    fn new(target: &str) -> Self {
        let normalized = target.trim_matches('"').replace('/', "\\").to_lowercase();
        let has_dir = normalized.contains('\\');
        let mut file_name = Path::new(&normalized)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if Path::new(&file_name).extension().is_none() {
            file_name.push_str(".exe");
        }
        Matcher {
            full_path: has_dir.then_some(normalized),
            file_name,
        }
    }

    fn is(&self, image: &str) -> bool {
        let image = image.to_lowercase();
        match &self.full_path {
            Some(path) => image == *path,
            None => image.rsplit('\\').next() == Some(self.file_name.as_str()),
        }
    }
}

/// 작업 표시줄에 나올 만한 창: 보이고, 주인 창이 없고, 도구 창이 아니고, 제목이 있고,
/// 숨김 처리(cloak)되지 않은 창. 다른 가상 데스크톱의 창은 cloak 상태라 빠진다.
fn top_level_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        if is_app_window(hwnd) {
            list.push(hwnd);
        }
        BOOL(1)
    }
    let mut list: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

fn is_app_window(hwnd: HWND) -> bool {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        if GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid()) {
            return false;
        }
        if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        let mut cloaked: u32 = 0;
        let got = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
        !(got.is_ok() && cloaked != 0)
    }
}

fn process_image(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// 창을 앞으로 가져온다. Windows는 포그라운드를 마음대로 바꾸지 못하게 막으므로
/// 실패하면 두 가지 우회를 차례로 시도한다.
fn activate(hwnd: HWND) -> Result<(), String> {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if try_foreground(hwnd) {
            return Ok(());
        }
        // 1. 이 프로세스가 마지막 입력을 보낸 것으로 만들어 포그라운드 권한을 얻는다.
        let _ = inject::tap_vk(0xE8, false);
        if try_foreground(hwnd) {
            return Ok(());
        }
        // 2. 현재 포그라운드 스레드의 입력 상태에 잠시 붙는다.
        let foreground = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(foreground, None);
        let me = GetCurrentThreadId();
        if fg_thread != 0 && fg_thread != me {
            let _ = AttachThreadInput(me, fg_thread, true);
            let _ = BringWindowToTop(hwnd);
            let ok = try_foreground(hwnd);
            let _ = AttachThreadInput(me, fg_thread, false);
            if ok {
                return Ok(());
            }
        }
    }
    Err(crate::i18n::tr(
        "창을 앞으로 가져오지 못했습니다. Windows가 포커스 변경을 막았습니다.",
        "Could not bring the window to the front. Windows blocked the focus change.",
    ))
}

fn try_foreground(hwnd: HWND) -> bool {
    unsafe { SetForegroundWindow(hwnd).as_bool() && GetForegroundWindow() == hwnd }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_by_file_name_when_no_directory() {
        let m = Matcher::new("mspaint.exe");
        assert!(m.is(r"C:\Program Files\WindowsApps\Microsoft.Paint_1\PaintApp\mspaint.exe"));
        assert!(!m.is(r"C:\Windows\notepad.exe"));
        assert!(Matcher::new("Notepad").is(r"C:\Windows\System32\notepad.exe"));
    }

    #[test]
    fn matches_by_full_path_when_directory_given() {
        let m = Matcher::new(r"C:\Tools\app.exe");
        assert!(m.is(r"c:\tools\APP.exe"));
        assert!(!m.is(r"D:\Other\app.exe"));
        assert!(Matcher::new("C:/Tools/app.exe").is(r"C:\Tools\app.exe"));
    }

    #[test]
    fn enumerates_some_windows_without_crashing() {
        // 테스트 환경에 따라 창이 없을 수도 있으니 개수는 확인하지 않는다.
        let _ = top_level_windows();
        let _ = app_windows("explorer.exe");
    }
}
