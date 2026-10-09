//! 덱 상태(현재 페이지)와 디스패처.
//!
//! 훅이 알린 넘패드 키를 키 배치([`layout`])에 따라 버튼 실행·페이지 전환·OSD 토글로
//! 바꾼다. 실행 자체는 하지 않고 무엇을 할지만 돌려준다. 설정은 소유하지 않고 매번
//! 받는다 (설정이 바뀌어도 현재 페이지는 유지해야 하므로).

use crate::config::{Button, Config};
use crate::key::NumpadKey;
use crate::layout::{self, Role};

#[derive(Debug, Clone, PartialEq)]
pub enum Dispatch {
    /// 버튼을 눌렀다. 빈 자리면 `button`이 `None`이다 (OSD로 빈 자리임을 보여준다).
    Button {
        page: usize,
        key: NumpadKey,
        button: Option<Button>,
    },
    /// 페이지가 바뀌었거나 페이지 키를 눌렀다. 페이지가 하나뿐이어도 OSD를 띄우도록 알린다.
    Page {
        page: usize,
    },
    ToggleOsd,
}

#[derive(Debug, Default)]
pub struct Deck {
    page: usize,
}

impl Deck {
    pub fn new() -> Self {
        Self::default()
    }

    /// 현재 페이지 (0부터).
    pub fn page(&self) -> usize {
        self.page
    }

    pub fn on_press(&mut self, key: NumpadKey, config: &Config) -> Dispatch {
        let count = config.pages.len().max(1);
        match layout::role(key) {
            Role::Button(_) => Dispatch::Button {
                page: self.page,
                key,
                button: config.button(self.page, key).cloned(),
            },
            // 끝에서 넘기면 반대쪽 끝으로 돈다 (프로토타입과 같다).
            Role::PrevPage => self.go((self.page + count - 1) % count),
            Role::NextPage => self.go((self.page + 1) % count),
            Role::FirstPage => self.go(0),
            Role::ToggleOsd => Dispatch::ToggleOsd,
        }
    }

    /// 설정이 바뀌어 페이지 수가 줄었으면 마지막 페이지로 옮긴다. 옮겼으면 알린다.
    pub fn on_config_changed(&mut self, config: &Config) -> Option<Dispatch> {
        let last = config.pages.len().saturating_sub(1);
        (self.page > last).then(|| self.go(last))
    }

    fn go(&mut self, page: usize) -> Dispatch {
        self.page = page;
        Dispatch::Page { page }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse;

    fn config(pages: usize) -> Config {
        let pages: Vec<String> = (0..pages)
            .map(|i| {
                format!(
                    r#"{{"name":"p{i}","buttons":{{"7":{{"label":"b{i}","action":{{"type":"micMute"}}}}}}}}"#
                )
            })
            .collect();
        parse(&format!(r#"{{"version":1,"pages":[{}]}}"#, pages.join(","))).unwrap()
    }

    fn label(d: &Dispatch) -> Option<&str> {
        match d {
            Dispatch::Button { button, .. } => button.as_ref().map(|b| b.label.as_str()),
            _ => None,
        }
    }

    #[test]
    fn buttons_come_from_current_page() {
        let c = config(3);
        let mut deck = Deck::new();
        assert_eq!(label(&deck.on_press(NumpadKey::N7, &c)), Some("b0"));
        deck.on_press(NumpadKey::Multiply, &c);
        assert_eq!(label(&deck.on_press(NumpadKey::N7, &c)), Some("b1"));
    }

    #[test]
    fn empty_slot_is_reported() {
        let c = config(1);
        let mut deck = Deck::new();
        assert_eq!(
            deck.on_press(NumpadKey::N8, &c),
            Dispatch::Button {
                page: 0,
                key: NumpadKey::N8,
                button: None
            }
        );
    }

    #[test]
    fn page_keys_wrap_around() {
        let c = config(3);
        let mut deck = Deck::new();
        assert_eq!(
            deck.on_press(NumpadKey::Divide, &c),
            Dispatch::Page { page: 2 }
        );
        assert_eq!(
            deck.on_press(NumpadKey::Multiply, &c),
            Dispatch::Page { page: 0 }
        );
        assert_eq!(
            deck.on_press(NumpadKey::Multiply, &c),
            Dispatch::Page { page: 1 }
        );
        assert_eq!(
            deck.on_press(NumpadKey::Decimal, &c),
            Dispatch::Page { page: 0 }
        );
    }

    #[test]
    fn single_page_still_reports_page_for_osd() {
        let c = config(1);
        let mut deck = Deck::new();
        assert_eq!(
            deck.on_press(NumpadKey::Multiply, &c),
            Dispatch::Page { page: 0 }
        );
        assert_eq!(
            deck.on_press(NumpadKey::Divide, &c),
            Dispatch::Page { page: 0 }
        );
    }

    #[test]
    fn osd_key() {
        let c = config(1);
        assert_eq!(
            Deck::new().on_press(NumpadKey::Subtract, &c),
            Dispatch::ToggleOsd
        );
    }

    #[test]
    fn shrinking_config_clamps_page() {
        let mut deck = Deck::new();
        let three = config(3);
        deck.on_press(NumpadKey::Divide, &three); // 마지막 페이지(2)
        assert_eq!(deck.on_config_changed(&three), None);
        assert_eq!(
            deck.on_config_changed(&config(2)),
            Some(Dispatch::Page { page: 1 })
        );
        assert_eq!(deck.page(), 1);
    }
}
