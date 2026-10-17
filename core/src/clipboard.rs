//! 클립보드: 문장 붙여넣기용 보관·넣기·되돌리기.
//!
//! 순서는 액션 실행 스레드에서 [`snapshot`] → [`put_text_private`] → Ctrl+V →
//! [`RESTORE_DELAY`] 대기 → [`restore`]다. 넣은 문장과 되돌린 내용에는 기록 제외
//! 형식을 붙여 Win+V 기록·클라우드 동기화·클립보드 관리자에 남지 않게 한다.
//!
//! 클립보드는 창 없이(`OpenClipboard(None)`) 연다. 다른 앱이 잡고 있으면 짧게 다시
//! 시도한다. 실제 클립보드를 쓰는 함수는 사용자 클립보드를 건드리므로 자동 시험에
//! 넣지 않고, 판단은 순수 함수로 떼어 시험한다.

use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    GetLastError, GlobalFree, SetLastError, ERROR_SUCCESS, HANDLE, HGLOBAL,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardFormatNameW, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::{
    CF_BITMAP, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT, CF_DSPTEXT, CF_ENHMETAFILE,
    CF_GDIOBJFIRST, CF_GDIOBJLAST, CF_METAFILEPICT, CF_OWNERDISPLAY, CF_PALETTE, CF_PRIVATEFIRST,
    CF_PRIVATELAST, CF_UNICODETEXT,
};

/// Ctrl+V를 보낸 뒤 되돌리기까지 기다리는 시간. 대상 앱이 클립보드를 늦게 읽을 수
/// 있어서 기다린다. 체크리스트 결과로 조정한다.
pub const RESTORE_DELAY: Duration = Duration::from_millis(400);

/// 보관할 수 있는 합계 바이트. 넘으면 보관하지 않고 직접 입력으로 대신 보낸다.
const SNAPSHOT_LIMIT: usize = 32 * 1024 * 1024;

/// `OpenClipboard` 재시도 횟수와 간격 (10ms × 10회).
const OPEN_TRIES: u32 = 10;
const OPEN_RETRY_WAIT: Duration = Duration::from_millis(10);

/// 기록 제외 형식 이름. 값은 모두 DWORD 0이다.
/// `ExcludeClipboardContentFromMonitorProcessing`은 내용과 무관하게 있기만 하면 되고,
/// 나머지 둘은 0이 "허용 안 함"이다.
const PRIVATE_FORMAT_NAMES: [&str; 3] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard",
];
const PRIVATE_FORMAT_VALUE: [u8; 4] = 0u32.to_le_bytes();

/// 보관해 둔 클립보드 내용. `HGLOBAL` 기반 형식의 바이트만 담는다.
/// 비어 있으면 보관할 때 클립보드가 비어 있었던 것이다.
pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
}

impl Snapshot {
    /// 보관한 형식 수와 합계 바이트(진단용).
    pub fn summary(&self) -> (usize, usize) {
        let bytes = self.formats.iter().map(|(_, b)| b.len()).sum();
        (self.formats.len(), bytes)
    }
}

/// 클립보드의 글자(`CF_UNICODETEXT`)를 읽는다. `\r\n`은 `\n`으로 바꾼다.
/// 열기 실패·글자 없음(비었음, 그림만 있음)·빈 글자면 `None`.
pub fn read_text() -> Option<String> {
    let _clipboard = open().ok()?;
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0 as u32) }.ok()?;
    let locked = Locked::new(HGLOBAL(handle.0)).ok()?;
    // 글자는 NUL에서 끝난다. 블록이 요청 크기보다 클 수 있어 GlobalSize만 믿지 않는다.
    let units: Vec<u16> = locked
        .bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&b| u16::from_le_bytes(b))
        .take_while(|&u| u != 0)
        .collect();
    let text = String::from_utf16_lossy(&units).replace("\r\n", "\n");
    (!text.is_empty()).then_some(text)
}

/// 지금 클립보드 내용을 보관한다. 합계가 32MiB를 넘거나 한 형식이라도 읽지 못하면
/// `Err` — 일부만 보관하면 되돌릴 때 사용자 클립보드를 잃기 때문이다. 다만 OLE 데이터
/// 객체가 알리기만 하고 주지 않는 등록 형식은 건너뛴다(`may_skip_unreadable`).
pub fn snapshot() -> Result<Snapshot, String> {
    let private = private_formats()?;
    let _clipboard = open()?;
    let ole = is_ole_clipboard();
    let mut skipped = 0usize;
    let mut formats = Vec::new();
    let mut total = 0usize;
    let mut format = 0u32;
    loop {
        // 끝에 닿았을 때와 실패를 GetLastError로 가린다. 이전 값이 남지 않게 먼저 지운다.
        unsafe { SetLastError(ERROR_SUCCESS) };
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_SUCCESS {
                return Err(format!("EnumClipboardFormats: {error:?}"));
            }
            break;
        }
        // 기록 제외 형식은 되돌릴 때 새로 붙인다.
        if should_skip_format(format) || private.contains(&format) {
            continue;
        }
        // OLE 데이터 객체 정보는 소유자 없이 되돌리면 낡은 정보가 남아 OleGetClipboard가
        // 잘못 쓸 수 있다. 가상 파일 형식은 OLE로만 읽힌다. 둘 다 이름으로 가린다.
        let name = format_name(format);
        if name.as_deref().is_some_and(should_skip_format_name) {
            continue;
        }
        let label = match &name {
            Some(name) => format!("{format} \"{name}\""),
            None => format.to_string(),
        };
        // NULL이면 windows-rs는 GetLastError로 Err를 만든다. 오류 없는 NULL(코드가 S_OK)과
        // 진짜 실패를 가르려고 먼저 지운다.
        unsafe { SetLastError(ERROR_SUCCESS) };
        let handle = match unsafe { GetClipboardData(format) } {
            Ok(handle) => handle,
            // OLE 클립보드는 데이터 객체가 HGLOBAL로 주지 않는 등록 형식도 알린다(메모장 글자의
            // `EnterpriseDataProtectionId` 등). 그런 형식은 오류 없이 NULL이고 되돌릴 수도 없다.
            Err(e) if may_skip_unreadable(format, name.as_deref(), ole, e.code().is_ok()) => {
                eprintln!("클립보드 보관: 읽을 수 없는 형식 {label} 건너뜀");
                skipped += 1;
                continue;
            }
            Err(e) => return Err(win_error(&format!("GetClipboardData({label})"), &e)),
        };
        let locked = Locked::new(HGLOBAL(handle.0)).map_err(|e| format!("format {label}: {e}"))?;
        let bytes = locked.bytes();
        total = total.saturating_add(bytes.len());
        if !within_limit(total) {
            return Err(format!("clipboard larger than {SNAPSHOT_LIMIT} bytes"));
        }
        formats.push((format, bytes.to_vec()));
    }
    // 건너뛰기만 하고 하나도 못 읽었으면 되돌릴 것이 없다. 비우게 되므로 실패로 본다.
    if skipped > 0 && formats.is_empty() {
        return Err(format!("no readable clipboard format ({skipped} skipped)"));
    }
    Ok(Snapshot { formats })
}

/// [`put_text_private`] 실패. 클립보드를 이미 비웠는지 함께 알려 준다.
#[derive(Debug)]
pub struct PutError {
    /// true면 클립보드를 비웠으므로 보관한 내용을 되돌려야 한다.
    pub emptied: bool,
    pub msg: String,
}

impl std::fmt::Display for PutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

/// 지금 클립보드 순번.
pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// 문장을 클립보드에 넣는다(줄바꿈은 `\r\n`). 기록 제외 형식을 같이 넣고, 닫은 뒤의
/// 클립보드 순번을 돌려준다. [`restore`]에 그대로 넘긴다.
///
/// `Err`이면 [`PutError::emptied`]로 클립보드를 비웠는지 알 수 있다. 비웠다면 부르는 쪽이
/// `restore(snapshot, sequence())`로 보관한 내용을 되돌려야 하고, 비우지 않았다면(열기 전에
/// 실패) 클립보드는 그대로이므로 되돌리지 않아야 한다(OLE·지연 렌더링 형식이 사라진다).
pub fn put_text_private(text: &str) -> Result<u32, PutError> {
    let untouched = |msg| PutError {
        emptied: false,
        msg,
    };
    let private = private_formats().map_err(untouched)?;
    let mut bytes: Vec<u8> = to_crlf(text)
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    // CF_UNICODETEXT는 NUL로 끝나야 한다.
    bytes.extend_from_slice(&[0, 0]);
    {
        let _clipboard = open().map_err(untouched)?;
        // 비우기가 실패해도 일부만 지워졌을 수 있어 비운 것으로 본다.
        empty().map_err(|msg| PutError { emptied: true, msg })?;
        // 기록 제외 형식을 먼저 넣는다. 하나라도 실패하면 다시 비워서, 표시 없는 문장이
        // 닫을 때 그대로 공개되지 않게 한다.
        let result =
            set_private(&private).and_then(|()| set_bytes(CF_UNICODETEXT.0 as u32, &bytes));
        if let Err(msg) = result {
            let _ = empty();
            return Err(PutError { emptied: true, msg });
        }
    }
    // 닫은 뒤에 읽는다. 닫은 다음부터는 다른 앱이 바꿔야만 순번이 바뀐다.
    Ok(sequence())
}

/// 보관한 내용을 되돌린다. 순번이 `expected_seq`와 다르면 그사이 사용자나 다른 앱이
/// 클립보드를 바꾼 것이므로 아무것도 하지 않고 `Ok(false)`. 빈 스냅숏이면 비우기만 한다.
///
/// 기록 제외 형식을 먼저 넣고, 그게 실패하면 다시 비운 채로 `Err`이다(표시 없는 내용을
/// 공개하지 않는다). 표시를 넣은 뒤에는 한 형식이 실패해도 나머지를 되살리고 첫 오류를 알린다.
pub fn restore(snapshot: Snapshot, expected_seq: u32) -> Result<bool, String> {
    if !should_restore(expected_seq, sequence()) {
        return Ok(false);
    }
    let private = private_formats()?;
    let _clipboard = open()?;
    // 확인과 열기 사이에 바뀌었을 수 있다. 열어 둔 동안에는 아무도 못 바꾸므로 한 번 더 본다.
    if !should_restore(expected_seq, sequence()) {
        return Ok(false);
    }
    empty()?;
    if snapshot.formats.is_empty() {
        return Ok(true);
    }
    // 이미 기록에 있던 내용이 한 번 더 쌓이지 않게 되돌린 내용에도 붙인다. 먼저 넣는다.
    if let Err(e) = set_private(&private) {
        let _ = empty();
        return Err(e);
    }
    // 표시가 붙었으므로 한 형식이 실패해도 나머지는 되살린다. 오류는 처음 것만 알린다.
    let mut first_error = None;
    for (format, bytes) in &snapshot.formats {
        if let Err(e) = set_bytes(*format, bytes) {
            first_error.get_or_insert(e);
        }
    }
    match first_error {
        None => Ok(true),
        Some(e) => Err(e),
    }
}

/// 보관하지 않는 형식. GDI 핸들 형식(`CF_BITMAP` 등)과 표시 전용 형식(`CF_OWNERDISPLAY`,
/// `CF_DSP*`)은 바이트로 복사할 수 있는 `HGLOBAL`이 아니다. 그림은 `CF_DIB`로 되살아난다.
/// GDI 객체 범위와 앱 전용 범위(`CF_GDIOBJ*`·`CF_PRIVATE*`)도 핸들 종류를 알 수 없어 건너뛴다.
fn should_skip_format(format: u32) -> bool {
    let skipped = [
        CF_BITMAP,
        CF_ENHMETAFILE,
        CF_PALETTE,
        CF_METAFILEPICT,
        CF_OWNERDISPLAY,
        CF_DSPTEXT,
        CF_DSPBITMAP,
        CF_DSPMETAFILEPICT,
        CF_DSPENHMETAFILE,
    ];
    let in_range = |first: u16, last: u16| (first as u32..=last as u32).contains(&format);
    skipped.iter().any(|f| f.0 as u32 == format)
        || in_range(CF_PRIVATEFIRST.0, CF_PRIVATELAST.0)
        || in_range(CF_GDIOBJFIRST.0, CF_GDIOBJLAST.0)
}

/// 이름으로 보관하지 않는 등록 형식. OLE 클립보드가 쓰는 데이터 객체 정보와 가상 파일이다.
/// `FileContents`는 OLE 스트림으로만 주므로(탐색기에서 파일을 복사하면 생기고
/// GetClipboardData는 NULL) 읽을 수 없고, 내용 없이 되돌린 `FileGroupDescriptor(W)`는 받을 수
/// 없는 파일을 알려 붙여넣는 앱이 실패할 수 있다. 파일은 `CF_HDROP`·`Shell IDList Array`로
/// 되살아난다.
fn should_skip_format_name(name: &str) -> bool {
    matches!(
        name,
        "DataObject"
            | "Ole Private Data"
            | "FileContents"
            | "FileGroupDescriptor"
            | "FileGroupDescriptorW"
    )
}

/// 읽지 못한 형식을 건너뛰어도 되는지. OLE 클립보드(`DataObject`가 있음)의 등록 형식이
/// 오류 코드 없이 NULL일 때만 — 데이터 객체가 그 형식을 HGLOBAL로 주지 않는 것이다.
/// 미리 정의된 형식(글자·그림·파일), 서식 있는 내용 형식(`is_content_format_name`), 다른
/// 실패는 여전히 보관 실패로 본다 — 잠깐 못 읽은 것을 빼고 되돌리면 서식을 잃는다.
fn may_skip_unreadable(format: u32, name: Option<&str>, ole: bool, no_error_code: bool) -> bool {
    ole && no_error_code && format >= 0xC000 && !name.is_some_and(is_content_format_name)
}

/// 건너뛰면 안 되는 등록 형식: 웹·오피스·그림의 서식 있는 내용.
fn is_content_format_name(name: &str) -> bool {
    matches!(
        name,
        "HTML Format" | "Rich Text Format" | "PNG" | "GIF" | "JFIF" | "Csv" | "XML Spreadsheet"
    ) || name.starts_with("Biff")
}

/// 열린 클립보드가 OLE 데이터 객체로 채워졌는지(`DataObject` 형식이 있는지).
fn is_ole_clipboard() -> bool {
    let wide: Vec<u16> = "DataObject".encode_utf16().chain(Some(0)).collect();
    let format = unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) };
    format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
}

/// 등록 형식의 이름. 미리 정의된 형식이나 실패면 `None`.
fn format_name(format: u32) -> Option<String> {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClipboardFormatNameW(format, &mut buf) };
    (len > 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
}

/// 보관 합계가 한도 안인지.
fn within_limit(total: usize) -> bool {
    total <= SNAPSHOT_LIMIT
}

/// 되돌려도 되는지: 넣은 뒤 순번이 그대로일 때만.
fn should_restore(expected: u32, current: u32) -> bool {
    expected == current
}

/// 줄바꿈을 `\r\n`으로. 이미 `\r\n`인 것은 그대로 둔다.
fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// 열린 클립보드. 놓을 때 닫는다 — 어느 경로로 나가든 `CloseClipboard`가 불린다.
struct Clipboard;

impl Drop for Clipboard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// 창 없이 연다. 다른 앱이 잡고 있으면 10ms 간격으로 10번까지 시도한다.
fn open() -> Result<Clipboard, String> {
    let mut last = None;
    for attempt in 0..OPEN_TRIES {
        if attempt > 0 {
            thread::sleep(OPEN_RETRY_WAIT);
        }
        match unsafe { OpenClipboard(None) } {
            Ok(()) => return Ok(Clipboard),
            Err(e) => last = Some(e),
        }
    }
    Err(match last {
        Some(e) => win_error("OpenClipboard", &e),
        None => "OpenClipboard failed".into(),
    })
}

fn empty() -> Result<(), String> {
    unsafe { EmptyClipboard() }.map_err(|e| win_error("EmptyClipboard", &e))
}

/// 잠근 `HGLOBAL`. 놓을 때 푼다 — 잠금과 풀기가 늘 짝을 이룬다.
struct Locked {
    handle: HGLOBAL,
    ptr: *mut u8,
    len: usize,
}

impl Locked {
    /// 실패 문구는 고정이다. GetLastError가 0일 때도 있어 오류 코드로는 뜻이 없다.
    fn new(handle: HGLOBAL) -> Result<Self, &'static str> {
        // 크기는 블록 자체에서 얻는다. 0은 실패이거나 버려진 블록이라 읽을 수 없다.
        let len = unsafe { GlobalSize(handle) };
        if len == 0 {
            return Err("GlobalSize returned 0 (invalid or discarded memory)");
        }
        let ptr = unsafe { GlobalLock(handle) } as *mut u8;
        if ptr.is_null() {
            return Err("GlobalLock returned NULL");
        }
        Ok(Self { handle, ptr, len })
    }

    fn bytes(&self) -> &[u8] {
        // 잠근 동안 ptr부터 len 바이트가 유효하다.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        // 마지막 풀기는 0(실패 모양)을 돌려주므로 결과는 보지 않는다.
        let _ = unsafe { GlobalUnlock(self.handle) };
    }
}

/// 열린 클립보드에 한 형식을 넣는다. 성공하면 메모리는 시스템 소유가 되고,
/// 실패했을 때만 직접 푼다.
fn set_bytes(format: u32, bytes: &[u8]) -> Result<(), String> {
    // GMEM_MOVEABLE: 클립보드에 넣는 메모리는 옮길 수 있어야 한다. 0바이트 블록은 버려진
    // 블록이 되므로 최소 1바이트를 잡는다.
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }
        .map_err(|e| win_error("GlobalAlloc", &e))?;
    match Locked::new(handle) {
        Ok(mut locked) => {
            // 블록이 요청보다 클 수 있다. 남는 부분은 0으로 채운다.
            let dest = locked.bytes_mut();
            dest[..bytes.len()].copy_from_slice(bytes);
            dest[bytes.len()..].fill(0);
        }
        Err(e) => {
            let _ = unsafe { GlobalFree(Some(handle)) };
            return Err(e.to_string());
        }
    }
    // 잠금을 푼 뒤에 넘긴다.
    match unsafe { SetClipboardData(format, Some(HANDLE(handle.0))) } {
        Ok(_) => Ok(()),
        Err(e) => {
            // 실패하면 시스템이 가져가지 않았으므로 우리가 푼다. GlobalFree는 성공하면
            // NULL을 돌려줘 windows-rs에서는 Err 모양이므로 결과는 보지 않는다.
            let _ = unsafe { GlobalFree(Some(handle)) };
            Err(win_error(&format!("SetClipboardData({format})"), &e))
        }
    }
}

/// 기록 제외 형식 셋을 넣는다.
fn set_private(private: &[u32; 3]) -> Result<(), String> {
    for &format in private {
        set_bytes(format, &PRIVATE_FORMAT_VALUE)?;
    }
    Ok(())
}

/// 기록 제외 형식 번호. 셋 다 등록되면 기억해 두고 다시 쓴다. 등록에 실패하면(0) 기억하지
/// 않고 다음에 다시 시도한다. 실패하면 기록에 남을 수 있으므로 `Err` — 부르는 쪽은 직접
/// 입력으로 대신 보낸다.
fn private_formats() -> Result<[u32; 3], String> {
    static FORMATS: OnceLock<[u32; 3]> = OnceLock::new();
    if let Some(&formats) = FORMATS.get() {
        return Ok(formats);
    }
    let formats = PRIVATE_FORMAT_NAMES.map(|name| {
        let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
        unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) }
    });
    if formats.contains(&0) {
        return Err("RegisterClipboardFormatW failed".into());
    }
    // 같은 이름은 늘 같은 번호라 동시에 등록해도 결과가 같다.
    Ok(*FORMATS.get_or_init(|| formats))
}

/// windows-rs 오류를 문구로. GetLastError가 0이면 오류에 S_OK가 담겨 뜻이 없으므로
/// 고정 문구만 쓴다.
fn win_error(what: &str, e: &windows::core::Error) -> String {
    if e.code().is_ok() {
        format!("{what} failed")
    } else {
        format!("{what} failed: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Ole::{CF_DIB, CF_HDROP};

    #[test]
    fn skips_gdi_handle_formats() {
        assert!(should_skip_format(CF_BITMAP.0 as u32));
        assert!(should_skip_format(CF_ENHMETAFILE.0 as u32));
        assert!(should_skip_format(CF_OWNERDISPLAY.0 as u32));
        assert!(should_skip_format(CF_DSPTEXT.0 as u32));
        assert!(!should_skip_format(CF_UNICODETEXT.0 as u32));
        assert!(!should_skip_format(CF_DIB.0 as u32));
        assert!(!should_skip_format(CF_HDROP.0 as u32));
        // 등록 형식(HTML·RTF 등)은 0xC000 이상이고 HGLOBAL이다.
        assert!(!should_skip_format(0xC0FF));
    }

    #[test]
    fn skips_ole_private_format_names() {
        assert!(should_skip_format_name("DataObject"));
        assert!(should_skip_format_name("Ole Private Data"));
        assert!(should_skip_format_name("FileContents"));
        assert!(should_skip_format_name("FileGroupDescriptor"));
        assert!(should_skip_format_name("FileGroupDescriptorW"));
        assert!(!should_skip_format_name("FileNameW"));
        assert!(!should_skip_format_name("HTML Format"));
        assert!(!should_skip_format_name("Rich Text Format"));
        assert!(!should_skip_format_name("dataobject"));
    }

    #[test]
    fn skips_unreadable_only_for_ole_registered_formats() {
        let edp = Some("EnterpriseDataProtectionId");
        assert!(may_skip_unreadable(0xC0BF, edp, true, true));
        assert!(may_skip_unreadable(0xC0BF, None, true, true));
        // OLE가 아니거나, 오류 코드가 있거나, 미리 정의된 형식이면 보관 실패.
        assert!(!may_skip_unreadable(0xC0BF, edp, false, true));
        assert!(!may_skip_unreadable(0xC0BF, edp, true, false));
        assert!(!may_skip_unreadable(
            CF_UNICODETEXT.0 as u32,
            None,
            true,
            true
        ));
        assert!(!may_skip_unreadable(CF_HDROP.0 as u32, None, true, true));
        // 서식 있는 내용은 건너뛰지 않는다.
        for name in [
            "HTML Format",
            "Rich Text Format",
            "PNG",
            "Csv",
            "Biff12",
            "Biff8",
        ] {
            assert!(
                !may_skip_unreadable(0xC0BF, Some(name), true, true),
                "{name}"
            );
        }
    }

    #[test]
    fn limit_is_32_mib() {
        assert!(within_limit(32 * 1024 * 1024));
        assert!(!within_limit(32 * 1024 * 1024 + 1));
    }

    #[test]
    fn restores_only_when_sequence_matches() {
        assert!(should_restore(5, 5));
        assert!(!should_restore(5, 6));
    }

    #[test]
    fn converts_newlines_to_crlf() {
        assert_eq!(to_crlf("a\nb"), "a\r\nb");
        assert_eq!(to_crlf("a\r\nb"), "a\r\nb");
        assert_eq!(to_crlf("a\n\nb\r\n"), "a\r\n\r\nb\r\n");
    }
}
