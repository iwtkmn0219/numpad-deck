//! Numpad Deck 앱: 코어를 트레이 상주 앱으로 묶는다.
//!
//! 창은 OSD(항상 숨겨 둔 채 존재)와 설정 편집기(열 때만 존재) 두 가지다. 트레이 왼쪽
//! 클릭으로 편집기를 열고, 오른쪽 클릭 메뉴로 일시정지·종료 등을 한다.

mod editor;
mod osd;
mod runtime;
mod tray;

use std::sync::Arc;

use deck_core::action::apps::AppEntry;
use deck_core::config::{Action, Config};
use tauri::{Manager, State};

use osd::Osd;
use runtime::Runtime;
use tray::{Tray, MENU_EDITOR, MENU_OPEN_CONFIG, MENU_PAUSE, MENU_QUIT};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 훅이 두 개 걸리면 버튼이 두 번 실행되므로 하나만 띄운다. 두 번째 실행은
        // 이미 떠 있는 앱의 편집기를 연다.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            editor::open(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            test_action,
            list_apps,
            start_recording,
            cancel_recording
        ])
        .setup(|app| {
            let runtime = Arc::new(Runtime::start()?);
            let tray = Tray::create(app.handle(), &runtime.view())?;
            // OSD를 못 만들어도 덱은 동작해야 한다.
            let osd = Osd::create(app.handle())
                .map_err(|e| eprintln!("OSD 창을 만들 수 없습니다: {e}"))
                .ok()
                .map(Arc::new);
            runtime.attach_ui(app.handle().clone(), tray, osd);
            app.manage(runtime);
            Ok(())
        })
        .on_menu_event(|app, event| {
            let runtime = app.state::<Arc<Runtime>>();
            match event.id().as_ref() {
                MENU_EDITOR => editor::open(app),
                MENU_PAUSE => runtime.toggle_pause(),
                MENU_OPEN_CONFIG => runtime.open_config(),
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
