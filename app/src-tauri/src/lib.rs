//! Numpad Deck 앱: 코어를 트레이 상주 앱으로 묶는다.
//!
//! 창은 아직 없다 (OSD는 2-5, 편집기는 3단계). 트레이 메뉴로 일시정지·설정 파일
//! 열기·종료를 한다.

mod runtime;
mod tray;

use std::sync::Arc;

use tauri::Manager;

use runtime::Runtime;
use tray::{Tray, MENU_OPEN_CONFIG, MENU_PAUSE, MENU_QUIT};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 훅이 두 개 걸리면 버튼이 두 번 실행되므로 하나만 띄운다.
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .setup(|app| {
            let runtime = Arc::new(Runtime::start()?);
            let tray = Tray::create(app.handle(), &runtime.view())?;
            runtime.attach_tray(tray);
            app.manage(runtime);
            Ok(())
        })
        .on_menu_event(|app, event| {
            let runtime = app.state::<Arc<Runtime>>();
            match event.id().as_ref() {
                MENU_PAUSE => runtime.toggle_pause(),
                MENU_OPEN_CONFIG => runtime.open_config(),
                MENU_QUIT => app.exit(0),
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("Numpad Deck을 시작할 수 없습니다")
        // 창이 없어도 종료하지 않는다 (트레이 상주).
        .run(|_app, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
