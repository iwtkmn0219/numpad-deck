//! Numpad Deck 앱: 코어를 트레이 상주 앱으로 묶는다.
//!
//! 창은 OSD(항상 숨겨 둔 채 존재)와 설정 편집기(열 때만 존재) 두 가지다. 트레이 왼쪽
//! 클릭으로 편집기를 열고, 오른쪽 클릭 메뉴로 일시정지·종료 등을 한다.

mod editor;
mod mic_badge;
mod osd;
mod runtime;
mod theme;
mod tray;
mod update;

use std::sync::Arc;

use deck_core::action::apps::AppEntry;
use deck_core::config::{Action, Config};
use deck_core::key::{NumpadKey, RawKey};
use deck_core::presets::{self, Lang, Preset};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use osd::Osd;
use runtime::Runtime;
use tray::{
    Tray, MENU_AUTOSTART, MENU_EDITOR, MENU_OPEN_CONFIG, MENU_PAUSE, MENU_QUIT, MENU_STOP_SOUNDS,
    MENU_UPDATE,
};

#[tauri::command]
fn get_config(runtime: State<'_, Arc<Runtime>>) -> Config {
    runtime.config()
}

/// 검증에 실패하면 저장하지 않고 사용자에게 보여줄 이유를 돌려준다.
#[tauri::command]
fn save_config(runtime: State<'_, Arc<Runtime>>, config: Config) -> Result<(), String> {
    runtime.save_config(config)
}

#[tauri::command]
fn test_action(runtime: State<'_, Arc<Runtime>>, label: String, action: Action) {
    runtime.test_action(label, action);
}

/// 설치된 앱 목록. COM 셸 API라 느릴 수 있어 메인 스레드 밖에서 읽는다.
#[tauri::command]
async fn list_apps() -> Result<Vec<AppEntry>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        deck_core::action::with_com(deck_core::action::apps::list)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn start_recording(runtime: State<'_, Arc<Runtime>>) {
    runtime.start_recording();
}

#[tauri::command]
fn cancel_recording(runtime: State<'_, Arc<Runtime>>) {
    runtime.cancel_recording();
}

/// 편집기 화면이 받은 키 하나 (녹화 중). `vk`·`extended`는 화면이 물리 키 위치로 만든다.
#[tauri::command]
fn record_key(
    runtime: State<'_, Arc<Runtime>>,
    vk: u16,
    extended: bool,
    down: bool,
) -> Option<runtime::RecordResult> {
    runtime.record_key(RawKey { vk, extended }, down)
}

/// 편집기 화면이 받은 넘패드 키 (NumLock 꺼짐). 키캡 글자로 받는다.
#[tauri::command]
fn press_deck_key(runtime: State<'_, Arc<Runtime>>, key: String) {
    if let Some(k) = NumpadKey::ALL.into_iter().find(|k| k.label() == key) {
        runtime.press_deck_key(k);
    }
}

/// 프리셋 목록 (편집기의 "프리셋에서 추가", 온보딩).
#[tauri::command]
fn list_presets(lang: Lang) -> Vec<Preset> {
    presets::all(lang)
}

/// 설정 창의 앱 정보. 프런트엔드 `src/editor/SettingsDialog.tsx`의 `AppInfo`와 모양이 같아야 한다.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    config_path: String,
}

#[tauri::command]
fn app_info(app: AppHandle, runtime: State<'_, Arc<Runtime>>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        config_path: runtime.config_path().to_string_lossy().into_owned(),
    }
}

#[tauri::command]
fn open_config(runtime: State<'_, Arc<Runtime>>) {
    runtime.open_config();
}

#[tauri::command]
fn open_config_folder(runtime: State<'_, Arc<Runtime>>) {
    runtime.open_config_folder();
}

/// 지금 언어 (설정의 `language`, 없으면 Windows 표시 언어).
#[tauri::command]
fn get_lang(runtime: State<'_, Arc<Runtime>>) -> Lang {
    runtime.lang()
}

/// Windows 표시 언어 (언어 메뉴의 "자동"이 될 언어).
#[tauri::command]
fn system_lang() -> Lang {
    deck_core::i18n::system()
}

/// 지금 모드 (온보딩 ① NumLock 끄기).
#[tauri::command]
fn get_mode(runtime: State<'_, Arc<Runtime>>) -> tray::Mode {
    runtime.mode()
}

/// 첫 실행이라 온보딩을 보여줘야 하는가.
#[tauri::command]
fn onboarding_pending(runtime: State<'_, Arc<Runtime>>) -> bool {
    runtime.onboarding_pending()
}

/// 온보딩을 끝냈거나 건너뛰었다 (이번 실행에서 다시 띄우지 않는다).
#[tauri::command]
fn finish_onboarding(runtime: State<'_, Arc<Runtime>>) {
    runtime.finish_onboarding();
}

/// Windows 시작 시 실행 (`HKCU\...\Run`)이 켜져 있는가. 읽지 못하면 꺼진 것으로 본다.
fn autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

/// 사용자가 Windows 시작 시 실행을 직접 껐다는 표시 파일 (설정 폴더). 이 PC에만 해당하는
/// 선택이라 공유·백업되는 설정 파일이 아니라 따로 둔다.
fn autostart_off_marker(runtime: &Runtime) -> Option<std::path::PathBuf> {
    runtime
        .config_path()
        .parent()
        .map(|dir| dir.join("autostart-off"))
}

/// 기본은 켜짐: 사용자가 직접 끈 적이 없으면 켠다 (릴리스 빌드만 — 개발용 실행 파일이
/// 로그인 때 뜨면 안 된다).
///
/// 켜져 있어도 다시 등록한다: 등록된 경로가 다른 실행 파일(옮긴 설치, 개발용 실행 파일)이면
/// 지금 실행 파일로 고쳐진다.
fn apply_default_autostart(app: &AppHandle, runtime: &Runtime) {
    if !AUTOSTART_AVAILABLE || autostart_off_marker(runtime).is_some_and(|m| m.exists()) {
        return;
    }
    if let Err(e) = app.autolaunch().enable() {
        eprintln!("Windows 시작 시 실행을 켤 수 없습니다: {e}");
    }
}

/// 개발 빌드는 Windows 시작 시 실행을 바꾸지 않는다. 개발용 실행 파일(콘솔 창이 뜨고 화면은
/// 개발 서버가 있어야 보임)이 로그인 때 뜨게 되기 때문이다 (실제로 겪음).
const AUTOSTART_AVAILABLE: bool = !cfg!(debug_assertions);

/// 로그인 때 자동 실행으로 켜졌다는 표시 (`Run` 값에 붙는 인자)
const AUTOSTART_ARG: &str = "--autostart";

/// 설정 창에 보내는 상태. 프런트엔드 `SettingsDialog.tsx`의 `Autostart`와 모양이 같아야 한다.
#[derive(serde::Serialize)]
struct AutostartState {
    enabled: bool,
    /// 개발 빌드면 false (바꿀 수 없음)
    available: bool,
}

/// Windows 시작 시 실행을 켜거나 끈다 (설정 창, 트레이 메뉴). 끄면 다음 실행 때 다시 켜지
/// 않도록 표시를 남긴다. 실제 상태를 돌려준다.
fn set_autostart_state(app: &AppHandle, runtime: &Runtime, on: bool) -> bool {
    if !AUTOSTART_AVAILABLE {
        return autostart_enabled(app);
    }
    let manager = app.autolaunch();
    let result = if on {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(e) = result {
        eprintln!("Windows 시작 시 실행을 바꿀 수 없습니다: {e}");
    }
    if let Some(marker) = autostart_off_marker(runtime) {
        let _ = if on {
            std::fs::remove_file(&marker)
        } else {
            std::fs::write(&marker, b"")
        };
    }
    let now = autostart_enabled(app);
    runtime.set_autostart_checked(now);
    now
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> AutostartState {
    AutostartState {
        enabled: autostart_enabled(&app),
        available: AUTOSTART_AVAILABLE,
    }
}

#[tauri::command]
fn set_autostart(app: AppHandle, runtime: State<'_, Arc<Runtime>>, on: bool) -> AutostartState {
    AutostartState {
        enabled: set_autostart_state(&app, &runtime, on),
        available: AUTOSTART_AVAILABLE,
    }
}

/// 설정 창: 지금 버전과 업데이트 상태.
#[tauri::command]
fn get_update(app: AppHandle) -> update::State {
    app.state::<update::Updates>().state(&app)
}

/// 설정 창: 지금 확인.
#[tauri::command]
async fn check_update(app: AppHandle) -> update::State {
    update::check(&app).await
}

/// 설정 창: 받아 둔 업데이트 설치 (성공하면 앱이 끝나고 새 버전으로 다시 켜진다).
#[tauri::command]
async fn install_update(app: AppHandle) -> update::State {
    update::install(&app).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 훅이 두 개 걸리면 버튼이 두 번 실행되므로 하나만 띄운다. 두 번째 실행은
        // 이미 떠 있는 앱의 편집기를 연다.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            editor::open(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        // 로그인 때 자동 실행이면 이 인자가 붙는다 (그때만 창 없이 트레이에서 시작)
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([AUTOSTART_ARG])
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .manage(update::Updates::default())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            test_action,
            list_apps,
            start_recording,
            cancel_recording,
            record_key,
            press_deck_key,
            list_presets,
            get_mode,
            get_lang,
            app_info,
            open_config,
            open_config_folder,
            get_autostart,
            set_autostart,
            get_update,
            check_update,
            install_update,
            system_lang,
            onboarding_pending,
            finish_onboarding
        ])
        .setup(|app| {
            let runtime = Arc::new(Runtime::start()?);
            apply_default_autostart(app.handle(), &runtime);
            let tray = Tray::create(
                app.handle(),
                &runtime.view(),
                autostart_enabled(app.handle()),
                AUTOSTART_AVAILABLE,
            )?;
            // OSD를 못 만들어도 덱은 동작해야 한다.
            let osd = Osd::create(app.handle())
                .map_err(|e| eprintln!("OSD 창을 만들 수 없습니다: {e}"))
                .ok()
                .map(Arc::new);
            let mic_badge = Arc::new(mic_badge::MicBadge::new(app.handle()));
            runtime.attach_ui(app.handle().clone(), tray, osd, mic_badge);
            // 첫 실행: 편집기를 열어 온보딩을 보여준다.
            let first_run = runtime.onboarding_pending();
            app.manage(runtime);
            // 첫 실행(온보딩)이거나 사용자가 직접 실행했으면 편집기를 연다. 로그인 때 자동
            // 실행이면 조용히 트레이에만 둔다. 직접 실행했는데 창이 안 뜨면 켜졌는지 알 수 없다.
            update::start(app.handle());
            // 작업 표시줄이 밝게/어둡게 바뀌거나 배율이 바뀌면 트레이 아이콘을 다시 그린다
            {
                let app = app.handle().clone();
                theme::watch(move || {
                    let runtime = app.state::<Arc<Runtime>>();
                    if let Some(tray) = runtime.tray() {
                        tray.refresh_icon();
                    }
                    runtime.reposition_mic_badge();
                });
            }
            let autostarted = std::env::args().any(|a| a == AUTOSTART_ARG);
            if first_run || !autostarted {
                editor::open(app.handle());
            }
            Ok(())
        })
        .on_menu_event(|app, event| {
            let runtime = app.state::<Arc<Runtime>>();
            match event.id().as_ref() {
                MENU_EDITOR => editor::open(app),
                MENU_PAUSE => runtime.toggle_pause(),
                MENU_STOP_SOUNDS => runtime.stop_sounds(),
                MENU_OPEN_CONFIG => runtime.open_config(),
                MENU_UPDATE => update::on_menu(app),
                MENU_AUTOSTART => {
                    set_autostart_state(app, &runtime, !autostart_enabled(app));
                }
                MENU_QUIT => app.exit(0),
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("Numpad Deck을 시작할 수 없습니다")
        // 창을 모두 닫아도 종료하지 않는다 (트레이 상주).
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
