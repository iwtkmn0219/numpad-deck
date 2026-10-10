//! 설정 편집기 창.
//!
//! 필요할 때 만들고 닫으면 파기한다 (상주 메모리를 늘리지 않게). 이미 열려 있으면
//! 앞으로 가져온다.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::osd::BROWSER_ARGS;

pub const LABEL: &str = "editor";

pub fn open(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
        .title("Numpad Deck 설정")
        .inner_size(1040.0, 700.0)
        .min_inner_size(900.0, 620.0)
        .center()
        .additional_browser_args(BROWSER_ARGS)
        .build();
    if let Err(e) = built {
        eprintln!("편집기 창을 열 수 없습니다: {e}");
    }
}
