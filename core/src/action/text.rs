//! 문장 액션: 자리표시자를 펼치고 붙여넣기 또는 한 글자씩 입력으로 보낸다.

use std::thread;

use windows::Win32::System::SystemInformation::GetLocalTime;

use super::blocked;
use crate::clipboard;
use crate::config::TextMethod;
use crate::i18n::{self, tr};
use crate::inject;
use crate::text_template::{self, Moment, TemplateError};

/// 실제로 보내는 방식.
#[derive(Debug, PartialEq, Eq)]
enum Send {
    Paste,
    Type,
}

/// 설정의 방식과 클립보드 보관 결과로 보내는 방식을 정한다. 없으면 `type`이고,
/// 붙여넣기는 사용자 클립보드를 보관한 경우에만 쓴다(보관하지 못하면 잃을 수 있다).
fn plan(method: Option<TextMethod>, snapshot_ok: bool) -> Send {
    match method {
        Some(TextMethod::Paste) if snapshot_ok => Send::Paste,
        _ => Send::Type,
    }
}

fn local_moment() -> Moment {
    let t = unsafe { GetLocalTime() };
    Moment {
        year: t.wYear,
        month: t.wMonth as u8,
        day: t.wDay as u8,
        hour: t.wHour as u8,
        minute: t.wMinute as u8,
        weekday: t.wDayOfWeek as u8,
    }
}

/// 문장 버튼을 실행한다. 자리표시자는 누른 순간의 값으로 펼친다.
pub(super) fn run(text: &str, method: Option<TextMethod>) -> Result<(), String> {
    // `{클립보드}`는 보관(snapshot)보다 먼저 읽는다. 없으면 클립보드를 읽지 않는다.
    let text = text_template::expand(text, &local_moment(), i18n::current(), clipboard::read_text)
        .map_err(|e| match e {
            TemplateError::NoClipboardText => {
                tr("클립보드에 글자가 없어요", "Clipboard has no text")
            }
        })?;

    let snapshot = (method == Some(TextMethod::Paste)).then(clipboard::snapshot);
    let snapshot_ok = snapshot.as_ref().is_some_and(|s| s.is_ok());
    match plan(method, snapshot_ok) {
        Send::Type => {
            if let Some(Err(e)) = &snapshot {
                eprintln!("붙여넣기 대신 입력으로 보냄: 클립보드 보관 실패 ({e})");
            }
            inject::text(&text).map_err(blocked)
        }
        Send::Paste => {
            let Some(Ok(snapshot)) = snapshot else {
                unreachable!("plan이 Paste면 보관에 성공했다");
            };
            paste(&text, snapshot)
        }
    }
}

/// 문장을 클립보드에 넣고 Ctrl+V를 보낸 뒤 사용자 클립보드를 되돌린다.
fn paste(text: &str, snapshot: clipboard::Snapshot) -> Result<(), String> {
    let seq = match clipboard::put_text_private(text) {
        Ok(seq) => seq,
        Err(e) => {
            // 넣다가 실패하면 클립보드가 비워졌을 수 있다. 되돌린 뒤 입력으로 대신 보낸다.
            eprintln!("붙여넣기 대신 입력으로 보냄: 클립보드에 넣기 실패 ({e})");
            log_restore(clipboard::restore(snapshot, clipboard::sequence()));
            return inject::text(text).map_err(blocked);
        }
    };
    // 보내기가 막혀도 클립보드는 되돌려야 하므로 결과는 나중에 돌려준다.
    let sent = inject::paste_shortcut();
    // 대상 앱이 클립보드를 늦게 읽을 수 있어 기다린다.
    thread::sleep(clipboard::RESTORE_DELAY);
    log_restore(clipboard::restore(snapshot, seq));
    sent.map_err(blocked)
}

/// 되돌리기 실패는 사용자에게 알리지 않고 기록만 남긴다.
fn log_restore(result: Result<bool, String>) {
    if let Err(e) = result {
        eprintln!("클립보드 되돌리기 실패: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_defaults_to_typing() {
        assert_eq!(plan(None, true), Send::Type);
        assert_eq!(plan(None, false), Send::Type);
        assert_eq!(plan(Some(TextMethod::Type), true), Send::Type);
    }

    #[test]
    fn plan_pastes_only_when_snapshot_succeeded() {
        assert_eq!(plan(Some(TextMethod::Paste), true), Send::Paste);
        assert_eq!(plan(Some(TextMethod::Paste), false), Send::Type);
    }
}
