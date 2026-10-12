//! 설치된 앱 목록: 시작 메뉴의 "모든 앱"과 같은 `shell:AppsFolder`를 읽는다.
//!
//! 일반 프로그램은 실행 파일 경로를 대상으로 삼아 "실행 중이면 앞으로 가져오기"가
//! 동작하게 한다. 스토어 앱처럼 실행 파일을 알 수 없으면 `shell:AppsFolder\<앱 ID>`로
//! 실행만 한다.

use serde::Serialize;
use windows::core::{w, Interface, PWSTR};
use windows::Win32::Storage::EnhancedStorage::PKEY_Link_TargetParsingPath;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, IEnumShellItems, IShellItem, IShellItem2, SHCreateItemFromParsingName,
    SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppEntry {
    /// 시작 메뉴에 보이는 이름
    pub name: String,
    /// `launch` 액션의 `target`
    pub target: String,
}

/// 설치된 앱 목록 (이름순). 호출 스레드에 COM이 초기화되어 있어야 한다
/// ([`super::with_com`]). 앱이 많으면 수백 ms 걸릴 수 있다.
pub fn list() -> Result<Vec<AppEntry>, String> {
    unsafe {
        let folder: IShellItem = SHCreateItemFromParsingName(w!("shell:AppsFolder"), None)
            .map_err(|e| {
                crate::i18n::tr(
                    format!("앱 목록을 열 수 없습니다: {e}"),
                    format!("Cannot open the app list: {e}"),
                )
            })?;
        let items: IEnumShellItems = folder.BindToHandler(None, &BHID_EnumItems).map_err(|e| {
            crate::i18n::tr(
                format!("앱 목록을 읽을 수 없습니다: {e}"),
                format!("Cannot read the app list: {e}"),
            )
        })?;

        let mut apps = Vec::new();
        loop {
            let mut batch = [None];
            let mut fetched = 0;
            if items.Next(&mut batch, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = batch[0].take() else { break };
            if let Some(app) = entry(&item) {
                apps.push(app);
            }
        }
        apps.sort_by_key(|a| a.name.to_lowercase());
        apps.dedup_by(|a, b| a.name == b.name && a.target == b.target);
        Ok(apps)
    }
}

unsafe fn entry(item: &IShellItem) -> Option<AppEntry> {
    unsafe {
        let name = take_string(item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?)?;
        let id = take_string(item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).ok()?)?;
        // 웹 주소로 등록된 항목(문서 링크 등)은 빼고, 실행 파일이 있으면 그 경로를 쓴다.
        if id.contains("://") {
            return None;
        }
        let exe = item
            .cast::<IShellItem2>()
            .ok()
            .and_then(|i2| i2.GetString(&PKEY_Link_TargetParsingPath).ok())
            .and_then(|p| take_string(p))
            .filter(|p| p.to_ascii_lowercase().ends_with(".exe"))
            // 탐색기를 거쳐 여는 바로가기(인자로 폴더·앱을 넘김)는 탐색기로 잡히면 안 된다.
            .filter(|p| !p.to_ascii_lowercase().ends_with(r"\explorer.exe"));
        let target = exe.unwrap_or_else(|| format!("shell:AppsFolder\\{id}"));
        Some(AppEntry { name, target })
    }
}

unsafe fn take_string(p: PWSTR) -> Option<String> {
    if p.is_null() {
        return None;
    }
    unsafe {
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.filter(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_some_apps_including_explorer_or_settings() {
        let apps = super::super::with_com(list).unwrap();
        assert!(!apps.is_empty());
        // 모든 Windows에 있는 앱 하나쯤은 있어야 한다.
        assert!(
            apps.iter()
                .any(|a| a.target.to_lowercase().contains("explorer")
                    || a.target.contains("windows.immersivecontrolpanel")
                    || a.name.contains("Settings")
                    || a.name.contains("설정")),
            "{:?}",
            apps.iter().take(20).collect::<Vec<_>>()
        );
        for a in &apps {
            assert!(!a.name.is_empty() && !a.target.is_empty());
        }
    }
}
