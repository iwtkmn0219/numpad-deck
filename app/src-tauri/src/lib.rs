//! Numpad Deck 앱: 코어를 트레이 상주 앱으로 묶는다.
//!
//! 창은 OSD(항상 숨겨 둔 채 존재)와 설정 편집기(열 때만 존재) 두 가지다. 트레이 왼쪽
//! 클릭으로 편집기를 열고, 오른쪽 클릭 메뉴로 일시정지·종료 등을 한다.

mod editor;
mod osd;
mod runtime;
mod tray;

use std::sync::Arc;

use deck_core::config::Config;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 훅이 두 개 걸리면 버튼이 두 번 실행되므로 하나만 띄운다. 두 번째 실행은
        // 이미 떠 있는 앱의 편집기를 연다.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            editor::open(app);
        }))
        .invoke_handler(tauri::generate_handler![get_config, save_config])
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
