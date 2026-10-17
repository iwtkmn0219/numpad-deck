//! 페이지 공유 파일(`.npdeck`) 형식과 경로 이식성.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::action::expand_env;
use crate::config::{Action, Config, KeyPlace, Page, SystemKeys};
use crate::i18n::{tr, Lang};

/// 파일 형식 번호.
pub const FORMAT: u32 = 1;
/// 공유 파일 확장자.
pub const EXTENSION: &str = "npdeck";
/// 소리 파일 하나의 최대 크기.
pub const MAX_SOUND: u64 = 5 * 1024 * 1024;
/// 소리 파일 합계 최대 크기.
pub const MAX_SOUNDS_TOTAL: u64 = 20 * 1024 * 1024;
/// 묶음 파일 전체 최대 크기.
pub const MAX_FILE: u64 = 30 * 1024 * 1024;
/// 묶음 파일의 최대 항목 수.
pub const MAX_ENTRIES: usize = 512;
/// manifest.json 최대 크기.
pub const MAX_MANIFEST: u64 = 1024 * 1024;
/// 허용하는 소리 확장자 (rodio 기능과 같음).
pub const SOUND_EXTS: [&str; 4] = ["wav", "mp3", "ogg", "flac"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub enum Kind {
    Pages,
    Backup,
}

/// manifest.json. `kind`가 `Backup`일 때만 아래 선택 항목이 있다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub kind: Kind,
    pub app: String,
    pub pages: Vec<Page>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<Lang>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<Vec<KeyPlace>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_keys: Option<SystemKeys>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mic_indicator: Option<bool>,
}

/// 내 사용자 폴더 값 (시험에서 주입).
pub struct UserDirs {
    pub local_app_data: String,
    pub app_data: String,
    pub user_profile: String,
}

impl UserDirs {
    /// 환경 변수에서 읽는다.
    pub fn current() -> UserDirs {
        let get = |k: &str| std::env::var(k).unwrap_or_default();
        UserDirs {
            local_app_data: get("LOCALAPPDATA"),
            app_data: get("APPDATA"),
            user_profile: get("USERPROFILE"),
        }
    }
}

/// 경로 앞부분이 사용자 폴더면 환경 변수로 바꾼다 (긴 것부터, 대소문자 무시, 구분자 `\`·`/` 모두).
/// URL·`shell:`·`%`로 시작하는 값은 그대로 둔다.
pub fn portable_path(path: &str, dirs: &UserDirs) -> String {
    if path.starts_with('%') || path.starts_with("shell:") || path.contains("://") {
        return path.to_string();
    }
    // 긴 폴더가 먼저 맞도록 길이 순으로 본다 (LOCALAPPDATA가 USERPROFILE보다 우선).
    let mut cands = [
        ("%LOCALAPPDATA%", &dirs.local_app_data),
        ("%APPDATA%", &dirs.app_data),
        ("%USERPROFILE%", &dirs.user_profile),
    ];
    cands.sort_by_key(|(_, d)| std::cmp::Reverse(d.len()));
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let p = norm(path);
    for (var, dir) in cands {
        let d = norm(dir.trim_end_matches(['\\', '/']));
        if d.is_empty() || !p.starts_with(&d) {
            continue;
        }
        // 소문자로 바꾸면 바이트 길이가 달라질 수 있어 글자 수로 자른다.
        let rest: String = path.chars().skip(d.chars().count()).collect();
        if rest.is_empty() || rest.starts_with(['\\', '/']) {
            return format!("{var}{}", rest.replace('/', "\\"));
        }
    }
    path.to_string()
}

/// 페이지 안 모든 동작(묶음 안 포함)에 f를 적용한다.
pub fn for_each_action_mut(page: &mut Page, f: &mut impl FnMut(&mut Action)) {
    fn walk(a: &mut Action, f: &mut impl FnMut(&mut Action)) {
        f(a);
        if let Action::Sequence { steps } = a {
            for s in steps {
                walk(s, f);
            }
        }
    }
    for b in page.buttons.values_mut() {
        walk(&mut b.action, f);
    }
}

/// 페이지 안 모든 동작(묶음 안 포함)을 돌며 f를 부른다.
pub fn for_each_action(page: &Page, f: &mut impl FnMut(&Action)) {
    fn walk(a: &Action, f: &mut impl FnMut(&Action)) {
        f(a);
        if let Action::Sequence { steps } = a {
            for s in steps {
                walk(s, f);
            }
        }
    }
    for b in page.buttons.values() {
        walk(&b.action, f);
    }
}

/// 내보내기 대상.
pub enum Selection {
    /// 설정 전체 (백업)
    All,
    /// 고른 페이지 (0부터)
    Pages(Vec<usize>),
}

#[derive(Debug)]
pub enum ShareError {
    /// 덱 파일이 아님 (zip이 아니거나 manifest가 없음)
    NotDeckFile,
    Corrupt(String),
    /// 더 새로운 앱이 만든 파일 (형식 번호)
    TooNew(u32),
    TooBig,
    /// 내보낼 효과음의 합계가 한도를 넘음
    SoundsTooBig,
    /// 너무 큰 효과음 (파일 이름)
    SoundTooBig(String),
    /// 버튼 설정이 맞지 않음 (메시지)
    Invalid(String),
    Io(String),
}

impl fmt::Display for ShareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mb = MAX_SOUND / 1024 / 1024;
        let total = MAX_SOUNDS_TOTAL / 1024 / 1024;
        let s = match self {
            ShareError::NotDeckFile => tr(
                "Numpad Deck 파일이 아니에요.",
                "This is not a Numpad Deck file.",
            ),
            ShareError::Corrupt(e) => tr(
                format!("파일이 손상된 것 같아요. ({e})"),
                format!("The file seems to be damaged. ({e})"),
            ),
            ShareError::TooNew(_) => tr(
                "더 새로운 Numpad Deck에서 만든 파일이에요. 앱을 업데이트해 주세요.",
                "This file was made with a newer Numpad Deck. Please update the app.",
            ),
            ShareError::TooBig => tr(
                "파일이 너무 커서 열 수 없어요.",
                "The file is too big to open.",
            ),
            ShareError::SoundsTooBig => tr(
                format!("효과음을 모두 합쳐 {total}MB까지만 담을 수 있어요."),
                format!("Sounds can add up to {total} MB at most."),
            ),
            ShareError::SoundTooBig(name) => tr(
                format!("효과음 ‘{name}’이 너무 커서 담을 수 없어요({mb}MB까지)."),
                format!("The sound \u{2018}{name}\u{2019} is too big to include (up to {mb} MB)."),
            ),
            ShareError::Invalid(e) => tr(
                format!("버튼 설정이 맞지 않아요. {e}"),
                format!("The button settings are not valid. {e}"),
            ),
            ShareError::Io(e) => tr(
                format!("파일을 쓰거나 읽지 못했어요. ({e})"),
                format!("Could not write or read the file. ({e})"),
            ),
        };
        f.write_str(&s)
    }
}

impl std::error::Error for ShareError {}

/// 내보내기 전에 알려 줄 것.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct ExportReport {
    /// 텍스트 입력 동작 수 (묶음 안 포함). 비밀번호 같은 글자가 담길 수 있다.
    pub text_buttons: usize,
    /// 파일이 없는 효과음
    pub missing_sounds: Vec<String>,
    /// 5 MiB를 넘는 효과음
    pub too_big_sounds: Vec<String>,
    /// 담을 효과음 합계가 20 MiB를 넘음
    pub sounds_total_too_big: bool,
}

/// 읽어 낸 묶음. 소리 키 = `sounds/<해시>.<확장자>`.
pub struct Bundle {
    pub manifest: Manifest,
    pub sounds: BTreeMap<String, Vec<u8>>,
}

const MANIFEST_NAME: &str = "manifest.json";

fn io_err(e: impl fmt::Display) -> ShareError {
    ShareError::Io(e.to_string())
}

/// 항목 이름이 `sounds/<소문자 hex 16자>.(wav|mp3|ogg|flac)`인가.
fn is_sound_entry(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("sounds/") else {
        return false;
    };
    let Some((stem, ext)) = rest.split_once('.') else {
        return false;
    };
    stem.len() == 16
        && stem.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        && SOUND_EXTS.contains(&ext)
}

fn selected_pages(config: &Config, sel: &Selection) -> Vec<Page> {
    match sel {
        Selection::All => config.pages.clone(),
        Selection::Pages(idx) => idx
            .iter()
            .filter_map(|&i| config.pages.get(i).cloned())
            .collect(),
    }
}

/// 효과음 파일 하나의 디스크 상태.
enum SoundFile {
    Missing,
    /// 담을 수 없는 확장자: 있어도 담지 않는다.
    Skipped,
    Found {
        path: PathBuf,
        ext: String,
        size: u64,
    },
}

fn probe_sound(file: &str) -> SoundFile {
    let path = PathBuf::from(expand_env(file));
    let Ok(meta) = std::fs::metadata(&path) else {
        return SoundFile::Missing;
    };
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if !meta.is_file() || !SOUND_EXTS.contains(&ext.as_str()) {
        return SoundFile::Skipped;
    }
    SoundFile::Found {
        path,
        ext,
        size: meta.len(),
    }
}

/// 디스크를 읽기만 한다 (소리 파일 크기·존재).
pub fn export_check(config: &Config, sel: &Selection) -> ExportReport {
    let mut r = ExportReport::default();
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut total = 0u64;
    for page in selected_pages(config, sel) {
        for_each_action(&page, &mut |a| match a {
            Action::Text { .. } => r.text_buttons += 1,
            Action::Sound { file, .. } => match probe_sound(file) {
                SoundFile::Missing if !r.missing_sounds.contains(file) => {
                    r.missing_sounds.push(file.clone())
                }
                SoundFile::Found { size, .. } if size > MAX_SOUND => {
                    if !r.too_big_sounds.contains(file) {
                        r.too_big_sounds.push(file.clone())
                    }
                }
                // 합계는 같은 파일을 한 번만 센다 (내용이 같은 다른 파일은 세지 못해 넉넉히 센다)
                SoundFile::Found { path, size, .. } if !seen.contains(&path) => {
                    total += size;
                    seen.push(path);
                }
                _ => {}
            },
            _ => {}
        });
    }
    r.sounds_total_too_big = total > MAX_SOUNDS_TOTAL;
    r
}

/// 고른 페이지(All이면 설정 전체)를 `path`에 `.npdeck`로 쓴다.
/// 효과음은 `sounds/<해시>.<확장자>`로 담고 경로는 [`portable_path`]로 바꾼다.
pub fn export(
    config: &Config,
    sel: &Selection,
    app_version: &str,
    dirs: &UserDirs,
    path: &Path,
) -> Result<(), ShareError> {
    export_with(config, sel, app_version, dirs, path, false)
}

/// [`export`]와 같되, 담을 수 없는 효과음(너무 크거나 합계를 넘음)은 오류로 멈추지 않고 소리만 뺀다.
/// 버튼은 그대로 두고 경로만 [`portable_path`]로 바꾼다. 자동 백업이 효과음 때문에 실패하지 않게 한다.
fn export_with(
    config: &Config,
    sel: &Selection,
    app_version: &str,
    dirs: &UserDirs,
    path: &Path,
    skip_unfit_sounds: bool,
) -> Result<(), ShareError> {
    let mut pages = selected_pages(config, sel);
    if pages.is_empty() {
        return Err(ShareError::Invalid(tr(
            "내보낼 페이지가 없어요.",
            "There are no pages to export.",
        )));
    }
    let mut sounds: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut total = 0u64;
    let mut error: Option<ShareError> = None;
    for page in &mut pages {
        for_each_action_mut(page, &mut |a| {
            if error.is_some() {
                return;
            }
            match a {
                Action::Launch { target, .. } | Action::Open { target } => {
                    *target = portable_path(target, dirs);
                }
                Action::Sound { file, .. } => match probe_sound(file) {
                    SoundFile::Found { path, ext, size } => {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        if size > MAX_SOUND {
                            if skip_unfit_sounds {
                                *file = portable_path(file, dirs);
                            } else {
                                error = Some(ShareError::SoundTooBig(name));
                            }
                            return;
                        }
                        let bytes = match std::fs::read(&path) {
                            Ok(b) => b,
                            Err(e) => {
                                if skip_unfit_sounds {
                                    *file = portable_path(file, dirs);
                                } else {
                                    error = Some(io_err(e));
                                }
                                return;
                            }
                        };
                        if bytes.len() as u64 > MAX_SOUND {
                            if skip_unfit_sounds {
                                *file = portable_path(file, dirs);
                            } else {
                                error = Some(ShareError::SoundTooBig(name));
                            }
                            return;
                        }
                        let entry = sound_entry_name(&bytes, &ext);
                        if !sounds.contains_key(&entry) {
                            if total + bytes.len() as u64 > MAX_SOUNDS_TOTAL {
                                if skip_unfit_sounds {
                                    *file = portable_path(file, dirs);
                                } else {
                                    error = Some(ShareError::SoundsTooBig);
                                }
                                return;
                            }
                            total += bytes.len() as u64;
                            sounds.insert(entry.clone(), bytes);
                        }
                        *file = entry;
                    }
                    SoundFile::Missing | SoundFile::Skipped => {
                        *file = portable_path(file, dirs);
                    }
                },
                _ => {}
            }
        });
        if let Some(e) = error {
            return Err(e);
        }
    }

    let backup = matches!(sel, Selection::All);
    let manifest = Manifest {
        format: FORMAT,
        kind: if backup { Kind::Backup } else { Kind::Pages },
        app: app_version.to_string(),
        pages,
        language: config.language.filter(|_| backup),
        layout: config.layout.clone().filter(|_| backup),
        system_keys: config.system_keys.clone().filter(|_| backup),
        mic_indicator: config.mic_indicator.filter(|_| backup),
    };
    write_bundle(path, &manifest, &sounds)
}

/// `sounds/<sha256 앞 16자 소문자 hex>.<확장자>`
fn sound_entry_name(bytes: &[u8], ext: &str) -> String {
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("sounds/{hex}.{ext}")
}

fn write_bundle(
    path: &Path,
    manifest: &Manifest,
    sounds: &BTreeMap<String, Vec<u8>>,
) -> Result<(), ShareError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let result = (|| -> Result<(), ShareError> {
        let file = std::fs::File::create(&tmp).map_err(io_err)?;
        let mut w = zip::ZipWriter::new(file);
        let opt = zip::write::SimpleFileOptions::default();
        w.start_file(MANIFEST_NAME, opt).map_err(io_err)?;
        let json = serde_json::to_vec_pretty(manifest).map_err(io_err)?;
        w.write_all(&json).map_err(io_err)?;
        for (name, bytes) in sounds {
            w.start_file(name.as_str(), opt).map_err(io_err)?;
            w.write_all(bytes).map_err(io_err)?;
        }
        w.finish().map_err(io_err)?.sync_all().map_err(io_err)?;
        std::fs::rename(&tmp, path).map_err(io_err)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// 항목 하나를 최대 `limit`바이트까지 읽는다. 넘으면 `TooBig`.
/// 선언된 크기를 믿지 않고 실제로 풀면서 센다.
fn read_limited(entry: &mut impl Read, limit: u64) -> Result<Vec<u8>, ShareError> {
    let mut buf = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| ShareError::Corrupt(e.to_string()))?;
    if buf.len() as u64 > limit {
        return Err(ShareError::TooBig);
    }
    Ok(buf)
}

/// 메모리로만 읽는다 (디스크에 풀지 않음). 알 수 없는 항목은 무시한다.
pub fn read(path: &Path) -> Result<Bundle, ShareError> {
    let file = std::fs::File::open(path).map_err(io_err)?;
    if file.metadata().map_err(io_err)?.len() > MAX_FILE {
        return Err(ShareError::TooBig);
    }
    let mut archive = zip::ZipArchive::new(file).map_err(|_| ShareError::NotDeckFile)?;
    if archive.len() > MAX_ENTRIES {
        return Err(ShareError::TooBig);
    }
    let mut manifest_bytes = None;
    let mut sounds = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ShareError::Corrupt(e.to_string()))?;
        let name = entry.name().to_string();
        if name == MANIFEST_NAME {
            manifest_bytes = Some(read_limited(&mut entry, MAX_MANIFEST)?);
        } else if is_sound_entry(&name) {
            let bytes = read_limited(&mut entry, MAX_SOUND)?;
            // 이름이 내용의 해시와 다르면 위조로 보고 무시한다
            let ext = name.rsplit('.').next().unwrap_or("");
            if sound_entry_name(&bytes, ext) != name {
                continue;
            }
            total += bytes.len() as u64;
            if total > MAX_SOUNDS_TOTAL {
                return Err(ShareError::TooBig);
            }
            sounds.insert(name, bytes);
        }
    }
    let manifest_bytes = manifest_bytes.ok_or(ShareError::NotDeckFile)?;
    let value: serde_json::Value =
        serde_json::from_slice(&manifest_bytes).map_err(|e| ShareError::Corrupt(e.to_string()))?;
    if let Some(format) = value.get("format").and_then(|f| f.as_u64()) {
        if format > FORMAT as u64 {
            return Err(ShareError::TooNew(format.min(u32::MAX as u64) as u32));
        }
    }
    let manifest: Manifest =
        serde_json::from_value(value).map_err(|e| ShareError::Corrupt(e.to_string()))?;
    let config = Config {
        version: crate::config::VERSION,
        language: manifest.language,
        layout: manifest.layout.clone(),
        system_keys: manifest.system_keys.clone(),
        mic_indicator: manifest.mic_indicator,
        pages: manifest.pages.clone(),
    };
    config.validate().map_err(ShareError::Invalid)?;
    // 버전 문자열은 숫자와 점 16자까지만 믿는다 (화면에 그대로 보이므로)
    let mut manifest = manifest;
    if !(1..=16).contains(&manifest.app.len())
        || !manifest
            .app
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.')
    {
        manifest.app = String::new();
    }
    Ok(Bundle { manifest, sounds })
}

/// 묶음에 `shell:AppsFolder\` 실행 대상이 있는가 (없으면 설치된 앱 목록을 읽을 필요가 없다).
pub fn needs_app_list(bundle: &Bundle) -> bool {
    let mut found = false;
    for page in &bundle.manifest.pages {
        for_each_action(page, &mut |a| {
            if let Action::Launch { target, .. } = a {
                found |= target
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with(r"shell:appsfolder\");
            }
        });
    }
    found
}

/// 이 PC에 대상이 있는지 (앱이 실제 확인으로 채운다, 시험은 가짜).
pub trait Probe {
    fn file_exists(&self, expanded: &str) -> bool;
    fn dir_exists(&self, expanded: &str) -> bool;
    /// `shell:AppsFolder\…` 대상 전체를 받아 설치돼 있는지 본다.
    fn app_installed(&self, apps_folder_target: &str) -> bool;
}

/// 버튼 하나의 미리 보기 표시.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct ButtonNote {
    /// 키캡 글자
    pub key: String,
    /// 실행·입력을 일으키는 동작이 있음
    pub caution: bool,
    /// 이 PC에 대상이 없음
    pub missing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct PreviewPage {
    pub page: Page,
    pub notes: Vec<ButtonNote>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct Preview {
    /// 열 때마다 새로 매긴다 (오래된 미리 보기로 설치하지 않게).
    #[ts(type = "number")]
    pub id: u64,
    pub kind: Kind,
    pub app: String,
    pub pages: Vec<PreviewPage>,
    /// 백업일 때만
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub layout: Option<Vec<KeyPlace>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub system_keys: Option<SystemKeys>,
    pub any_caution: bool,
}

/// `ms-settings:`, `mailto:`, `https://` 같은 주소인가 (앞 글자 2자 이상이면 `C:\` 같은 드라이브가 아님).
/// 셸이 알아서 여는 대상이라 이 PC에 있는지 판정하지 않는다.
fn has_uri_scheme(t: &str) -> bool {
    let Some((scheme, _)) = t.split_once(':') else {
        return false;
    };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

/// 동작 하나의 (주의, 없음). 묶음은 단계 중 하나라도 해당하면 해당한다.
fn action_flags(a: &Action, bundle: &Bundle, probe: &impl Probe) -> (bool, bool) {
    match a {
        Action::Launch { target, .. } => {
            let missing = if target.to_lowercase().starts_with("shell:appsfolder") {
                !probe.app_installed(target)
            } else {
                let p = expand_env(target);
                // 주소나 경로 없는 이름(`notepad.exe`)은 셸이 찾으므로 판정하지 않는다.
                !(has_uri_scheme(target) || !p.contains(['\\', '/']) || probe.file_exists(&p))
            };
            (true, missing)
        }
        Action::Open { target } => {
            let missing = if has_uri_scheme(target) {
                false
            } else {
                let p = expand_env(target);
                !(probe.file_exists(&p) || probe.dir_exists(&p))
            };
            (true, missing)
        }
        Action::Hotkey { .. } | Action::Text { .. } => (true, false),
        Action::Sound { file, .. } => {
            let missing = if file.starts_with("sounds/") {
                !bundle.sounds.contains_key(file)
            } else {
                !probe.file_exists(&expand_env(file))
            };
            (false, missing)
        }
        Action::Sequence { steps } => steps.iter().fold((false, false), |acc, s| {
            let f = action_flags(s, bundle, probe);
            (acc.0 || f.0, acc.1 || f.1)
        }),
        Action::Media { .. } | Action::MicMute | Action::StopSounds | Action::Delay { .. } => {
            (false, false)
        }
    }
}

/// 묶음의 페이지마다 버튼의 주의·없음을 판정한다 (디스크는 `probe`로만 본다).
pub fn preview(bundle: &Bundle, id: u64, probe: &impl Probe) -> Preview {
    let m = &bundle.manifest;
    let mut any_caution = false;
    let pages = m
        .pages
        .iter()
        .map(|page| {
            let notes = page
                .buttons
                .iter()
                .map(|(key, button)| {
                    let (caution, missing) = action_flags(&button.action, bundle, probe);
                    any_caution |= caution;
                    ButtonNote {
                        key: key.key().label().to_string(),
                        caution,
                        missing,
                    }
                })
                .collect();
            PreviewPage {
                page: page.clone(),
                notes,
            }
        })
        .collect();
    let backup = m.kind == Kind::Backup;
    Preview {
        id,
        kind: m.kind,
        app: m.app.clone(),
        pages,
        layout: m.layout.clone().filter(|_| backup),
        system_keys: m.system_keys.clone().filter(|_| backup),
        any_caution,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub enum InstallMode {
    Add,
    Replace,
}

#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct InstallChoice {
    #[ts(type = "number")]
    pub preview_id: u64,
    /// 고른 페이지 번호 (0부터)
    pub pages: Vec<usize>,
    pub mode: InstallMode,
}

/// 겹치지 않는 페이지 이름: `이름`, `이름 (2)`, `이름 (3)` …
fn unique_name(name: &str, used: &BTreeSet<String>) -> String {
    if !used.contains(name) {
        return name.to_string();
    }
    (2u32..)
        .map(|n| format!("{name} ({n})"))
        .find(|c| !used.contains(c))
        .expect("끝없는 번호")
}

/// 고른 페이지를 설정에 넣은 새 `Config`를 돌려준다 (저장은 호출자가 한다).
/// 묶음의 소리는 `data_dir/sounds`에 쓴다 (이미 있으면 건너뜀).
pub fn install(
    current: &Config,
    bundle: &Bundle,
    choice: &InstallChoice,
    data_dir: &Path,
) -> Result<Config, ShareError> {
    let m = &bundle.manifest;
    if choice.mode == InstallMode::Replace && m.kind != Kind::Backup {
        return Err(ShareError::Invalid(tr(
            "이 파일은 설정 전체를 바꿀 수 없어요.",
            "This file cannot replace all settings.",
        )));
    }
    if choice.pages.is_empty() {
        return Err(ShareError::Invalid(tr(
            "가져올 페이지를 골라 주세요.",
            "Please choose pages to import.",
        )));
    }
    if choice.pages.iter().any(|&i| i >= m.pages.len()) {
        return Err(ShareError::Invalid(tr(
            "없는 페이지를 골랐어요.",
            "That page does not exist.",
        )));
    }

    // 소리 경로를 가져온 폴더 기준으로 바꾼다 (묶음에 없는 소리는 그대로 둔다).
    let mut sound_names: BTreeSet<String> = BTreeSet::new();
    let mut picked: Vec<Page> = choice.pages.iter().map(|&i| m.pages[i].clone()).collect();
    for page in &mut picked {
        for_each_action_mut(page, &mut |a| {
            if let Action::Sound { file, .. } = a {
                if bundle.sounds.contains_key(file.as_str()) {
                    let name = file.trim_start_matches("sounds/").to_string();
                    *file = format!(r"%APPDATA%\NumpadDeck\sounds\{name}");
                    sound_names.insert(name);
                }
            }
        });
    }

    let mut config = match choice.mode {
        InstallMode::Add => {
            let mut used: BTreeSet<String> = current.pages.iter().map(|p| p.name.clone()).collect();
            for page in &mut picked {
                page.name = unique_name(&page.name, &used);
                used.insert(page.name.clone());
            }
            let mut c = current.clone();
            c.pages.extend(picked);
            c
        }
        InstallMode::Replace => Config {
            version: crate::config::VERSION,
            language: m.language,
            layout: m.layout.clone(),
            system_keys: m.system_keys.clone(),
            mic_indicator: m.mic_indicator,
            pages: picked,
        },
    };
    config.version = crate::config::VERSION;
    config.validate().map_err(ShareError::Invalid)?;

    // 문제가 없을 때만 디스크에 쓴다.
    if !sound_names.is_empty() {
        let dir = data_dir.join("sounds");
        std::fs::create_dir_all(&dir).map_err(io_err)?;
        for name in &sound_names {
            let dest = dir.join(name);
            if dest.exists() {
                continue;
            }
            let bytes = &bundle.sounds[&format!("sounds/{name}")];
            // 임시 파일에 쓴 뒤 옮겨, 중간에 끊겨도 잘린 파일이 남지 않게 한다
            let tmp = dir.join(format!("{name}.tmp"));
            let r = std::fs::write(&tmp, bytes).and_then(|_| std::fs::rename(&tmp, &dest));
            if let Err(e) = r {
                let _ = std::fs::remove_file(&tmp);
                return Err(io_err(e));
            }
        }
    }
    Ok(config)
}

/// 남겨 둘 자동 백업 개수.
const KEEP_BACKUPS: usize = 10;

/// 1970-01-01부터 지난 일수 → (년, 월, 일). 양력, 윤년 포함 (Howard Hinnant의 civil_from_days).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// UTC 초 -> (년, 월, 일, 시, 분, 초).
fn utc_parts(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let (y, mo, d) = civil_from_days(secs.div_euclid(86_400));
    let t = secs.rem_euclid(86_400) as u32;
    (y, mo, d, t / 3600, t % 3600 / 60, t % 60)
}

/// UTC 초를 현지 시각 부품으로 바꾼다. 변환이 안 되면 UTC 그대로.
fn local_parts(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    use windows::Win32::Foundation::SYSTEMTIME;
    use windows::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
    let utc = utc_parts(secs);
    let st = SYSTEMTIME {
        wYear: utc.0 as u16,
        wMonth: utc.1 as u16,
        wDay: utc.2 as u16,
        wHour: utc.3 as u16,
        wMinute: utc.4 as u16,
        wSecond: utc.5 as u16,
        ..Default::default()
    };
    let mut local = SYSTEMTIME::default();
    // SAFETY: 두 SYSTEMTIME은 이 함수 안의 값이고, 시간대는 현재 설정(None)을 쓴다.
    if unsafe { SystemTimeToTzSpecificLocalTime(None, &st, &mut local) }.is_err() {
        return utc;
    }
    (
        local.wYear as i64,
        local.wMonth as u32,
        local.wDay as u32,
        local.wHour as u32,
        local.wMinute as u32,
        local.wSecond as u32,
    )
}

fn format_stamp(p: (i64, u32, u32, u32, u32, u32)) -> String {
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        p.0, p.1, p.2, p.3, p.4, p.5
    )
}

/// `YYYYMMDD-HHMMSS` (현지 시각). 날짜 계산은 직접 하고(chrono 없음) 현지 변환만 Windows에 맡긴다.
fn stamp(now: SystemTime) -> String {
    let secs = match now.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    format_stamp(local_parts(secs))
}

/// 자동 백업 확장자: 설정이 정상이면 `.npdeck`, 깨졌을 때만 원본 `.json`.
const BACKUP_EXTS: [&str; 2] = ["json", EXTENSION];

/// `config-<시각>[-<번호>].(json|npdeck)` → (시각, 번호) 정렬 키. 형식이 다르면 None.
fn backup_sort_key(file_name: &str) -> Option<(String, u32)> {
    let rest = file_name.strip_prefix("config-")?;
    let stem = BACKUP_EXTS
        .iter()
        .find_map(|ext| rest.strip_suffix(ext).and_then(|r| r.strip_suffix('.')))?;
    let mut it = stem.splitn(3, '-');
    let (date, time) = (it.next()?, it.next()?);
    let n = it.next().map_or(Some(1), |n| n.parse().ok())?;
    Some((format!("{date}-{time}"), n))
}

/// 덮어쓰기 직전의 설정을 같은 폴더의 `backups/`에 남기고 최신 10개만 둔다. 만든 경로를 돌려준다.
///
/// - 정상(`broken == false`): 지금 앱이 쓰는 설정(`config`)을 `.npdeck` 백업 파일로 내보낸다.
///   "백업 파일 가져오기"로 되돌릴 수 있고 효과음도 담긴다. 담을 수 없는 효과음은 빼고 만든다.
/// - 깨짐(`broken == true`): 디스크의 설정 파일이 올바른 설정이 아니므로 원본을 `.json`으로 복사한다.
///
/// `.json`과 `.npdeck`은 한 묶음으로 세어 오래된 것부터 지운다.
pub fn backup_config(
    config_path: &Path,
    config: &Config,
    broken: bool,
    app_version: &str,
    dirs: &UserDirs,
    now: SystemTime,
) -> Result<PathBuf, ShareError> {
    let dir = config_path
        .parent()
        .ok_or_else(|| ShareError::Io("no parent".into()))?
        .join("backups");
    std::fs::create_dir_all(&dir).map_err(io_err)?;
    let ext = if broken { "json" } else { EXTENSION };
    let base = format!("config-{}", stamp(now));
    // 같은 초에 또 만들면 번호를 붙여 덮어쓰지 않는다 (두 확장자 모두 확인해 정렬 순서가 겹치지 않게).
    let taken = |name: &str| {
        BACKUP_EXTS
            .iter()
            .any(|e| dir.join(format!("{name}.{e}")).exists())
    };
    let mut name = base.clone();
    let mut n = 2;
    while taken(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    let dest = dir.join(format!("{name}.{ext}"));
    if broken {
        std::fs::copy(config_path, &dest).map_err(io_err)?;
    } else {
        // 임시 파일에 쓰고 이름을 바꾸므로 덜 쓴 파일이 `config-*.npdeck`로 남지 않는다.
        export_with(config, &Selection::All, app_version, dirs, &dest, true)?;
    }

    let mut all: Vec<((String, u32), PathBuf)> = std::fs::read_dir(&dir)
        .map_err(io_err)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let key = backup_sort_key(&e.file_name().to_string_lossy())?;
            Some((key, e.path()))
        })
        .collect();
    all.sort();
    // 방금 만든 것은 시계가 어긋나 있어도 지우지 않는다.
    all.retain(|(_, p)| *p != dest);
    let extra = all.len().saturating_sub(KEEP_BACKUPS - 1);
    for (_, old) in all.into_iter().take(extra) {
        let _ = std::fs::remove_file(old);
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs() -> UserDirs {
        UserDirs {
            local_app_data: r"C:\Users\me\AppData\Local".into(),
            app_data: r"C:\Users\me\AppData\Roaming".into(),
            user_profile: r"C:\Users\me".into(),
        }
    }

    #[test]
    fn portable_local_app_data() {
        assert_eq!(
            portable_path(
                r"C:\Users\me\AppData\Local\Microsoft\Teams\current\Teams.exe",
                &dirs()
            ),
            r"%LOCALAPPDATA%\Microsoft\Teams\current\Teams.exe"
        );
    }

    #[test]
    fn portable_ignores_case_and_slash() {
        assert_eq!(
            portable_path("c:/users/ME/AppData/Roaming/x.wav", &dirs()),
            r"%APPDATA%\x.wav"
        );
    }

    #[test]
    fn portable_user_profile() {
        assert_eq!(
            portable_path(r"C:\Users\me\Music\a.mp3", &dirs()),
            r"%USERPROFILE%\Music\a.mp3"
        );
    }

    #[test]
    fn portable_respects_boundary() {
        let p = r"C:\Users\meow\a.exe";
        assert_eq!(portable_path(p, &dirs()), p);
    }

    #[test]
    fn portable_leaves_others() {
        for p in [
            "https://example.com/a",
            r"shell:AppsFolder\X",
            r"%WINDIR%\notepad.exe",
        ] {
            assert_eq!(portable_path(p, &dirs()), p);
        }
    }

    #[test]
    fn portable_exact_folder() {
        assert_eq!(portable_path(r"C:\Users\me", &dirs()), "%USERPROFILE%");
    }

    fn sample_page() -> Page {
        serde_json::from_str(
            r#"{"name":"p","buttons":{
              "7":{"label":"a","action":{"type":"launch","target":"x.exe"}},
              "8":{"label":"b","action":{"type":"sequence","steps":[
                {"type":"sound","file":"a.wav"},
                {"type":"delay","ms":10},
                {"type":"sequence","steps":[{"type":"open","target":"http://a"}]}
              ]}}
            }}"#,
        )
        .unwrap()
    }

    #[test]
    fn for_each_action_visits_sequence_steps() {
        let page = sample_page();
        let mut n = 0;
        let mut opens = 0;
        for_each_action(&page, &mut |a| {
            n += 1;
            if matches!(a, Action::Open { .. }) {
                opens += 1;
            }
        });
        // launch + sequence + (sound, delay, sequence, open)
        assert_eq!(n, 6);
        assert_eq!(opens, 1);
    }

    #[test]
    fn for_each_action_mut_edits_nested() {
        let mut page = sample_page();
        for_each_action_mut(&mut page, &mut |a| {
            if let Action::Sound { file, .. } = a {
                *file = "b.wav".into();
            }
        });
        let mut found = vec![];
        for_each_action(&page, &mut |a| {
            if let Action::Sound { file, .. } = a {
                found.push(file.clone());
            }
        });
        assert_eq!(found, ["b.wav"]);
    }

    fn manifest() -> Manifest {
        Manifest {
            format: FORMAT,
            kind: Kind::Pages,
            app: "0.6.0".into(),
            pages: vec![],
            language: None,
            layout: None,
            system_keys: None,
            mic_indicator: None,
        }
    }

    #[test]
    fn manifest_serializes_pages_without_options() {
        let v = serde_json::to_value(manifest()).unwrap();
        assert_eq!(v["kind"], "pages");
        let obj = v.as_object().unwrap();
        assert!(!obj.contains_key("language"));
        assert!(!obj.contains_key("systemKeys"));
        assert!(!obj.contains_key("micIndicator"));
    }

    #[test]
    fn manifest_backup_uses_camel_case() {
        let mut m = manifest();
        m.kind = Kind::Backup;
        m.mic_indicator = Some(false);
        m.system_keys =
            Some(serde_json::from_str(r#"{"prev":"/","next":"*","osd":"-","first":"."}"#).unwrap());
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["kind"], "backup");
        assert_eq!(v["micIndicator"], false);
        assert!(v.get("systemKeys").is_some());
        let back: Manifest = serde_json::from_value(v).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn manifest_rejects_unknown_fields() {
        let r: Result<Manifest, _> =
            serde_json::from_str(r#"{"format":1,"kind":"pages","app":"x","pages":[],"extra":1}"#);
        assert!(r.is_err());
    }

    // ---- 내보내기·읽기 ----

    use std::path::PathBuf;

    fn test_dirs(root: &Path) -> UserDirs {
        UserDirs {
            local_app_data: root.join("Local").to_string_lossy().into_owned(),
            app_data: root.join("Roaming").to_string_lossy().into_owned(),
            user_profile: root.to_string_lossy().into_owned(),
        }
    }

    fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> String {
        let p = dir.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p.to_string_lossy().into_owned()
    }

    fn esc(s: &str) -> String {
        s.replace('\\', "\\\\")
    }

    fn page_json(name: &str, sound: &str) -> String {
        let sound = esc(sound);
        format!(
            r#"{{"name":"{name}","buttons":{{"7":{{"label":"s","action":{{"type":"sound","file":"{sound}"}}}}}}}}"#
        )
    }

    fn config_of(pages: &[String], extra: &str) -> Config {
        let json = format!(r#"{{"version":1,{extra}"pages":[{}]}}"#, pages.join(","));
        serde_json::from_str(&json).unwrap()
    }

    fn first_sound(page: &Page) -> String {
        let mut out = vec![];
        for_each_action(page, &mut |a| {
            if let Action::Sound { file, .. } = a {
                out.push(file.clone());
            }
        });
        out.remove(0)
    }

    /// 지정한 항목만 담은 zip을 만든다.
    fn make_zip(path: &Path, entries: &[(&str, Vec<u8>)]) {
        let f = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        let opt = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            w.start_file(*name, opt).unwrap();
            w.write_all(bytes).unwrap();
        }
        w.finish().unwrap();
    }

    fn manifest_bytes(format: u32, pages_json: &str) -> Vec<u8> {
        format!(r#"{{"format":{format},"kind":"pages","app":"t","pages":[{pages_json}]}}"#)
            .into_bytes()
    }

    fn export_to(config: &Config, sel: &Selection, root: &Path) -> Result<PathBuf, ShareError> {
        let out = root.join("out.npdeck");
        export(config, sel, "0.6.0", &test_dirs(root), &out).map(|_| out)
    }

    #[test]
    fn round_trip_pages_with_sound() {
        let dir = tempfile::tempdir().unwrap();
        let wav = write_file(dir.path(), "snd/a.wav", b"RIFFdata");
        let cfg = config_of(&[page_json("A", &wav), page_json("B", &wav)], "");
        let out = export_to(&cfg, &Selection::Pages(vec![1]), dir.path()).unwrap();
        let b = read(&out).unwrap();
        assert_eq!(b.manifest.kind, Kind::Pages);
        assert_eq!(b.manifest.pages.len(), 1);
        assert_eq!(b.manifest.pages[0].name, "B");
        assert_eq!(b.sounds.len(), 1);
        let file = first_sound(&b.manifest.pages[0]);
        assert!(
            file.starts_with("sounds/") && file.ends_with(".wav"),
            "{file}"
        );
        assert_eq!(file.len(), "sounds/".len() + 16 + ".wav".len());
        assert_eq!(b.sounds[&file], b"RIFFdata");
        assert!(!dir.path().join("out.npdeck.tmp").exists());
    }

    #[test]
    fn round_trip_backup_keeps_settings() {
        let dir = tempfile::tempdir().unwrap();
        let places: Vec<String> = crate::config::layout_key_names()
            .enumerate()
            .map(|(i, k)| {
                format!(
                    r#"{{"key":"{k}","x":{},"y":{},"w":1,"h":1}}"#,
                    i % 10,
                    i / 10
                )
            })
            .collect();
        let extra = format!(
            r#""language":"en","micIndicator":false,
            "systemKeys":{{"prev":"*","next":"/","osd":"-","first":"."}},
            "layout":[{}],"#,
            places.join(",")
        );
        let cfg = config_of(&[page_json("A", "x.wav")], &extra);
        let out = export_to(&cfg, &Selection::All, dir.path()).unwrap();
        let b = read(&out).unwrap();
        assert_eq!(b.manifest.kind, Kind::Backup);
        assert_eq!(b.manifest.language, cfg.language);
        assert_eq!(b.manifest.layout, cfg.layout);
        assert_eq!(b.manifest.system_keys, cfg.system_keys);
        assert_eq!(b.manifest.mic_indicator, Some(false));
    }

    #[test]
    fn same_sound_file_twice_is_stored_once() {
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "one/ding.wav", b"same");
        let b = write_file(dir.path(), "two/ding.wav", b"same");
        let c = write_file(dir.path(), "three/ding.wav", b"different");
        let cfg = config_of(&[page_json("A", &a), page_json("B", &b)], "");
        let out = export_to(&cfg, &Selection::All, dir.path()).unwrap();
        assert_eq!(read(&out).unwrap().sounds.len(), 1);
        let cfg = config_of(&[page_json("A", &a), page_json("B", &c)], "");
        let out = export_to(&cfg, &Selection::All, dir.path()).unwrap();
        assert_eq!(read(&out).unwrap().sounds.len(), 2);
    }

    #[test]
    fn sound_in_sequence_is_bundled() {
        let dir = tempfile::tempdir().unwrap();
        let wav = esc(&write_file(dir.path(), "a.wav", b"seq"));
        let page = format!(
            r#"{{"name":"S","buttons":{{"7":{{"label":"s","action":{{"type":"sequence","steps":[
                {{"type":"delay","ms":10}},{{"type":"sound","file":"{wav}"}}]}}}}}}}}"#
        );
        let cfg = config_of(&[page], "");
        let out = export_to(&cfg, &Selection::All, dir.path()).unwrap();
        let b = read(&out).unwrap();
        assert_eq!(b.sounds.len(), 1);
        assert!(first_sound(&b.manifest.pages[0]).starts_with("sounds/"));
    }

    #[test]
    fn export_check_counts_text_and_missing() {
        let dir = tempfile::tempdir().unwrap();
        let big = write_file(dir.path(), "big.wav", &vec![0u8; MAX_SOUND as usize + 1]);
        let ok = write_file(dir.path(), "ok.wav", b"ok");
        let missing = dir.path().join("none.wav").to_string_lossy().into_owned();
        let page = format!(
            r#"{{"name":"T","buttons":{{
              "7":{{"label":"t","action":{{"type":"text","text":"hi"}}}},
              "8":{{"label":"q","action":{{"type":"sequence","steps":[{{"type":"text","text":"x"}}]}}}},
              "9":{{"label":"m","action":{{"type":"sound","file":"{}"}}}},
              "4":{{"label":"b","action":{{"type":"sound","file":"{}"}}}},
              "5":{{"label":"o","action":{{"type":"sound","file":"{}"}}}}
            }}}}"#,
            esc(&missing),
            esc(&big),
            esc(&ok)
        );
        let cfg = config_of(&[page], "");
        let r = export_check(&cfg, &Selection::All);
        assert_eq!(r.text_buttons, 2);
        assert_eq!(r.missing_sounds, [missing]);
        assert_eq!(r.too_big_sounds, [big]);
        assert!(!r.sounds_total_too_big);
    }

    #[test]
    fn export_total_over_limit_is_flagged_and_rejected() {
        let dir = tempfile::tempdir().unwrap();
        // 서로 다른 4.5 MiB 소리 5개 = 22.5 MiB
        let pages: Vec<String> = (0..5u8)
            .map(|i| {
                let f = write_file(
                    dir.path(),
                    &format!("s{i}.wav"),
                    &vec![i; MAX_SOUND as usize - 512 * 1024],
                );
                page_json(&format!("P{i}"), &f)
            })
            .collect();
        let cfg = config_of(&pages, "");
        assert!(export_check(&cfg, &Selection::All).sounds_total_too_big);
        assert!(!export_check(&cfg, &Selection::Pages(vec![0, 1])).sounds_total_too_big);
        let r = export_to(&cfg, &Selection::All, dir.path());
        assert!(matches!(r, Err(ShareError::SoundsTooBig)), "{r:?}");
        assert!(!r.unwrap_err().to_string().contains("열 수"));
    }

    #[test]
    fn export_rejects_too_big_sound() {
        let dir = tempfile::tempdir().unwrap();
        let big = write_file(dir.path(), "big.wav", &vec![0u8; MAX_SOUND as usize + 1]);
        let cfg = config_of(&[page_json("A", &big)], "");
        let r = export_to(&cfg, &Selection::All, dir.path());
        assert!(matches!(r, Err(ShareError::SoundTooBig(_))), "{r:?}");
    }

    #[test]
    fn read_ignores_unknown_and_traversal_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        let page = r#"{"name":"A","buttons":{}}"#;
        make_zip(
            &path,
            &[
                ("manifest.json", manifest_bytes(1, page)),
                ("../evil.txt", b"x".to_vec()),
                ("sounds/../../x.wav", b"x".to_vec()),
                ("other/a.wav", b"x".to_vec()),
                ("sounds/ABCDEF0123456789.wav", b"x".to_vec()),
                ("sounds/0123456789abcde.wav", b"x".to_vec()),
                ("sounds/0123456789abcdef.exe", b"x".to_vec()),
            ],
        );
        let b = read(&path).unwrap();
        assert!(b.sounds.is_empty());
        let good = sound_entry_name(b"x", "wav");
        make_zip(
            &path,
            &[
                ("manifest.json", manifest_bytes(1, page)),
                (good.as_str(), b"x".to_vec()),
            ],
        );
        assert_eq!(read(&path).unwrap().sounds.len(), 1);
    }

    #[test]
    fn read_ignores_sound_whose_name_does_not_match_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        let page = r#"{"name":"A","buttons":{}}"#;
        make_zip(
            &path,
            &[
                ("manifest.json", manifest_bytes(1, page)),
                ("sounds/0123456789abcdef.wav", b"x".to_vec()),
            ],
        );
        assert!(read(&path).unwrap().sounds.is_empty());
    }

    #[test]
    fn needs_app_list_only_for_apps_folder_targets() {
        let plain = bundle_of(Kind::Pages, &[mic_page("A")], &[]);
        assert!(!needs_app_list(&plain));
        let exe = page_with(
            "E",
            &[("7", r#"{"type":"launch","target":"C:\\a\\b.exe"}"#)],
        );
        assert!(!needs_app_list(&bundle_of(Kind::Pages, &[exe], &[])));
        let store = page_with(
            "S",
            &[(
                "7",
                r#"{"type":"sequence","steps":[{"type":"launch","target":"SHELL:AppsFolder\\X!App"}]}"#,
            )],
        );
        assert!(needs_app_list(&bundle_of(Kind::Pages, &[store], &[])));
    }

    #[test]
    fn read_rejects_too_many_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        let page = r#"{"name":"A","buttons":{}}"#;
        let names: Vec<String> = (0..MAX_ENTRIES).map(|i| format!("x/{i}.txt")).collect();
        let mut all = vec![("manifest.json", manifest_bytes(1, page))];
        for n in &names {
            all.push((n.as_str(), vec![]));
        }
        make_zip(&path, &all); // manifest 포함 MAX_ENTRIES + 1개
        assert!(matches!(read(&path), Err(ShareError::TooBig)));
    }

    #[test]
    fn read_keeps_only_plain_version_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        let with_app = |app: &str| {
            format!(r#"{{"format":1,"kind":"pages","app":"{app}","pages":[{{"name":"A","buttons":{{}}}}]}}"#)
                .into_bytes()
        };
        for (app, want) in [
            ("0.6.0", "0.6.0"),
            ("", ""),
            ("1.2.3 <b>", ""),
            ("12345678901234567", ""),
            ("v1", ""),
        ] {
            make_zip(&path, &[("manifest.json", with_app(app))]);
            assert_eq!(read(&path).unwrap().manifest.app, want, "{app}");
        }
    }

    #[test]
    fn read_rejects_bomb() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        let page = r#"{"name":"A","buttons":{}}"#;
        // 압축하면 작지만 풀면 크다
        let big_sound = vec![0u8; MAX_SOUND as usize + 1];
        make_zip(
            &path,
            &[
                ("manifest.json", manifest_bytes(1, page)),
                ("sounds/0123456789abcdef.wav", big_sound),
            ],
        );
        assert!(matches!(read(&path), Err(ShareError::TooBig)));
        let mut m = manifest_bytes(1, page);
        m.extend(vec![b' '; MAX_MANIFEST as usize]);
        make_zip(&path, &[("manifest.json", m)]);
        assert!(matches!(read(&path), Err(ShareError::TooBig)));
        // 소리 합계 초과 (5 MiB x 5 = 25 MiB)
        let part = vec![0u8; MAX_SOUND as usize];
        // 내용을 조금씩 달리해 이름(해시)이 맞는 소리 5개
        let parts: Vec<Vec<u8>> = (0..5u8)
            .map(|i| {
                let mut p = part.clone();
                p[0] = i;
                p
            })
            .collect();
        let names: Vec<String> = parts.iter().map(|p| sound_entry_name(p, "wav")).collect();
        let mut all = vec![("manifest.json", manifest_bytes(1, page))];
        for (n, p) in names.iter().zip(&parts) {
            all.push((n.as_str(), p.clone()));
        }
        make_zip(&path, &all);
        assert!(matches!(read(&path), Err(ShareError::TooBig)));
    }

    #[test]
    fn read_rejects_big_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MAX_FILE + 1).unwrap();
        assert!(matches!(read(&path), Err(ShareError::TooBig)));
    }

    #[test]
    fn read_rejects_newer_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        make_zip(
            &path,
            &[("manifest.json", manifest_bytes(2, r#"{"name":"A"}"#))],
        );
        assert!(matches!(read(&path), Err(ShareError::TooNew(2))));
    }

    #[test]
    fn read_rejects_non_zip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        std::fs::write(&path, b"hello, not a zip").unwrap();
        assert!(matches!(read(&path), Err(ShareError::NotDeckFile)));
    }

    #[test]
    fn read_rejects_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        make_zip(&path, &[("readme.txt", b"x".to_vec())]);
        assert!(matches!(read(&path), Err(ShareError::NotDeckFile)));
    }

    #[test]
    fn read_rejects_invalid_page() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.npdeck");
        let page = r#"{"name":"A","buttons":{"7":{"label":"d","action":{"type":"delay","ms":5}}}}"#;
        make_zip(&path, &[("manifest.json", manifest_bytes(1, page))]);
        assert!(matches!(read(&path), Err(ShareError::Invalid(_))));
        make_zip(&path, &[("manifest.json", manifest_bytes(1, ""))]);
        assert!(matches!(read(&path), Err(ShareError::Invalid(_))));
    }

    // ---- 미리 보기·설치·백업 ----

    #[derive(Default)]
    struct FakeProbe {
        files: BTreeSet<String>,
        dirs: BTreeSet<String>,
        apps: BTreeSet<String>,
    }

    impl Probe for FakeProbe {
        fn file_exists(&self, p: &str) -> bool {
            self.files.contains(p)
        }
        fn dir_exists(&self, p: &str) -> bool {
            self.dirs.contains(p)
        }
        fn app_installed(&self, t: &str) -> bool {
            self.apps.contains(t)
        }
    }

    fn bundle_of(kind: Kind, pages_json: &[String], sounds: &[(&str, &[u8])]) -> Bundle {
        let json = format!(
            r#"{{"format":1,"kind":"pages","app":"t","pages":[{}]}}"#,
            pages_json.join(",")
        );
        let mut manifest: Manifest = serde_json::from_str(&json).unwrap();
        manifest.kind = kind;
        Bundle {
            manifest,
            sounds: sounds
                .iter()
                .map(|(n, b)| (n.to_string(), b.to_vec()))
                .collect(),
        }
    }

    fn page_with(name: &str, actions: &[(&str, &str)]) -> String {
        let btns: Vec<String> = actions
            .iter()
            .map(|(k, a)| format!(r#""{k}":{{"label":"b","action":{a}}}"#))
            .collect();
        format!(r#"{{"name":"{name}","buttons":{{{}}}}}"#, btns.join(","))
    }

    fn mic_page(name: &str) -> String {
        page_with(name, &[("7", r#"{"type":"micMute"}"#)])
    }

    fn notes_of(b: &Bundle, probe: &FakeProbe) -> Vec<ButtonNote> {
        preview(b, 1, probe).pages.remove(0).notes
    }

    #[test]
    fn preview_caution_by_action_kind() {
        let page = page_with(
            "p",
            &[
                ("7", r#"{"type":"launch","target":"a.exe"}"#),
                ("8", r#"{"type":"open","target":"https://x.y"}"#),
                ("9", r#"{"type":"hotkey","keys":["Ctrl","C"]}"#),
                ("4", r#"{"type":"text","text":"hi"}"#),
                ("5", r#"{"type":"media","command":"playPause"}"#),
                ("6", r#"{"type":"micMute"}"#),
                ("1", r#"{"type":"stopSounds"}"#),
                ("2", r#"{"type":"sound","file":"a.wav"}"#),
                (
                    "3",
                    r#"{"type":"sequence","steps":[{"type":"delay","ms":5},{"type":"text","text":"x"}]}"#,
                ),
                (
                    "0",
                    r#"{"type":"sequence","steps":[{"type":"delay","ms":5},{"type":"micMute"}]}"#,
                ),
            ],
        );
        let b = bundle_of(Kind::Pages, &[page], &[]);
        let pv = preview(&b, 7, &FakeProbe::default());
        assert_eq!(pv.id, 7);
        let mut caution: Vec<(String, bool)> = pv.pages[0]
            .notes
            .iter()
            .map(|n| (n.key.clone(), n.caution))
            .collect();
        caution.sort();
        let mut want: Vec<(String, bool)> = [
            ("7", true),
            ("8", true),
            ("9", true),
            ("4", true),
            ("5", false),
            ("6", false),
            ("1", false),
            ("2", false),
            ("3", true),
            ("0", false),
        ]
        .iter()
        .map(|(k, c)| (k.to_string(), *c))
        .collect();
        want.sort();
        assert_eq!(caution, want);
        assert!(pv.any_caution);
        assert!(pv.layout.is_none() && pv.system_keys.is_none());
    }

    #[test]
    fn preview_no_caution_for_safe_pages() {
        let b = bundle_of(Kind::Pages, &[mic_page("p")], &[]);
        assert!(!preview(&b, 1, &FakeProbe::default()).any_caution);
    }

    #[test]
    fn preview_missing_launch_open_sound() {
        let mut probe = FakeProbe::default();
        std::env::set_var("NPDK_SHARE_T", r"C:\t");
        probe.files.insert(r"C:\t\ok.exe".into());
        probe.dirs.insert(r"C:\docs".into());
        probe.apps.insert(r"shell:AppsFolder\Store.App!App".into());
        probe.files.insert(r"C:\snd\ok.wav".into());
        let page = page_with(
            "p",
            &[
                (
                    "7",
                    r#"{"type":"launch","target":"%NPDK_SHARE_T%\\ok.exe"}"#,
                ),
                (
                    "8",
                    r#"{"type":"launch","target":"%NPDK_SHARE_T%\\no.exe"}"#,
                ),
                (
                    "9",
                    r#"{"type":"launch","target":"shell:AppsFolder\\Store.App!App"}"#,
                ),
                (
                    "4",
                    r#"{"type":"launch","target":"shell:AppsFolder\\Gone.App!App"}"#,
                ),
                ("5", r#"{"type":"open","target":"https://nowhere.example"}"#),
                ("6", r#"{"type":"open","target":"mailto:a@b.c"}"#),
                ("1", r#"{"type":"open","target":"C:\\docs"}"#),
                ("2", r#"{"type":"open","target":"C:\\nope"}"#),
                (
                    "3",
                    r#"{"type":"sound","file":"sounds/0123456789abcdef.wav"}"#,
                ),
                (
                    "0",
                    r#"{"type":"sound","file":"sounds/ffffffffffffffff.wav"}"#,
                ),
                ("+", r#"{"type":"sound","file":"C:\\snd\\ok.wav"}"#),
                ("Enter", r#"{"type":"sound","file":"C:\\snd\\gone.wav"}"#),
            ],
        );
        let b = bundle_of(
            Kind::Pages,
            &[page],
            &[("sounds/0123456789abcdef.wav", b"x")],
        );
        let mut missing: Vec<(String, bool)> = notes_of(&b, &probe)
            .iter()
            .map(|n| (n.key.clone(), n.missing))
            .collect();
        missing.sort();
        let mut want: Vec<(String, bool)> = [
            ("7", false),
            ("8", true),
            ("9", false),
            ("4", true),
            ("5", false),
            ("6", false),
            ("1", false),
            ("2", true),
            ("3", false),
            ("0", true),
            ("+", false),
            ("Enter", true),
        ]
        .iter()
        .map(|(k, c)| (k.to_string(), *c))
        .collect();
        want.sort();
        assert_eq!(missing, want);
    }

    #[test]
    fn preview_does_not_judge_uris_and_bare_names() {
        let page = page_with(
            "p",
            &[
                ("7", r#"{"type":"open","target":"ms-settings:apps-volume"}"#),
                ("8", r#"{"type":"open","target":"steam://run/1"}"#),
                ("9", r#"{"type":"launch","target":"notepad.exe"}"#),
                ("4", r#"{"type":"launch","target":"steam://run/1"}"#),
                ("5", r#"{"type":"open","target":"D:\\nope"}"#),
                ("6", r#"{"type":"launch","target":"D:\\nope.exe"}"#),
            ],
        );
        let b = bundle_of(Kind::Pages, &[page], &[]);
        let mut m: Vec<(String, bool)> = notes_of(&b, &FakeProbe::default())
            .iter()
            .map(|n| (n.key.clone(), n.missing))
            .collect();
        m.sort();
        let want = [
            ("4", false),
            ("5", true),
            ("6", true),
            ("7", false),
            ("8", false),
            ("9", false),
        ];
        let want: Vec<(String, bool)> = want.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        assert_eq!(m, want);
    }

    #[test]
    fn default_preset_flags_only_real_paths() {
        for lang in [Lang::Ko, Lang::En] {
            let cfg = crate::presets::default_config(lang);
            let mut b = bundle_of(Kind::Pages, &[], &[]);
            b.manifest.pages = cfg.pages;
            let missing_of = |probe: &FakeProbe| -> Vec<String> {
                preview(&b, 1, probe)
                    .pages
                    .iter()
                    .flat_map(|p| p.notes.iter())
                    .filter(|n| n.missing)
                    .map(|n| n.key.clone())
                    .collect()
            };
            // 주소(https, ms-settings)는 판정 제외. 음악 폴더 버튼만 경로라 판정한다.
            assert_eq!(missing_of(&FakeProbe::default()), ["2"]);
            let mut probe = FakeProbe::default();
            probe.dirs.insert(expand_env(r"%USERPROFILE%\Music"));
            assert!(missing_of(&probe).is_empty());
        }
    }

    #[test]
    fn preview_sequence_missing_if_any_step_missing() {
        let page = page_with(
            "p",
            &[
                (
                    "7",
                    r#"{"type":"sequence","steps":[{"type":"sound","file":"sounds/ffffffffffffffff.wav"},{"type":"text","text":"x"}]}"#,
                ),
                (
                    "8",
                    r#"{"type":"sequence","steps":[{"type":"delay","ms":5},{"type":"launch","target":"C:\\none.exe"}]}"#,
                ),
            ],
        );
        let b = bundle_of(Kind::Pages, &[page], &[]);
        let n = notes_of(&b, &FakeProbe::default());
        assert!(n.iter().all(|n| n.missing && n.caution), "{n:?}");
    }

    #[test]
    fn preview_backup_carries_settings() {
        let mut b = bundle_of(Kind::Backup, &[mic_page("p")], &[]);
        b.manifest.system_keys =
            Some(serde_json::from_str(r#"{"prev":"*","next":"/","osd":"-","first":"."}"#).unwrap());
        let pv = preview(&b, 1, &FakeProbe::default());
        assert_eq!(pv.kind, Kind::Backup);
        assert!(pv.system_keys.is_some());
    }

    fn two_pages() -> Vec<String> {
        vec![mic_page("Music"), mic_page("Work")]
    }

    fn choice(pages: &[usize], mode: InstallMode) -> InstallChoice {
        InstallChoice {
            preview_id: 1,
            pages: pages.to_vec(),
            mode,
        }
    }

    #[test]
    fn install_add_renames_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let cur = config_of(&[mic_page("Music")], "");
        let b = bundle_of(Kind::Pages, &two_pages(), &[]);
        let out = install(&cur, &b, &choice(&[0, 0, 1], InstallMode::Add), dir.path()).unwrap();
        let names: Vec<&str> = out.pages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Music", "Music (2)", "Music (3)", "Work"]);
        // 또 설치하면 다음 번호
        let out2 = install(&out, &b, &choice(&[0], InstallMode::Add), dir.path()).unwrap();
        assert_eq!(out2.pages.last().unwrap().name, "Music (4)");
    }

    #[test]
    fn install_add_ignores_manifest_settings() {
        let dir = tempfile::tempdir().unwrap();
        let cur = config_of(&[mic_page("A")], r#""language":"ko","#);
        let mut b = bundle_of(Kind::Pages, &two_pages(), &[]);
        b.manifest.language = Some(Lang::En);
        b.manifest.mic_indicator = Some(false);
        let out = install(&cur, &b, &choice(&[0], InstallMode::Add), dir.path()).unwrap();
        assert_eq!(out.language, cur.language);
        assert_eq!(out.mic_indicator, None);
    }

    fn sound_page(name: &str) -> String {
        page_with(
            name,
            &[
                (
                    "7",
                    r#"{"type":"sound","file":"sounds/0123456789abcdef.wav"}"#,
                ),
                (
                    "8",
                    r#"{"type":"sequence","steps":[{"type":"sound","file":"sounds/0123456789abcdef.wav"},{"type":"sound","file":"sounds/ffffffffffffffff.wav"}]}"#,
                ),
            ],
        )
    }

    #[test]
    fn install_writes_sounds_once_and_rewrites_paths() {
        let dir = tempfile::tempdir().unwrap();
        let cur = config_of(&[mic_page("A")], "");
        let b = bundle_of(
            Kind::Pages,
            &[sound_page("S")],
            &[("sounds/0123456789abcdef.wav", b"data")],
        );
        let out = install(&cur, &b, &choice(&[0], InstallMode::Add), dir.path()).unwrap();
        let mut files = vec![];
        for_each_action(&out.pages[1], &mut |a| {
            if let Action::Sound { file, .. } = a {
                files.push(file.clone());
            }
        });
        let want = r"%APPDATA%\NumpadDeck\sounds\0123456789abcdef.wav";
        // 묶음에 없는 소리는 그대로
        assert_eq!(files, [want, want, "sounds/ffffffffffffffff.wav"]);
        let sdir = dir.path().join("sounds");
        assert_eq!(
            std::fs::read(sdir.join("0123456789abcdef.wav")).unwrap(),
            b"data"
        );
        assert!(std::fs::read_dir(&sdir).unwrap().all(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "wav")));
        // 두 번째 설치: 파일은 하나, 덮어쓰지 않음
        std::fs::write(sdir.join("0123456789abcdef.wav"), b"mine").unwrap();
        install(&out, &b, &choice(&[0], InstallMode::Add), dir.path()).unwrap();
        assert_eq!(std::fs::read_dir(&sdir).unwrap().count(), 1); // .tmp도 남지 않음
        assert_eq!(
            std::fs::read(sdir.join("0123456789abcdef.wav")).unwrap(),
            b"mine"
        );
    }

    #[test]
    fn install_replace_requires_backup_kind() {
        let dir = tempfile::tempdir().unwrap();
        let cur = config_of(&[mic_page("A")], "");
        let b = bundle_of(Kind::Pages, &two_pages(), &[]);
        let r = install(&cur, &b, &choice(&[0], InstallMode::Replace), dir.path());
        assert!(matches!(r, Err(ShareError::Invalid(_))), "{r:?}");
    }

    #[test]
    fn install_replace_keeps_settings() {
        let dir = tempfile::tempdir().unwrap();
        let cur = config_of(&[mic_page("Old")], r#""language":"ko","#);
        let mut b = bundle_of(Kind::Backup, &two_pages(), &[]);
        b.manifest.language = Some(Lang::En);
        b.manifest.mic_indicator = Some(false);
        b.manifest.system_keys =
            Some(serde_json::from_str(r#"{"prev":"*","next":"/","osd":"-","first":"."}"#).unwrap());
        let out = install(&cur, &b, &choice(&[1], InstallMode::Replace), dir.path()).unwrap();
        assert_eq!(out.pages.len(), 1);
        assert_eq!(out.pages[0].name, "Work");
        assert_eq!(out.language, Some(Lang::En));
        assert_eq!(out.mic_indicator, Some(false));
        assert_eq!(out.system_keys, b.manifest.system_keys);
        let none = install(&cur, &b, &choice(&[], InstallMode::Replace), dir.path());
        assert!(matches!(none, Err(ShareError::Invalid(_))));
    }

    #[test]
    fn install_rejects_out_of_range() {
        let dir = tempfile::tempdir().unwrap();
        let cur = config_of(&[mic_page("A")], "");
        let b = bundle_of(Kind::Pages, &two_pages(), &[]);
        let r = install(&cur, &b, &choice(&[0, 2], InstallMode::Add), dir.path());
        assert!(matches!(r, Err(ShareError::Invalid(_))), "{r:?}");
        assert!(!dir.path().join("sounds").exists());
    }

    /// 백업 시험용: 설정 파일을 만들고 (설정 파일 경로, 설정, 사용자 폴더)를 돌려준다.
    fn backup_fixture(dir: &Path, raw: &[u8]) -> (PathBuf, Config, UserDirs) {
        let cfg_path = dir.join("config.json");
        std::fs::write(&cfg_path, raw).unwrap();
        let cfg = config_of(&[page_json("A", "x.wav")], "");
        (cfg_path, cfg, test_dirs(dir))
    }

    /// 61초 간격의 시각 (1971-01-01 근처).
    fn minute_after(i: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(86_400 * 365)
            + std::time::Duration::from_secs(i * 61)
    }

    #[test]
    fn backup_keeps_latest_ten() {
        let dir = tempfile::tempdir().unwrap();
        let (path, cfg, dirs) = backup_fixture(dir.path(), b"{}");
        let mut first = None;
        for i in 0..11u64 {
            let p = backup_config(&path, &cfg, false, "0.6.0", &dirs, minute_after(i)).unwrap();
            first.get_or_insert(p);
        }
        let first = first.unwrap();
        // 현지 시각이라 값은 시간대에 따라 다르다: 형식만 본다
        let fname = first.file_name().unwrap().to_string_lossy().into_owned();
        assert!(backup_sort_key(&fname).is_some(), "{fname}");
        assert!(fname.ends_with(".npdeck"), "{fname}");
        assert!(!first.exists());
        let n = std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .count();
        assert_eq!(n, 10);
    }

    #[test]
    fn backup_same_second_does_not_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let (path, cfg, dirs) = backup_fixture(dir.path(), b"one");
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        let a = backup_config(&path, &cfg, true, "0.6.0", &dirs, t).unwrap();
        std::fs::write(&path, b"two").unwrap();
        let b = backup_config(&path, &cfg, true, "0.6.0", &dirs, t).unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read(&a).unwrap(), b"one");
        assert_eq!(std::fs::read(&b).unwrap(), b"two");
        assert_eq!(
            a.file_name().unwrap().to_string_lossy().len(),
            "config-20010909-014640.json".len()
        );
        // 확장자가 달라도 같은 초면 번호가 겹치지 않는다
        let c = backup_config(&path, &cfg, false, "0.6.0", &dirs, t).unwrap();
        let name = |p: &Path| p.file_name().unwrap().to_string_lossy().into_owned();
        let keys: BTreeSet<_> = [&a, &b, &c]
            .iter()
            .map(|p| backup_sort_key(&name(p)).unwrap())
            .collect();
        assert_eq!(keys.len(), 3, "{a:?} {b:?} {c:?}");
    }

    #[test]
    fn backup_is_npdeck_backup_file() {
        let dir = tempfile::tempdir().unwrap();
        let wav = write_file(dir.path(), "snd/a.wav", b"RIFFdata");
        let cfg = config_of(
            &[page_json("A", &wav), page_json("B", "none.wav")],
            r#""language":"en","micIndicator":false,"#,
        );
        // 설정 파일이 아직 없어도 메모리의 설정을 백업한다
        let path = dir.path().join("config.json");
        let dirs = test_dirs(dir.path());
        let p = backup_config(&path, &cfg, false, "0.6.0", &dirs, minute_after(0)).unwrap();
        assert_eq!(p.extension().unwrap(), "npdeck");
        assert_eq!(p.parent().unwrap(), dir.path().join("backups"));
        let b = read(&p).unwrap();
        assert_eq!(b.manifest.kind, Kind::Backup);
        assert_eq!(b.manifest.pages.len(), 2);
        assert_eq!(b.manifest.pages[0].name, "A");
        assert_eq!(b.manifest.pages[1].name, "B");
        assert_eq!(b.manifest.language, Some(Lang::En));
        assert_eq!(b.manifest.mic_indicator, Some(false));
        assert_eq!(b.sounds.len(), 1);
        // 임시 파일이 남지 않고, 설정 파일은 만들지 않는다
        let names: Vec<_> = std::fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");
        assert!(!path.exists());
    }

    #[test]
    fn backup_leaves_out_sounds_it_cannot_hold() {
        let dir = tempfile::tempdir().unwrap();
        let ok = write_file(dir.path(), "ok.wav", b"ok");
        let big = write_file(dir.path(), "big.wav", &vec![1u8; MAX_SOUND as usize + 1]);
        let missing = dir.path().join("none.wav").to_string_lossy().into_owned();
        // 4MiB짜리 서로 다른 소리 6개(24MiB): 합계 한도(20MiB)를 넘는 만큼은 빠진다.
        let fillers: Vec<String> = (0..6u8)
            .map(|i| {
                write_file(
                    dir.path(),
                    &format!("f{i}.wav"),
                    &vec![i + 10; 4 * 1024 * 1024],
                )
            })
            .collect();
        let mut buttons = vec![
            ("7", ok.clone()),
            ("8", big.clone()),
            ("9", missing.clone()),
        ];
        let keys = ["4", "5", "6", "1", "2", "3"];
        for (k, f) in keys.iter().zip(&fillers) {
            buttons.push((k, f.clone()));
        }
        let body: Vec<String> = buttons
            .iter()
            .map(|(k, f)| {
                format!(
                    r#""{k}":{{"label":"s","action":{{"type":"sound","file":"{}"}}}}"#,
                    esc(f)
                )
            })
            .collect();
        let page = format!(r#"{{"name":"P","buttons":{{{}}}}}"#, body.join(","));
        let cfg = config_of(&[page], "");
        // 사용자용 내보내기는 그대로 실패한다
        assert!(export_to(&cfg, &Selection::All, dir.path()).is_err());
        let path = dir.path().join("config.json");
        let dirs = test_dirs(dir.path());
        let p = backup_config(&path, &cfg, false, "0.6.0", &dirs, minute_after(0)).unwrap();
        let b = read(&p).unwrap();
        let page = &b.manifest.pages[0];
        assert_eq!(page.buttons.len(), 9, "버튼은 모두 남는다");
        // 키 이름(JSON 키)으로 소리 파일 경로를 찾는다
        let json = serde_json::to_value(page).unwrap();
        let file_of = |k: &str| {
            json["buttons"][k]["action"]["file"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(file_of("8"), portable_path(&big, &dirs));
        assert_eq!(file_of("9"), portable_path(&missing, &dirs));
        // 합계 한도(20MiB) 안에서만 담기고, 못 담은 소리는 원래 경로가 그대로 남는다
        let all_keys = ["7", "4", "5", "6", "1", "2", "3"];
        let bundled = all_keys
            .iter()
            .filter(|k| file_of(k).starts_with("sounds/"))
            .count();
        assert!(bundled >= 4 && bundled < all_keys.len(), "{bundled}");
        assert_eq!(b.sounds.len(), bundled);
        let total: u64 = b.sounds.values().map(|v| v.len() as u64).sum();
        assert!(total <= MAX_SOUNDS_TOTAL);
        for (k, orig) in [("4", &fillers[0]), ("5", &fillers[1]), ("6", &fillers[2])] {
            let f = file_of(k);
            assert!(
                f.starts_with("sounds/") || f == portable_path(orig, &dirs),
                "{k}: {f}"
            );
        }
    }

    #[test]
    fn backup_of_broken_config_copies_raw_json() {
        let dir = tempfile::tempdir().unwrap();
        let raw = b"{ this is not valid json \xff";
        let (path, cfg, dirs) = backup_fixture(dir.path(), raw);
        let p = backup_config(&path, &cfg, true, "0.6.0", &dirs, minute_after(0)).unwrap();
        assert_eq!(p.extension().unwrap(), "json");
        assert_eq!(std::fs::read(&p).unwrap(), raw);
        assert_eq!(std::fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn backup_rotation_mixes_json_and_npdeck() {
        let dir = tempfile::tempdir().unwrap();
        let (path, cfg, dirs) = backup_fixture(dir.path(), b"{}");
        let mut made = vec![];
        for i in 0..13u64 {
            // 번갈아 두 종류를 만든다
            let p =
                backup_config(&path, &cfg, i % 2 == 0, "0.6.0", &dirs, minute_after(i)).unwrap();
            made.push(p);
        }
        let bdir = dir.path().join("backups");
        assert_eq!(std::fs::read_dir(&bdir).unwrap().count(), 10);
        // 가장 오래된 3개가 지워지고 최신 10개가 남는다
        for p in &made[..3] {
            assert!(!p.exists(), "{p:?}");
        }
        for p in &made[3..] {
            assert!(p.exists(), "{p:?}");
        }
        let exts: BTreeSet<_> = made[3..]
            .iter()
            .map(|p| p.extension().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(exts.len(), 2);
    }

    #[test]
    fn backup_sort_key_accepts_both_extensions() {
        assert!(backup_sort_key("config-20010909-014640.json").is_some());
        assert_eq!(
            backup_sort_key("config-20010909-014640-3.npdeck"),
            Some(("20010909-014640".into(), 3))
        );
        assert!(backup_sort_key("config-20010909-014640.npdeck.tmp").is_none());
        assert!(backup_sort_key("config-20010909-014640.txt").is_none());
    }

    #[test]
    fn utc_parts_known_instants() {
        // 2001-09-09 01:46:40 UTC, 1971-01-01, 윤일 2024-02-29 12:00:00
        assert_eq!(format_stamp(utc_parts(1_000_000_000)), "20010909-014640");
        assert_eq!(format_stamp(utc_parts(86_400 * 365)), "19710101-000000");
        assert_eq!(format_stamp(utc_parts(1_709_208_000)), "20240229-120000");
    }

    #[test]
    fn backup_never_deletes_the_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("config.json");
        std::fs::write(&cfg, b"{}").unwrap();
        let bdir = dir.path().join("backups");
        std::fs::create_dir_all(&bdir).unwrap();
        // 미래 시각 이름 10개 (시계가 되감긴 경우)
        for i in 0..10 {
            std::fs::write(bdir.join(format!("config-29990101-00000{i}.json")), b"x").unwrap();
        }
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        let (_, config, dirs) = backup_fixture(dir.path(), b"{}");
        let p = backup_config(&cfg, &config, false, "0.6.0", &dirs, t).unwrap();
        assert!(p.exists());
        assert_eq!(std::fs::read_dir(&bdir).unwrap().count(), 10);
    }
}
