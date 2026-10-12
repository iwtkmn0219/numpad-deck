//! 프리셋: 한 번에 추가하는 페이지 묶음.
//!
//! 첫 실행 기본 설정, 편집기의 "프리셋에서 페이지 추가", 온보딩이 같은 정의를 쓴다.
//! 이름과 버튼 이름은 한국어·영어를 함께 두고, 페이지를 만들 때 언어를 고른다.
//! 정의는 `presets/*.json`이다 (사람이 읽고 고치기 쉽게).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::{Action, Button, ButtonKey, Config, Page, VERSION};
pub use crate::i18n::Lang;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    ko: String,
    en: String,
}

impl Text {
    fn get(&self, lang: Lang) -> String {
        match lang {
            Lang::Ko => self.ko.clone(),
            Lang::En => self.en.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetButton {
    label: Text,
    icon: Option<String>,
    action: Action,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetFile {
    name: Text,
    description: Text,
    buttons: BTreeMap<ButtonKey, PresetButton>,
}

/// 편집기·온보딩에 보내는 프리셋 하나 (언어를 고른 뒤).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../app/src/bindings/")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 추가할 페이지 (이름 = 프리셋 이름)
    pub page: Page,
}

/// (id, JSON). 보여주는 순서.
const FILES: [(&str, &str); 6] = [
    ("music", include_str!("presets/music.json")),
    ("windows", include_str!("presets/windows.json")),
    ("zoom", include_str!("presets/zoom.json")),
    ("teams", include_str!("presets/teams.json")),
    ("meet", include_str!("presets/meet.json")),
    ("obs", include_str!("presets/obs.json")),
];

/// 첫 실행 때 들어가는 프리셋
pub const DEFAULT_PRESET: &str = "music";

fn load(id: &str, json: &str, lang: Lang) -> Preset {
    let file: PresetFile = serde_json::from_str(json)
        .unwrap_or_else(|e| panic!("내장 프리셋 {id}가 잘못됐습니다: {e}"));
    let name = file.name.get(lang);
    Preset {
        id: id.to_string(),
        description: file.description.get(lang),
        page: Page {
            name: name.clone(),
            buttons: file
                .buttons
                .into_iter()
                .map(|(key, b)| {
                    let button = Button {
                        label: b.label.get(lang),
                        icon: b.icon,
                        action: b.action,
                    };
                    (key, button)
                })
                .collect(),
        },
        name,
    }
}

/// 모든 프리셋 (보여주는 순서).
pub fn all(lang: Lang) -> Vec<Preset> {
    FILES
        .iter()
        .map(|(id, json)| load(id, json, lang))
        .collect()
}

pub fn get(id: &str, lang: Lang) -> Option<Preset> {
    FILES
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(i, json)| load(i, json, lang))
}

/// 첫 실행 기본 설정: 기본 프리셋 한 페이지.
pub fn default_config(lang: Lang) -> Config {
    Config {
        version: VERSION,
        // 언어는 정하지 않는다 (자동 = Windows 표시 언어)
        language: None,
        layout: None,
        system_keys: None,
        mic_indicator: None,
        pages: vec![get(DEFAULT_PRESET, lang).expect("기본 프리셋이 있다").page],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;
    use crate::key::NumpadKey;

    #[test]
    fn every_preset_is_a_valid_page_in_both_languages() {
        for lang in [Lang::Ko, Lang::En] {
            let presets = all(lang);
            assert_eq!(presets.len(), FILES.len());
            for p in presets {
                assert!(!p.page.buttons.is_empty(), "{}", p.id);
                let config = Config {
                    version: VERSION,
                    language: None,
                    layout: None,
                    system_keys: None,
                    mic_indicator: None,
                    pages: vec![p.page.clone()],
                };
                // 저장했다 다시 읽어도 통과해야 한다 (검증 포함)
                let text = serde_json::to_string(&config).unwrap();
                parse(&text).unwrap_or_else(|e| panic!("{} ({lang:?}): {e}", p.id));
                for b in p.page.buttons.values() {
                    assert!(!b.label.is_empty(), "{}", p.id);
                    assert!(b.icon.is_some(), "{}: {}", p.id, b.label);
                }
            }
        }
    }

    #[test]
    fn default_config_is_music_preset() {
        let ko = default_config(Lang::Ko);
        assert_eq!(ko.pages.len(), 1);
        assert_eq!(ko.button(0, NumpadKey::N8).unwrap().label, "재생/정지");
        let en = default_config(Lang::En);
        assert_eq!(en.button(0, NumpadKey::N8).unwrap().label, "Play/Pause");
    }

    #[test]
    fn unknown_preset() {
        assert!(get("nope", Lang::Ko).is_none());
    }
}
