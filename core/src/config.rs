//! 설정 파일(JSON) 스키마, 읽기·쓰기, 변경 감시.
//!
//! 사람이 읽고 고치고 백업할 수 있어야 하므로 버튼은 배열 순서가 아니라 키 이름
//! (`"7"`, `"+"`, `"Enter"`)으로 적는다. 빈 키는 생략한다.
//!
//! 읽기나 검증에 실패하면 호출한 쪽이 직전 설정을 유지한다. 이 모듈은 실패를
//! 사용자에게 보여줄 수 있는 메시지([`ConfigError`])로 돌려줄 뿐이다.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode, DebounceEventResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ts_rs::TS;

use crate::i18n::{tr, Lang};
use crate::key::NumpadKey;
use crate::layout::{self, Role, BUTTON_KEYS};

/// 현재 스키마 버전.
pub const VERSION: u32 = 1;

/// 묶음 안 지연 시간의 상한. 실수로 큰 값을 넣어 버튼이 멈춘 듯 보이는 것을 막는다.
pub const MAX_DELAY_MS: u64 = 60_000;

// TypeScript 타입(`app/src/bindings/*.ts`)은 `cargo test`가 만든다 (ts-rs). 타입을 바꾸면
// `cargo test -p numpad-deck-core`를 돌려 다시 만들고 함께 커밋한다.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct Config {
    pub version: u32,
    /// UI·메시지 언어. 없으면 Windows 표시 언어를 따른다.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub language: Option<Lang>,
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct Page {
    pub name: String,
    #[serde(default)]
    #[ts(as = "BTreeMap<ButtonKeyName, Button>")]
    pub buttons: BTreeMap<ButtonKey, Button>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct Button {
    pub label: String,
    /// Lucide 아이콘 이름 (예: `"play"`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub icon: Option<String>,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../app/src/bindings/")]
pub enum Action {
    /// 키 이름 배열. 예: `["Ctrl", "Shift", "S"]`. 이름은 [`crate::keys`] 참고.
    Hotkey {
        keys: Vec<String>,
    },
    /// 실행 파일 경로, `shell:AppsFolder\<AUMID>` 등.
    Launch {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        args: Option<String>,
    },
    /// URL 또는 폴더.
    Open {
        target: String,
    },
    Text {
        text: String,
    },
    Media {
        command: MediaCommand,
    },
    MicMute,
    Sound {
        file: String,
        /// 0–1. 없으면 1.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        volume: Option<f32>,
    },
    /// 재생 중인 효과음을 모두 멈춘다.
    StopSounds,
    Sequence {
        steps: Vec<Action>,
    },
    /// `sequence` 안에서만 쓸 수 있다.
    Delay {
        #[ts(type = "number")]
        ms: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../app/src/bindings/")]
pub enum MediaCommand {
    PlayPause,
    Next,
    Prev,
    Stop,
    VolumeUp,
    VolumeDown,
    VolumeMute,
}

/// TypeScript 타입 생성 전용: 버튼 키 이름 (`"7"`, `"+"`, `"Enter"` …).
/// 실제 값은 [`ButtonKey`]가 다룬다.
#[allow(dead_code)]
#[derive(TS)]
#[ts(export, export_to = "../../app/src/bindings/")]
enum ButtonKeyName {
    #[ts(rename = "7")]
    N7,
    #[ts(rename = "8")]
    N8,
    #[ts(rename = "9")]
    N9,
    #[ts(rename = "+")]
    Add,
    #[ts(rename = "4")]
    N4,
    #[ts(rename = "5")]
    N5,
    #[ts(rename = "6")]
    N6,
    #[ts(rename = "1")]
    N1,
    #[ts(rename = "2")]
    N2,
    #[ts(rename = "3")]
    N3,
    #[ts(rename = "Enter")]
    Enter,
    #[ts(rename = "0")]
    N0,
}

/// 버튼 역할을 하는 넘패드 키 (12개). JSON에서는 키캡 글자로 적는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ButtonKey(NumpadKey);

impl ButtonKey {
    pub fn new(key: NumpadKey) -> Option<Self> {
        matches!(layout::role(key), Role::Button(_)).then_some(Self(key))
    }

    pub fn key(self) -> NumpadKey {
        self.0
    }

    /// [`BUTTON_KEYS`] 안의 순서.
    pub fn index(self) -> usize {
        match layout::role(self.0) {
            Role::Button(i) => i,
            _ => unreachable!("ButtonKey는 버튼 키만 담는다"),
        }
    }
}

/// 파일에 적힐 때 키 배치 순서(7 8 9 + 4 5 6 1 2 3 Enter 0)를 따르게 한다.
impl Ord for ButtonKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.index().cmp(&other.index())
    }
}

impl PartialOrd for ButtonKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Serialize for ButtonKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0.label())
    }
}

impl<'de> Deserialize<'de> for ButtonKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        BUTTON_KEYS
            .iter()
            .find(|k| k.label().eq_ignore_ascii_case(&name))
            .map(|&k| ButtonKey(k))
            .ok_or_else(|| {
                let names: Vec<&str> = BUTTON_KEYS.iter().map(|k| k.label()).collect();
                let names = names.join(" ");
                serde::de::Error::custom(tr(
                    format!("버튼 키 \"{name}\"를 알 수 없습니다. 쓸 수 있는 키: {names}"),
                    format!("Unknown button key \"{name}\". Available keys: {names}"),
                ))
            })
    }
}

impl Config {
    /// 첫 실행 때 만드는 기본 설정 (버튼 이름은 지금 언어로).
    pub fn default_config() -> Config {
        crate::presets::default_config(crate::i18n::current())
    }

    /// 현재 페이지에서 키에 해당하는 버튼.
    pub fn button(&self, page: usize, key: NumpadKey) -> Option<&Button> {
        let key = ButtonKey::new(key)?;
        self.pages.get(page)?.buttons.get(&key)
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            let v = self.version;
            return Err(tr(
                format!("설정 파일 버전 {v}은 지원하지 않습니다 (지원: {VERSION})."),
                format!("Config file version {v} is not supported (supported: {VERSION})."),
            ));
        }
        if self.pages.is_empty() {
            return Err(tr(
                "페이지가 하나 이상 있어야 합니다.",
                "There must be at least one page.",
            ));
        }
        for (p, page) in self.pages.iter().enumerate() {
            for (key, button) in &page.buttons {
                let at = || {
                    let (n, name, key) = (p + 1, &page.name, key.0.label());
                    tr(
                        format!("{n}번째 페이지 \"{name}\"의 {key} 키"),
                        format!("Page {n} \"{name}\", key {key}"),
                    )
                };
                validate_action(&button.action, false).map_err(|e| format!("{}: {e}", at()))?;
            }
        }
        Ok(())
    }
}

fn validate_action(action: &Action, in_sequence: bool) -> Result<(), String> {
    match action {
        Action::Hotkey { keys } if keys.is_empty() => {
            Err(tr("단축키가 비어 있습니다.", "The shortcut is empty."))
        }
        Action::Hotkey { keys } => crate::keys::parse_all(keys).map(|_| ()),
        Action::Launch { target, .. } | Action::Open { target } if target.trim().is_empty() => {
            Err(tr("대상이 비어 있습니다.", "The target is empty."))
        }
        Action::Sound { file, .. } if file.trim().is_empty() => {
            Err(tr("소리 파일이 비어 있습니다.", "The sound file is empty."))
        }
        Action::Sound {
            volume: Some(v), ..
        } if !(0.0..=1.0).contains(v) => Err(tr(
            format!("볼륨 {v}은 0에서 1 사이여야 합니다."),
            format!("Volume {v} must be between 0 and 1."),
        )),
        Action::Delay { .. } if !in_sequence => Err(tr(
            "지연(delay)은 묶음(sequence) 안에서만 쓸 수 있습니다.",
            "A delay can only be used inside a sequence.",
        )),
        Action::Delay { ms } if *ms > MAX_DELAY_MS => Err(tr(
            format!("지연 {ms}ms가 너무 깁니다 (최대 {MAX_DELAY_MS}ms)."),
            format!("Delay {ms}ms is too long (max {MAX_DELAY_MS}ms)."),
        )),
        Action::Sequence { .. } if in_sequence => Err(tr(
            "묶음 안에 다시 묶음을 넣을 수 없습니다.",
            "A sequence cannot contain another sequence.",
        )),
        Action::Sequence { steps } if steps.is_empty() => {
            Err(tr("묶음이 비어 있습니다.", "The sequence is empty."))
        }
        Action::Sequence { steps } => steps.iter().try_for_each(|s| validate_action(s, true)),
        _ => Ok(()),
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(io::Error),
    /// JSON 문법이나 형식 오류. 줄·칸은 1부터 센다.
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Io(e) => f.write_str(&tr(
                format!("설정 파일을 읽거나 쓸 수 없습니다: {e}"),
                format!("Cannot read or write the config file: {e}"),
            )),
            ConfigError::Syntax {
                line,
                column,
                message,
            } => f.write_str(&tr(
                format!("설정 파일 {line}번째 줄 {column}번째 칸: {message}"),
                format!("Config file line {line}, column {column}: {message}"),
            )),
            ConfigError::Invalid(msg) => f.write_str(&tr(
                format!("설정이 올바르지 않습니다: {msg}"),
                format!("Invalid config: {msg}"),
            )),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<io::Error> for ConfigError {
    fn from(e: io::Error) -> Self {
        ConfigError::Io(e)
    }
}

/// `%APPDATA%\NumpadDeck\config.json`
pub fn default_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("NumpadDeck")
            .join("config.json"),
    )
}

pub fn parse(text: &str) -> Result<Config, ConfigError> {
    // 메모장이 붙이는 BOM을 허용한다.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let config: Config = serde_json::from_str(text).map_err(|e| ConfigError::Syntax {
        line: e.line(),
        column: e.column(),
        message: syntax_message(&e),
    })?;
    config.validate().map_err(ConfigError::Invalid)?;
    Ok(config)
}

/// serde_json 오류를 사용자에게 보여줄 문장으로. 위치는 [`ConfigError::Syntax`]가 따로 갖는다.
fn syntax_message(e: &serde_json::Error) -> String {
    use serde_json::error::Category;
    match e.classify() {
        Category::Syntax => tr(
            "JSON 문법이 맞지 않습니다. 쉼표, 따옴표, 괄호를 확인하세요.",
            "Invalid JSON. Check commas, quotes and brackets.",
        ),
        Category::Eof => tr(
            "파일이 중간에 끝났습니다. 닫는 괄호가 빠졌는지 확인하세요.",
            "The file ends too early. Check for a missing closing bracket.",
        ),
        Category::Io => tr(
            "파일을 읽는 중 오류가 났습니다.",
            "Error while reading the file.",
        ),
        // 형식 오류: 직접 만든 메시지(지금 언어)이거나 serde의 영어 메시지(필드 이름 포함).
        Category::Data => {
            let full = e.to_string();
            match full.rsplit_once(" at line ") {
                Some((message, _)) => message.to_string(),
                None => full,
            }
        }
    }
}

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    parse(&fs::read_to_string(path)?)
}

/// 파일이 없으면 기본 설정을 저장하고 돌려준다.
pub fn load_or_create(path: &Path) -> Result<Config, ConfigError> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let config = Config::default_config();
            save(path, &config)?;
            Ok(config)
        }
        Err(e) => Err(e.into()),
    }
}

/// 임시 파일에 쓴 뒤 이름을 바꿔서, 절반만 쓰인 파일이 읽히지 않게 한다.
pub fn save(path: &Path, config: &Config) -> Result<(), ConfigError> {
    config.validate().map_err(ConfigError::Invalid)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(config).expect("설정은 항상 직렬화된다");
    text.push('\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// 설정 파일 감시. 버리면 감시를 멈춘다.
pub struct ConfigWatcher {
    _debouncer: notify_debouncer_mini::Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>,
}

/// 파일이 바뀔 때마다 다시 읽어 `on_change`로 넘긴다. 짧은 시간 안의 변경은 한 번으로
/// 모은다. 내용이 직전에 넘긴 설정과 같으면 넘기지 않는다 (앱이 직접 저장한 경우 등).
///
/// 파일을 직접 감시하면 [`save`]의 이름 바꾸기 뒤에 감시가 끊기므로 폴더를 감시한다.
pub fn watch<F>(path: &Path, initial: Config, on_change: F) -> Result<ConfigWatcher, ConfigError>
where
    F: Fn(Result<Config, ConfigError>) + Send + 'static,
{
    let dir = path
        .parent()
        .ok_or_else(|| {
            ConfigError::Invalid(tr(
                "설정 파일 경로에 폴더가 없습니다.",
                "The config file path has no folder.",
            ))
        })?
        .to_path_buf();
    let file_name = path.file_name().map(|n| n.to_os_string());
    let path = path.to_path_buf();

    let (tx, rx) = mpsc::channel::<()>();
    let mut debouncer = new_debouncer(
        Duration::from_millis(200),
        move |res: DebounceEventResult| {
            if let Ok(events) = res {
                if events
                    .iter()
                    .any(|e| e.path.file_name() == file_name.as_deref())
                {
                    let _ = tx.send(());
                }
            }
        },
    )
    .map_err(|e| ConfigError::Io(io::Error::other(e)))?;
    debouncer
        .watcher()
        .watch(&dir, RecursiveMode::NonRecursive)
        .map_err(|e| ConfigError::Io(io::Error::other(e)))?;

    thread::Builder::new()
        .name("deck-config-watch".into())
        .spawn(move || reload_loop(&path, initial, rx, on_change))?;

    Ok(ConfigWatcher {
        _debouncer: debouncer,
    })
}

fn reload_loop<F>(path: &Path, mut last: Config, changes: mpsc::Receiver<()>, on_change: F)
where
    F: Fn(Result<Config, ConfigError>),
{
    // 감시가 멈추면(ConfigWatcher를 버리면) 송신 쪽이 사라져 루프가 끝난다.
    for () in changes {
        match load(path) {
            Ok(config) if config == last => {}
            Ok(config) => {
                last = config.clone();
                on_change(Ok(config));
            }
            // 저장 도중의 순간을 읽은 경우 곧 다음 변경이 온다.
            Err(ConfigError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => on_change(Err(e)),
        }
    }
}

/// [`watch`]의 결과를 채널로 받고 싶을 때.
pub fn watch_into(
    path: &Path,
    initial: Config,
    tx: Sender<Result<Config, ConfigError>>,
) -> Result<ConfigWatcher, ConfigError> {
    watch(path, initial, move |r| {
        let _ = tx.send(r);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn language_is_optional() {
        let auto = r#"{"version":1,"pages":[{"name":"p"}]}"#;
        let config = parse(auto).unwrap();
        assert_eq!(config.language, None);
        // 자동이면 파일에 적지 않는다
        assert!(!serde_json::to_string(&config).unwrap().contains("language"));

        let en = r#"{"version":1,"language":"en","pages":[{"name":"p"}]}"#;
        assert_eq!(parse(en).unwrap().language, Some(Lang::En));
        assert!(parse(r#"{"version":1,"language":"fr","pages":[{"name":"p"}]}"#).is_err());
    }

    #[test]
    fn default_config_is_valid_and_round_trips() {
        let config = Config::default_config();
        assert_eq!(config.version, VERSION);
        assert!(!config.pages.is_empty());
        let text = serde_json::to_string_pretty(&config).unwrap();
        assert_eq!(parse(&text).unwrap(), config);
    }

    #[test]
    fn m2_test_config_is_valid() {
        let config = parse(include_str!("../../docs/m2-test-config.json")).unwrap();
        assert_eq!(config.pages.len(), 2);
    }

    #[test]
    fn buttons_are_written_in_layout_order() {
        let config = Config::default_config();
        let text = serde_json::to_string(&config.pages[0].buttons).unwrap();
        let pos = |k: &str| text.find(&format!("\"{k}\":")).unwrap();
        assert!(pos("7") < pos("+"));
        assert!(pos("+") < pos("4"));
        assert!(pos("3") < pos("Enter"));
    }

    #[test]
    fn button_lookup_by_numpad_key() {
        let config = Config::default_config();
        assert_eq!(config.button(0, NumpadKey::N8).unwrap().label, "재생/정지");
        assert!(
            config.button(0, NumpadKey::Divide).is_none(),
            "시스템 키는 버튼이 아니다"
        );
        assert!(config.button(99, NumpadKey::N8).is_none());
    }

    #[test]
    fn system_keys_cannot_be_buttons() {
        let text = r#"{"version":1,"pages":[{"name":"p","buttons":{
            "/": {"label":"x","action":{"type":"micMute"}}}}]}"#;
        let err = parse(text).unwrap_err().to_string();
        assert!(err.contains("\"/\""), "{err}");
    }

    #[test]
    fn key_names_are_case_insensitive() {
        let text = r#"{"version":1,"pages":[{"name":"p","buttons":{
            "enter": {"label":"x","action":{"type":"micMute"}}}}]}"#;
        let config = parse(text).unwrap();
        assert!(config.button(0, NumpadKey::Enter).is_some());
    }

    #[test]
    fn syntax_error_reports_line() {
        let text = "{\n  \"version\": 1,\n  \"pages\": [,]\n}";
        match parse(text).unwrap_err() {
            ConfigError::Syntax { line, .. } => assert_eq!(line, 3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn error_messages_have_no_duplicate_position() {
        let syntax = parse(
            "{
  \"version\": 1,
  \"pages\": [,]
}",
        )
        .unwrap_err();
        let data = parse(r#"{"version":1,"pages":[{"name":"p","buttons":{"/":{"label":"a","action":{"type":"micMute"}}}}]}"#)
            .unwrap_err();
        for err in [syntax, data] {
            let text = err.to_string();
            assert!(!text.contains(" at line "), "{text}");
        }
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let text = r#"{"version":1,"pages":[{"name":"p","colour":"red"}]}"#;
        assert!(matches!(parse(text), Err(ConfigError::Syntax { .. })));
    }

    #[test]
    fn delay_only_inside_sequence() {
        let top = r#"{"version":1,"pages":[{"name":"p","buttons":{
            "7": {"label":"x","action":{"type":"delay","ms":10}}}}]}"#;
        assert!(matches!(parse(top), Err(ConfigError::Invalid(_))));

        let inside = r#"{"version":1,"pages":[{"name":"p","buttons":{
            "7": {"label":"x","action":{"type":"sequence","steps":[
                {"type":"hotkey","keys":["Win","D"]},
                {"type":"delay","ms":200},
                {"type":"text","text":"안녕"}]}}}}]}"#;
        assert!(parse(inside).is_ok());
    }

    #[test]
    fn invalid_values_are_rejected() {
        for action in [
            r#"{"type":"sequence","steps":[]}"#,
            r#"{"type":"sequence","steps":[{"type":"sequence","steps":[{"type":"micMute"}]}]}"#,
            r#"{"type":"sequence","steps":[{"type":"delay","ms":600000}]}"#,
            r#"{"type":"hotkey","keys":[]}"#,
            r#"{"type":"hotkey","keys":["Ctrl","Foo"]}"#,
            r#"{"type":"open","target":"  "}"#,
            r#"{"type":"sound","file":"a.mp3","volume":1.5}"#,
        ] {
            let text = format!(
                r#"{{"version":1,"pages":[{{"name":"p","buttons":{{"7":{{"label":"x","action":{action}}}}}}}]}}"#
            );
            assert!(
                matches!(parse(&text), Err(ConfigError::Invalid(_))),
                "{action}"
            );
        }
    }

    #[test]
    fn wrong_version_and_empty_pages_are_rejected() {
        assert!(matches!(
            parse(r#"{"version":2,"pages":[{"name":"p"}]}"#),
            Err(ConfigError::Invalid(_))
        ));
        assert!(matches!(
            parse(r#"{"version":1,"pages":[]}"#),
            Err(ConfigError::Invalid(_))
        ));
    }

    #[test]
    fn bom_is_accepted() {
        assert!(parse("\u{feff}{\"version\":1,\"pages\":[{\"name\":\"p\"}]}").is_ok());
    }

    #[test]
    fn load_or_create_writes_default_then_reads_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("config.json");
        let created = load_or_create(&path).unwrap();
        assert!(path.exists());
        assert!(!path.with_extension("json.tmp").exists());
        assert_eq!(load(&path).unwrap(), created);
    }

    fn recv_within(
        rx: &mpsc::Receiver<Result<Config, ConfigError>>,
        secs: u64,
    ) -> Option<Result<Config, ConfigError>> {
        rx.recv_timeout(Duration::from_secs(secs)).ok()
    }

    #[test]
    fn watcher_reports_changes_errors_and_skips_identical_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let initial = load_or_create(&path).unwrap();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch_into(&path, initial.clone(), tx).unwrap();
        thread::sleep(Duration::from_millis(300));

        // 같은 내용을 다시 저장하면 알리지 않는다.
        save(&path, &initial).unwrap();
        assert!(recv_within(&rx, 1).is_none());

        // 바뀐 내용은 알린다.
        let mut changed = initial.clone();
        changed.pages[0].name = "바뀐 이름".into();
        save(&path, &changed).unwrap();
        let started = Instant::now();
        let got = recv_within(&rx, 5).expect("변경 알림").unwrap();
        assert_eq!(got.pages[0].name, "바뀐 이름");
        assert!(started.elapsed() < Duration::from_secs(2));

        // 깨진 파일은 오류로 알린다.
        fs::write(&path, "{ 깨진").unwrap();
        assert!(matches!(
            recv_within(&rx, 5),
            Some(Err(ConfigError::Syntax { .. }))
        ));
    }
}
