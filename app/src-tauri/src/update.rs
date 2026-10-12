//! 자동 업데이트: GitHub Releases의 `latest.json`을 확인한다.
//!
//! 새 버전이 있으면 트레이 메뉴와 설정 창에 알리기만 하고, 사용자가 누르면 받아서 설치한다
//! (매크로를 쓰는 중에 앱이 갑자기 꺼지지 않게). 설치는 NSIS 설치 파일이 맡고, 앱을 끝낸 뒤
//! 다시 켠다. 업데이트 파일은 `tauri.conf.json`의 공개 키로 서명을 확인한다.
//!
//! 개발 빌드는 확인·설치하지 않는다 (설치된 앱을 개발용 실행 파일이 덮어쓰면 안 된다).

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::runtime::Runtime;

pub const RELEASE: bool = !cfg!(debug_assertions);

/// 편집기에 보내는 이벤트: 업데이트 상태가 바뀌었다.
pub const UPDATE_CHANGED: &str = "update://changed";

/// 앱을 켜고 첫 확인까지 (시작을 늦추지 않게)
const FIRST_CHECK: Duration = Duration::from_secs(15);
/// 켜 둔 동안 다시 확인하는 주기 (latest.json 하나라 부담이 적다)
const CHECK_EVERY: Duration = Duration::from_secs(3 * 60 * 60);
/// 편집기를 열 때 확인하되, 마지막 확인에서 이만큼 지났을 때만
const RECHECK_AFTER: Duration = Duration::from_secs(10 * 60);

/// 트레이 메뉴와 설정 창에 보이는 상태.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    /// 아직 확인하지 않음
    Idle,
    Checking,
    /// 지금이 최신
    Latest,
    Available {
        version: String,
    },
    Installing,
    /// 확인·설치 실패 (네트워크 등)
    Failed {
        error: String,
    },
    /// 개발 빌드
    Unavailable,
}

/// 설정 창에 보내는 상태 (`SettingsDialog.tsx`의 `UpdateState`와 모양이 같아야 한다)
#[derive(Debug, Clone, Serialize)]
pub struct State {
    current: String,
    status: Status,
}

#[derive(Default)]
pub struct Updates {
    status: Mutex<Option<Status>>,
    /// 받을 업데이트 (`Available`일 때)
    pending: Mutex<Option<Update>>,
    /// 마지막으로 확인을 시작한 때
    last_check: Mutex<Option<std::time::Instant>>,
}

impl Updates {
    pub fn status(&self) -> Status {
        let fallback = if RELEASE {
            Status::Idle
        } else {
            Status::Unavailable
        };
        self.status.lock().unwrap().clone().unwrap_or(fallback)
    }

    pub fn state(&self, app: &AppHandle) -> State {
        State {
            current: app.package_info().version.to_string(),
            status: self.status(),
        }
    }
}

fn set_status(app: &AppHandle, status: Status) {
    *app.state::<Updates>().status.lock().unwrap() = Some(status.clone());
    let runtime = app.state::<std::sync::Arc<Runtime>>();
    if let Some(tray) = runtime.tray() {
        tray.set_update(&status);
    }
    match &status {
        Status::Available { version } => {
            runtime.set_update_available(Some(version.clone()));
            notify_once(app, &runtime, version);
        }
        Status::Latest => runtime.set_update_available(None),
        _ => {}
    }
    let _ = app.emit_to(
        crate::editor::LABEL,
        UPDATE_CHANGED,
        app.state::<Updates>().state(app),
    );
}

/// 새 버전을 처음 찾았을 때 Windows 알림을 한 번 띄운다. 같은 버전은 다시 알리지 않는다
/// (설정 폴더의 `update-notified`에 알린 버전을 적어 둔다 — 로그인할 때마다 알리지 않게).
fn notify_once(app: &AppHandle, runtime: &Runtime, version: &str) {
    use deck_core::i18n::tr;
    use tauri_plugin_notification::NotificationExt;

    let marker = runtime
        .config_path()
        .parent()
        .map(|dir| dir.join("update-notified"));
    if let Some(m) = &marker {
        if std::fs::read_to_string(m).is_ok_and(|v| v.trim() == version) {
            return;
        }
    }
    let shown = app
        .notification()
        .builder()
        .title(tr(
            format!("Numpad Deck {version} 업데이트"),
            format!("Numpad Deck {version} update"),
        ))
        .body(tr(
            "새 버전이 있어요. 트레이 메뉴나 설정 창에서 설치할 수 있어요.",
            "A new version is available. Install it from the tray menu or Settings.",
        ))
        .show();
    match shown {
        Ok(()) => {
            if let Some(m) = &marker {
                let _ = std::fs::write(m, version);
            }
        }
        Err(e) => eprintln!("업데이트 알림을 띄울 수 없습니다: {e}"),
    }
}

/// 새 버전이 있는지 확인한다. 확인 중이거나 설치 중이면 그대로 둔다.
pub async fn check(app: &AppHandle) -> State {
    let updates = app.state::<Updates>();
    if !RELEASE || matches!(updates.status(), Status::Checking | Status::Installing) {
        return updates.state(app);
    }
    *updates.last_check.lock().unwrap() = Some(std::time::Instant::now());
    set_status(app, Status::Checking);
    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let status = match result {
        Ok(Some(update)) => {
            let version = update.version.clone();
            *updates.pending.lock().unwrap() = Some(update);
            Status::Available { version }
        }
        Ok(None) => Status::Latest,
        Err(e) => {
            eprintln!("업데이트 확인 실패: {e}");
            Status::Failed {
                error: e.to_string(),
            }
        }
    };
    set_status(app, status);
    updates.state(app)
}

/// 받아 둔 업데이트를 설치한다. 성공하면 설치 파일이 앱을 끝내므로 돌아오지 않는다.
pub async fn install(app: &AppHandle) -> State {
    let updates = app.state::<Updates>();
    let Some(update) = updates.pending.lock().unwrap().clone() else {
        return updates.state(app);
    };
    set_status(app, Status::Installing);
    if let Err(e) = update.download_and_install(|_, _| {}, || {}).await {
        eprintln!("업데이트 설치 실패: {e}");
        set_status(
            app,
            Status::Failed {
                error: e.to_string(),
            },
        );
    }
    updates.state(app)
}

/// 편집기를 열 때: 마지막 확인에서 10분이 지났으면 다시 확인한다 (앱을 켜 둔 채 새 버전이
/// 나와도 사용자가 앱을 볼 때 알 수 있게). 이미 새 버전을 찾았으면 다시 묻지 않는다.
pub fn check_if_stale(app: &AppHandle) {
    if !RELEASE {
        return;
    }
    let updates = app.state::<Updates>();
    if matches!(
        updates.status(),
        Status::Available { .. } | Status::Checking | Status::Installing
    ) {
        return;
    }
    let stale = updates
        .last_check
        .lock()
        .unwrap()
        .is_none_or(|t| t.elapsed() >= RECHECK_AFTER);
    if stale {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            check(&app).await;
        });
    }
}

/// 트레이 메뉴: 새 버전이 있으면 설치, 없으면 지금 확인.
pub fn on_menu(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if matches!(app.state::<Updates>().status(), Status::Available { .. }) {
            install(&app).await;
        } else {
            check(&app).await;
        }
    });
}

/// 켠 뒤 잠시 있다가 한 번, 그 뒤로 하루마다 확인한다 (릴리스 빌드만). 대부분 자고 있는
/// 일이라 비동기 작업자 대신 전용 스레드를 쓴다.
pub fn start(app: &AppHandle) {
    if !RELEASE {
        fake_for_dev(app);
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("deck-update".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK);
            loop {
                tauri::async_runtime::block_on(check(&app));
                std::thread::sleep(CHECK_EVERY);
            }
        });
    if let Err(e) = spawned {
        eprintln!("업데이트 확인 스레드를 만들 수 없습니다: {e}");
    }
}

/// 개발 빌드 시험용: `NPDK_FAKE_UPDATE=9.9.9 npm run tauri dev`면 5초 뒤 그 버전이 있는 것처럼
/// 알린다 (알림·OSD 줄·편집기 띠 확인용, 설치는 되지 않는다).
fn fake_for_dev(app: &AppHandle) {
    let Ok(version) = std::env::var("NPDK_FAKE_UPDATE") else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(5));
        set_status(&app, Status::Available { version });
    });
}
