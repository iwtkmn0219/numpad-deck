//! 셸 실행: URL·폴더·파일 열기, 프로그램 실행, 환경 변수 펼치기.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// `%USERPROFILE%` 같은 환경 변수를 펼친다. 설정을 다른 PC로 옮겨도 경로가 맞게 한다.
pub fn expand_env(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let src = HSTRING::from(s);
    let needed = unsafe { ExpandEnvironmentStringsW(&src, None) };
    if needed == 0 {
        return s.to_string();
    }
    let mut buf = vec![0u16; needed as usize];
    let written = unsafe { ExpandEnvironmentStringsW(&src, Some(&mut buf)) };
    if written == 0 || written as usize > buf.len() {
        return s.to_string();
    }
    // 끝의 NUL 제외
    String::from_utf16_lossy(&buf[..written as usize - 1])
}

/// URL, 폴더, 파일을 기본 프로그램으로 연다.
pub fn open(target: &str) -> Result<(), String> {
    execute(target, None)
}

/// 프로그램이나 `shell:` 경로를 실행한다. Windows 오류 창은 띄우지 않고 오류를 돌려준다.
pub fn execute(target: &str, args: Option<&str>) -> Result<(), String> {
    let target = expand_env(target.trim());
    let verb = HSTRING::from("open");
    let file = HSTRING::from(target.as_str());
    let params = args.map(|a| HSTRING::from(expand_env(a)));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: params
            .as_ref()
            .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    match unsafe { ShellExecuteExW(&mut info) } {
        Ok(()) => Ok(()),
        Err(e) => {
            let reason = match e.code().0 as u32 & 0xFFFF {
                2 => "파일을 찾을 수 없습니다".to_string(),
                3 => "경로를 찾을 수 없습니다".to_string(),
                5 => "접근이 거부되었습니다".to_string(),
                1155 => "이 파일을 열 프로그램이 없습니다".to_string(),
                1223 => "사용자가 취소했습니다".to_string(),
                // 모르는 오류만 Windows 원문을 붙인다.
                _ => format!("열 수 없습니다 ({e})"),
            };
            Err(format!("\"{target}\": {reason}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_known_variables_and_keeps_plain_text() {
        let expanded = expand_env("%WINDIR%\\notepad.exe");
        assert!(!expanded.contains('%'), "{expanded}");
        assert!(expanded.to_lowercase().ends_with("\\notepad.exe"));
        assert_eq!(expand_env("C:\\plain"), "C:\\plain");
        assert_eq!(expand_env("%NO_SUCH_VAR_NPDK%"), "%NO_SUCH_VAR_NPDK%");
    }

    #[test]
    fn missing_file_is_reported_in_korean_without_dialog() {
        let err = execute("C:\\no\\such\\file_npdk.exe", None).unwrap_err();
        assert!(err.contains("찾을 수 없습니다"), "{err}");
    }
}
