//! 문장 액션의 자리표시자(`{날짜}`, `{시간}`, `{클립보드}` …) 펼치기.
//!
//! 버튼을 누르는 순간의 값으로 바꾼다. 두 언어 이름을 모두 알아듣고, 출력 언어는 앱 언어를 따른다.
//! 표에 없는 `{...}`와 짝 없는 괄호는 그대로 둔다. 이름을 글자 그대로 쓰려면 `{{날짜}}`처럼 겹쳐 쓴다
//! (`{{` + 표의 이름 + `}}` 한 덩어리만 `{날짜}`로 줄고, 다른 `{{`·`}}`는 그대로다).
//! 그래서 표의 이름이 없는 기존 문장은 한 글자도 바뀌지 않는다.
//! Windows 의존 없음: 현재 시각과 클립보드는 부르는 쪽이 넘긴다.

use crate::i18n::Lang;

/// 현지 시각 한 순간. `weekday`는 0=일요일(`SYSTEMTIME.wDayOfWeek`와 같다).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub weekday: u8,
}

#[derive(Debug, PartialEq)]
pub enum TemplateError {
    /// `{클립보드}`가 있는데 클립보드에 글자가 없다.
    NoClipboardText,
}

const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAYS_EN: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

const WEEKDAYS_KO: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placeholder {
    Date,
    DateLong,
    DateShort,
    Time,
    Weekday,
    Clipboard,
}

/// `{`와 `}` 사이 이름이 표와 정확히 같을 때만 자리표시자다.
fn lookup(name: &str) -> Option<Placeholder> {
    Some(match name {
        "날짜" | "date" => Placeholder::Date,
        "날짜:긴" | "date:long" => Placeholder::DateLong,
        "날짜:짧은" | "date:short" => Placeholder::DateShort,
        "시간" | "time" => Placeholder::Time,
        "요일" | "weekday" => Placeholder::Weekday,
        "클립보드" | "clipboard" => Placeholder::Clipboard,
        _ => return None,
    })
}

enum Part {
    Text(String),
    Place(Placeholder),
}

/// 한 번 훑으며 글자와 자리표시자로 나눈다.
///
/// 글자 하나씩 보며 `{`에서만 다음을 본다(가장 왼쪽부터, 앞의 것이 먼저).
/// 1. `{{` + 표의 이름 + `}}` 한 덩어리: `{이름}` 글자로 줄인다(펼치지 않는다).
/// 2. `{` + 표의 이름 + `}`: 자리표시자.
/// 3. 그 밖: `{` 글자 하나만 그대로 두고 다음 글자부터 다시 본다.
///
/// `}`와 1번 밖의 `{{`·`}}`는 건드리지 않는다. 그래서 표의 이름이 없는 문장은 한 글자도 바뀌지 않는다.
fn parse(text: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c != '{' {
            buf.push(c);
            rest = &rest[c.len_utf8()..];
            continue;
        }
        // 1. `{{이름}}`
        if let Some(inner) = rest.strip_prefix("{{") {
            if let Some(end) = inner.find('}') {
                if inner[end..].starts_with("}}") && lookup(&inner[..end]).is_some() {
                    buf.push('{');
                    buf.push_str(&inner[..end]);
                    buf.push('}');
                    rest = &inner[end + 2..];
                    continue;
                }
            }
        }
        // 2. `{이름}`
        let found = rest
            .find('}')
            .and_then(|end| lookup(&rest[1..end]).map(|p| (p, end)));
        match found {
            Some((p, end)) => {
                if !buf.is_empty() {
                    parts.push(Part::Text(std::mem::take(&mut buf)));
                }
                parts.push(Part::Place(p));
                rest = &rest[end + 1..];
            }
            // 3. 그대로
            None => {
                buf.push('{');
                rest = &rest[1..];
            }
        }
    }
    if !buf.is_empty() {
        parts.push(Part::Text(buf));
    }
    parts
}

/// 표의 이름이 `{이름}` 꼴로 하나라도 있으면 true. `{{이름}}`으로 막은 것은 세지 않는다.
pub fn has_placeholders(text: &str) -> bool {
    parse(text).iter().any(|p| matches!(p, Part::Place(_)))
}

fn render(p: Placeholder, now: &Moment, lang: Lang, clipboard: &str) -> String {
    match p {
        Placeholder::Date => format!("{:04}-{:02}-{:02}", now.year, now.month, now.day),
        Placeholder::DateLong => match lang {
            Lang::Ko => format!("{}년 {}월 {}일", now.year, now.month, now.day),
            Lang::En => {
                let m = MONTHS_EN[usize::from(now.month.saturating_sub(1)) % 12];
                format!("{} {}, {}", m, now.day, now.year)
            }
        },
        Placeholder::DateShort => format!("{}/{}", now.month, now.day),
        Placeholder::Time => format!("{:02}:{:02}", now.hour, now.minute),
        Placeholder::Weekday => {
            let i = usize::from(now.weekday) % 7;
            match lang {
                Lang::Ko => format!("{}요일", WEEKDAYS_KO[i]),
                Lang::En => WEEKDAYS_EN[i].to_string(),
            }
        }
        Placeholder::Clipboard => clipboard.to_string(),
    }
}

/// 자리표시자를 지금 값으로 바꾼 문장. `clipboard`는 `{클립보드}`가 있을 때만, 한 번만 부른다.
/// 클립보드 글자는 줄바꿈 `CR LF`를 `LF`로 바꿔 넣는다. 글자가 없으면 `NoClipboardText`.
pub fn expand(
    text: &str,
    now: &Moment,
    lang: Lang,
    clipboard: impl FnOnce() -> Option<String>,
) -> Result<String, TemplateError> {
    let parts = parse(text);
    let needs_clip = parts
        .iter()
        .any(|p| matches!(p, Part::Place(Placeholder::Clipboard)));
    let clip = if needs_clip {
        clipboard()
            .map(|s| s.replace("\r\n", "\n"))
            .ok_or(TemplateError::NoClipboardText)?
    } else {
        String::new()
    };
    let mut out = String::with_capacity(text.len());
    for part in &parts {
        match part {
            Part::Text(s) => out.push_str(s),
            Part::Place(p) => out.push_str(&render(*p, now, lang, &clip)),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    /// 2026-10-17 토요일 14:05
    const SAT: Moment = Moment {
        year: 2026,
        month: 10,
        day: 17,
        hour: 14,
        minute: 5,
        weekday: 6,
    };
    /// 2026-03-04 수요일 09:07
    const WED: Moment = Moment {
        year: 2026,
        month: 3,
        day: 4,
        hour: 9,
        minute: 7,
        weekday: 3,
    };

    fn ex(text: &str, now: &Moment, lang: Lang) -> String {
        expand(text, now, lang, || None).unwrap()
    }

    #[test]
    fn date_plain() {
        assert_eq!(ex("{날짜}", &SAT, Lang::Ko), "2026-10-17");
        assert_eq!(ex("{date}", &SAT, Lang::En), "2026-10-17");
        assert_eq!(ex("{날짜}", &WED, Lang::Ko), "2026-03-04");
    }

    #[test]
    fn date_long() {
        assert_eq!(ex("{날짜:긴}", &SAT, Lang::Ko), "2026년 10월 17일");
        assert_eq!(ex("{date:long}", &SAT, Lang::En), "October 17, 2026");
        assert_eq!(ex("{날짜:긴}", &WED, Lang::Ko), "2026년 3월 4일");
        assert_eq!(ex("{date:long}", &WED, Lang::En), "March 4, 2026");
    }

    #[test]
    fn date_short() {
        assert_eq!(ex("{날짜:짧은}", &SAT, Lang::Ko), "10/17");
        assert_eq!(ex("{date:short}", &SAT, Lang::En), "10/17");
        assert_eq!(ex("{날짜:짧은}", &WED, Lang::Ko), "3/4");
    }

    #[test]
    fn time_is_24h_zero_padded() {
        assert_eq!(ex("{시간}", &SAT, Lang::Ko), "14:05");
        assert_eq!(ex("{time}", &WED, Lang::En), "09:07");
    }

    #[test]
    fn weekday_names() {
        assert_eq!(ex("{요일}", &SAT, Lang::Ko), "토요일");
        assert_eq!(ex("{weekday}", &SAT, Lang::En), "Saturday");
        assert_eq!(ex("{요일}", &WED, Lang::Ko), "수요일");
        assert_eq!(ex("{weekday}", &WED, Lang::En), "Wednesday");
        let sun = Moment { weekday: 0, ..SAT };
        assert_eq!(ex("{요일}", &sun, Lang::Ko), "일요일");
    }

    #[test]
    fn output_language_follows_app_language_not_name() {
        assert_eq!(ex("{요일}", &SAT, Lang::En), "Saturday");
        assert_eq!(ex("{weekday}", &SAT, Lang::Ko), "토요일");
        assert_eq!(ex("{날짜:긴}", &SAT, Lang::En), "October 17, 2026");
    }

    #[test]
    fn escaped_placeholder_names_collapse() {
        // `{{` + 표의 이름 + `}}` 한 덩어리만 `{이름}` 글자로 줄어든다
        assert_eq!(ex("{{날짜}}", &SAT, Lang::Ko), "{날짜}");
        assert_eq!(ex("{{date:long}}", &SAT, Lang::En), "{date:long}");
        assert_eq!(
            ex("a {{시간}} b {시간}", &SAT, Lang::Ko),
            "a {시간} b 14:05"
        );
    }

    #[test]
    fn other_double_braces_stay_literal() {
        for s in [
            "{{",
            "}}",
            "}}}",
            "a}}b",
            "{{x}}",
            "{{ 날짜}}",
            "{\"a\":{\"b\":1}}",
        ] {
            assert_eq!(ex(s, &SAT, Lang::Ko), s, "{s}");
        }
    }

    #[test]
    fn single_placeholder_inside_unmatched_double_braces() {
        assert_eq!(ex("{{날짜}", &SAT, Lang::Ko), "{2026-10-17");
        assert_eq!(ex("{날짜}}", &SAT, Lang::Ko), "2026-10-17}");
        assert_eq!(ex("{{{시간}}", &SAT, Lang::Ko), "{{시간}");
    }

    /// `{{{날짜}}}`: 왼쪽 `{`는 짝이 없어 글자로 남고, 안쪽 `{{날짜}}`가 `{날짜}`로 줄고,
    /// 오른쪽 `}`도 글자로 남는다. 그래서 `{{날짜}}` 그대로다(펼치지 않는다).
    #[test]
    fn triple_braces_pinned() {
        assert_eq!(ex("{{{날짜}}}", &SAT, Lang::Ko), "{{날짜}}");
        assert!(!has_placeholders("{{{날짜}}}"));
    }

    /// 표의 이름이 하나도 없는 문장(`{이름}`·`{{이름}}` 두 꼴 모두)은 한 글자도 바뀌지 않는다.
    #[test]
    fn text_without_table_names_is_unchanged() {
        let cases = [
            "",
            "plain text",
            "{\"a\":{\"b\":1}}",
            "{\"a\": [1, 2, {\"b\": {\"c\": 3}}]}",
            "a}}b",
            "{{x}}",
            "{{",
            "}}",
            "}}}",
            "{{{",
            "{",
            "}",
            "{}",
            "{{}}",
            "{^_^}",
            "{이름}",
            "{ 날짜}",
            "{날짜 }",
            "{Date}",
            "{DATE}",
            "{TIME}",
            "{날짜:}",
            "{날짜",
            "날짜}",
            "{{ date }}",
            "function f() { return {a: 1}; }",
            "한글 {{이름}} 😀 {x} }}😀{{ a
b",
            "😀{😀}😀}}{{😀",
        ];
        for s in cases {
            assert_eq!(ex(s, &SAT, Lang::Ko), s, "ko {s:?}");
            assert_eq!(ex(s, &WED, Lang::En), s, "en {s:?}");
            assert!(!has_placeholders(s), "{s:?}");
        }
    }

    #[test]
    fn unknown_names_and_unpaired_braces_stay() {
        let s = "{^_^} {이름} {Date} { 날짜} {날짜";
        assert_eq!(ex(s, &SAT, Lang::Ko), s);
    }

    #[test]
    fn placeholder_after_stray_brace_still_works() {
        assert_eq!(ex("{ {날짜}", &SAT, Lang::Ko), "{ 2026-10-17");
        assert_eq!(ex("{a{시간}", &SAT, Lang::Ko), "{a14:05");
    }

    #[test]
    fn mixed_text() {
        assert_eq!(
            ex("안녕 {날짜} {시간}!", &SAT, Lang::Ko),
            "안녕 2026-10-17 14:05!"
        );
    }

    #[test]
    fn clipboard_read_once() {
        let calls = Cell::new(0);
        let out = expand("{클립보드}-{clipboard}", &SAT, Lang::Ko, || {
            calls.set(calls.get() + 1);
            Some("x".into())
        })
        .unwrap();
        assert_eq!(out, "x-x");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn clipboard_crlf_becomes_lf() {
        let out = expand("{클립보드}", &SAT, Lang::Ko, || Some("a\r\nb😀".into())).unwrap();
        assert_eq!(out, "a\nb😀");
    }

    #[test]
    fn clipboard_missing_is_error() {
        let r = expand("{clipboard}", &SAT, Lang::En, || None);
        assert_eq!(r, Err(TemplateError::NoClipboardText));
    }

    #[test]
    fn clipboard_not_read_without_placeholder() {
        let calls = Cell::new(0);
        let out = expand("{{클립보드}} {시간}", &SAT, Lang::Ko, || {
            calls.set(calls.get() + 1);
            None
        })
        .unwrap();
        assert_eq!(out, "{클립보드} 14:05");
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn has_placeholders_cases() {
        assert!(has_placeholders("{시간}"));
        assert!(has_placeholders("a {clipboard} b"));
        assert!(!has_placeholders("{{시간}}"));
        assert!(!has_placeholders("{{x}}"));
        assert!(has_placeholders("{날짜}"));
        assert!(has_placeholders("{{날짜}} {날짜}"));
        assert!(!has_placeholders("{x}"));
        assert!(!has_placeholders(""));
    }
}
