//! 페이지 공유 파일(`.npdeck`) 형식과 경로 이식성.

use std::collections::BTreeMap;
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

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
                            error = Some(ShareError::SoundTooBig(name));
                            return;
                        }
                        let bytes = match std::fs::read(&path) {
                            Ok(b) => b,
                            Err(e) => {
                                error = Some(io_err(e));
                                return;
                            }
                        };
                        if bytes.len() as u64 > MAX_SOUND {
                            error = Some(ShareError::SoundTooBig(name));
                            return;
                        }
                        let entry = sound_entry_name(&bytes, &ext);
                        if !sounds.contains_key(&entry) {
                            total += bytes.len() as u64;
                            if total > MAX_SOUNDS_TOTAL {
                                error = Some(ShareError::SoundsTooBig);
                                return;
                            }
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
    Ok(Bundle { manifest, sounds })
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
        make_zip(
            &path,
            &[
                ("manifest.json", manifest_bytes(1, page)),
                ("sounds/0123456789abcdef.wav", b"x".to_vec()),
            ],
        );
        assert_eq!(read(&path).unwrap().sounds.len(), 1);
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
        let names: Vec<String> = (0..5).map(|i| format!("sounds/{i:016x}.wav")).collect();
        let mut all = vec![("manifest.json", manifest_bytes(1, page))];
        for n in &names {
            all.push((n.as_str(), part.clone()));
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
}
