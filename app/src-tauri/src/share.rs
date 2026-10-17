//! 페이지 공유 파일(.npdeck): command와 "열 파일" 요청. 파일 처리는 코어(`deck_core::share`)가 한다.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use deck_core::action::{apps, with_com};
use deck_core::i18n::tr;
use deck_core::share::{
    self, BackupMode, Bundle, ExportReport, InstallChoice, InstallMode, Preview, Probe, Selection,
    UserDirs,
};
use tauri::{AppHandle, State};

use crate::runtime::Runtime;

/// 지금 미리 보기 중인 묶음과 더블클릭으로 들어온 파일.
#[derive(Default)]
pub struct ShareState {
    pending: Mutex<Option<(u64, Bundle)>>,
    open_request: Mutex<Option<PathBuf>>,
    seq: AtomicU64,
}

impl ShareState {
    pub fn set_request(&self, path: PathBuf) {
        *self.open_request.lock().unwrap() = Some(path);
    }
}

/// 인자 중 `.npdeck`(대소문자 무시)인 첫 번째 경로.
pub fn share_arg<I, S>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    args.into_iter()
        .map(|a| PathBuf::from(a.as_ref()))
        .find(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(share::EXTENSION))
        })
}

/// 이 PC에 대상이 있는지 실제로 확인한다.
struct PcProbe {
    apps: BTreeSet<String>,
}

impl Probe for PcProbe {
    fn file_exists(&self, expanded: &str) -> bool {
        Path::new(expanded).exists()
    }
    fn dir_exists(&self, expanded: &str) -> bool {
        Path::new(expanded).is_dir()
    }
    fn app_installed(&self, target: &str) -> bool {
        self.apps.contains(&target.to_lowercase())
    }
}

fn selection(pages: Option<Vec<usize>>) -> Selection {
    pages.map_or(Selection::All, Selection::Pages)
}

fn join_err(e: tauri::Error) -> String {
    e.to_string()
}

#[tauri::command]
pub async fn export_check(
    runtime: State<'_, Arc<Runtime>>,
    pages: Option<Vec<usize>>,
) -> Result<ExportReport, String> {
    let config = runtime.config();
    tauri::async_runtime::spawn_blocking(move || share::export_check(&config, &selection(pages)))
        .await
        .map_err(join_err)
}

#[tauri::command]
pub async fn export_share(
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
    pages: Option<Vec<usize>>,
    path: String,
) -> Result<(), String> {
    let config = runtime.config();
    let version = app.package_info().version.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        share::export(
            &config,
            &selection(pages),
            &version,
            &UserDirs::current(),
            Path::new(&path),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(join_err)?
}

/// 파일을 읽어 미리 보기를 만든다. 열 때마다 대기 중인 묶음을 새 id로 교체한다.
#[tauri::command]
pub async fn open_share(state: State<'_, ShareState>, path: String) -> Result<Preview, String> {
    let (bundle, probe) = tauri::async_runtime::spawn_blocking(move || {
        let bundle = share::read(Path::new(&path)).map_err(|e| e.to_string())?;
        // 설치된 앱 목록을 못 읽으면 앱은 모두 없는 것으로 본다
        // 스토어 앱 실행 대상이 있을 때만 읽는다 (느리므로)
        let apps = if share::needs_app_list(&bundle) {
            with_com(apps::list)
                .ok()
                .map(|v| v.into_iter().map(|a| a.target.to_lowercase()).collect())
                .unwrap_or_default()
        } else {
            BTreeSet::new()
        };
        Ok::<_, String>((bundle, PcProbe { apps }))
    })
    .await
    .map_err(join_err)??;
    let id = state.seq.fetch_add(1, Ordering::SeqCst) + 1;
    let preview = share::preview(&bundle, id, &probe);
    *state.pending.lock().unwrap() = Some((id, bundle));
    Ok(preview)
}

fn stale() -> String {
    tr(
        "파일이 바뀌었어요. 다시 열어 주세요.",
        "The file changed. Please open it again.",
    )
}

/// 설치한다. "바꾸기"면 저장 전에 자동 백업을 하고 그 경로를 돌려준다.
#[tauri::command]
pub async fn install_share(
    state: State<'_, ShareState>,
    app: AppHandle,
    runtime: State<'_, Arc<Runtime>>,
    choice: InstallChoice,
) -> Result<Option<String>, String> {
    // 묶음을 꺼내 쓰고, 실패하면 (그 사이 새로 열린 게 없을 때만) 되돌린다
    let taken = {
        let mut pending = state.pending.lock().unwrap();
        match pending.take() {
            Some((id, b)) if id == choice.preview_id => (id, b),
            other => {
                *pending = other;
                return Err(stale());
            }
        }
    };
    let config_path = runtime.config_path().to_path_buf();
    let version = app.package_info().version.to_string();
    // 설정과 백업 방식은 한 번에 읽는다 (설정 파일이 깨졌으면 원본 .json도 남긴다)
    let (config, backup_mode) = runtime.backup_source();
    // 바꾸기이거나 설정 파일이 깨져 있으면 덮어쓰기 전에 백업한다
    let replace = choice.mode == InstallMode::Replace || backup_mode != BackupMode::Export;
    let (taken, result) = tauri::async_runtime::spawn_blocking(move || {
        let r = (|| {
            let backup = if replace {
                Some(
                    share::backup_config(
                        &config_path,
                        &config,
                        backup_mode,
                        &version,
                        &UserDirs::current(),
                        std::time::SystemTime::now(),
                    )
                    .map_err(|e| e.to_string())?,
                )
            } else {
                None
            };
            let dir = config_path.parent().unwrap_or(Path::new("."));
            let new = share::install(&config, &taken.1, &choice, dir).map_err(|e| e.to_string())?;
            Ok::<_, String>((new, backup))
        })();
        (taken, r)
    })
    .await
    .map_err(join_err)?;
    let saved = result.and_then(|(new, backup)| {
        runtime.save_config(new)?;
        Ok(backup.map(|p| p.to_string_lossy().into_owned()))
    });
    if saved.is_err() {
        let mut pending = state.pending.lock().unwrap();
        if pending.is_none() {
            *pending = Some(taken);
        }
    }
    saved
}

/// 더블클릭으로 들어온 파일을 꺼내고 비운다.
#[tauri::command]
pub fn take_share_request(state: State<'_, ShareState>) -> Option<String> {
    state
        .open_request
        .lock()
        .unwrap()
        .take()
        .map(|p| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_npdeck_case_insensitive() {
        let args = ["app.exe", "--autostart", r"C:\a\Zoom.NPDeck", "b.npdeck"];
        assert_eq!(share_arg(args), Some(PathBuf::from(r"C:\a\Zoom.NPDeck")));
    }

    #[test]
    fn accepts_os_strings() {
        let args = [
            std::ffi::OsString::from("app.exe"),
            std::ffi::OsString::from(r"C:\a\b.npdeck"),
        ];
        assert_eq!(share_arg(args), Some(PathBuf::from(r"C:\a\b.npdeck")));
    }

    #[test]
    fn ignores_other_args() {
        assert_eq!(
            share_arg(["app.exe", "--autostart", "x.json", "npdeck"]),
            None
        );
        assert_eq!(share_arg(Vec::<String>::new()), None);
    }
}
