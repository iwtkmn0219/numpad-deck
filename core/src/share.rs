//! 페이지 공유 파일(`.npdeck`) 형식과 경로 이식성.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::{Action, KeyPlace, Page, SystemKeys};
use crate::i18n::Lang;

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
}
