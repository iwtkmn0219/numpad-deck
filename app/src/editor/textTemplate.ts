// 문장 버튼 자리표시자(`{날짜}`, `{시간}`, `{클립보드}` …)의 미리 보기용 펼치기.
//
// 코어 `core/src/text_template.rs`의 `expand`·`has_placeholders`와 같은 규칙이어야 한다.
// 앱에 TS 시험 도구가 없어서, 코어 시험 사례를 아래 표로 옮겨 두고 리뷰에서 대조한다.
// (미리 보기는 클립보드를 읽지 않는다: 부르는 쪽이 넘기는 `clipboardLabel`로 대신 보인다.
//  그래서 코어의 클립보드 시험 — 한 번만 읽기, CRLF→LF, 글자 없음 오류 — 은 여기에 해당 없음.)
//
// 규칙
// - 두 언어 이름을 모두 알아듣는다. 출력 언어는 앱 언어(`lang`)를 따르고 이름의 언어와 무관하다.
// - 이름은 정확히 맞아야 한다(공백·대소문자 허용 안 함, 영어 이름은 소문자만).
// - 표에 없는 `{...}`와 짝 없는 `{`·`}`는 그대로 둔다.
// - `{{` + 표의 이름 + `}}` 한 덩어리만 `{이름}` 글자로 줄어든다(펼치지 않는다). 다른 `{{`·`}}`는 그대로다.
//   그래서 표의 이름이 없는 문장은 한 글자도 바뀌지 않는다(코어 시험 `text_without_table_names_is_unchanged`).
// - `{`에서 왼쪽부터 본다: ① `{{이름}}` 줄이기 ② `{이름}` 펼치기 ③ 아니면 `{`만 글자로 남기고 다음 글자부터 다시 본다.
//
// 시험 사례 (SAT = 2026-10-17 토요일 14:05, WED = 2026-03-04 수요일 09:07, 요일 0=일요일)
//   입력                         시각  앱 언어  출력
//   {날짜}                       SAT   ko       2026-10-17
//   {date}                       SAT   en       2026-10-17
//   {날짜}                       WED   ko       2026-03-04
//   {날짜:긴}                    SAT   ko       2026년 10월 17일
//   {date:long}                  SAT   en       October 17, 2026
//   {날짜:긴}                    WED   ko       2026년 3월 4일
//   {date:long}                  WED   en       March 4, 2026
//   {날짜:짧은}                  SAT   ko       10/17
//   {date:short}                 SAT   en       10/17
//   {날짜:짧은}                  WED   ko       3/4
//   {시간}                       SAT   ko       14:05      (24시간, 0 채움)
//   {time}                       WED   en       09:07
//   {요일}                       SAT   ko       토요일
//   {weekday}                    SAT   en       Saturday
//   {요일}                       WED   ko       수요일
//   {weekday}                    WED   en       Wednesday
//   {요일}  (요일=0)             SAT   ko       일요일
//   {요일}                       SAT   en       Saturday   (이름 언어와 출력 언어는 별개)
//   {weekday}                    SAT   ko       토요일
//   {날짜:긴}                    SAT   en       October 17, 2026
//   {{날짜}}                     SAT   ko       {날짜}
//   {{date:long}}                SAT   en       {date:long}
//   a {{시간}} b {시간}          SAT   ko       a {시간} b 14:05
//   다음 입력은 각각 따옴표 안의 글자 그대로 나온다 (SAT, ko):
//   "{{"  "}}"  "}}}"  "a}}b"  "{{x}}"  "{{ 날짜}}"  "{\"a\":{\"b\":1}}"
//   {{날짜}                      SAT   ko       {2026-10-17
//   {날짜}}                      SAT   ko       2026-10-17}
//   {{{시간}}                    SAT   ko       {{시간}
//   {{{날짜}}}                   SAT   ko       {{날짜}}   (왼쪽 `{` 글자, 안쪽 `{{날짜}}`가 `{날짜}`, 오른쪽 `}` 글자)
//   {^_^} {이름} {Date} { 날짜} {날짜   SAT  ko  입력 그대로 (모르는 이름·짝 없는 괄호)
//   { {날짜}                     SAT   ko       { 2026-10-17
//   {a{시간}                     SAT   ko       {a14:05
//   안녕 {날짜} {시간}!          SAT   ko       안녕 2026-10-17 14:05!
//   hasPlaceholders: {시간} 참 · {날짜} 참 · a {clipboard} b 참 · {{날짜}} {날짜} 참 · {{시간}} 거짓 · {{x}} 거짓 · {x} 거짓 · {{{날짜}}} 거짓 · (빈 문장) 거짓

export type PlaceholderId = "date" | "dateLong" | "dateShort" | "time" | "weekday" | "clipboard";

/** 끼워 넣기 메뉴 순서. 이름은 현재 앱 언어 것을 넣는다. */
export const PLACEHOLDERS: { id: PlaceholderId; ko: string; en: string }[] = [
  { id: "date", ko: "{날짜}", en: "{date}" },
  { id: "dateLong", ko: "{날짜:긴}", en: "{date:long}" },
  { id: "dateShort", ko: "{날짜:짧은}", en: "{date:short}" },
  { id: "time", ko: "{시간}", en: "{time}" },
  { id: "weekday", ko: "{요일}", en: "{weekday}" },
  { id: "clipboard", ko: "{클립보드}", en: "{clipboard}" },
];

const NAMES: Record<string, PlaceholderId> = {
  날짜: "date",
  date: "date",
  "날짜:긴": "dateLong",
  "date:long": "dateLong",
  "날짜:짧은": "dateShort",
  "date:short": "dateShort",
  시간: "time",
  time: "time",
  요일: "weekday",
  weekday: "weekday",
  클립보드: "clipboard",
  clipboard: "clipboard",
};

const MONTHS_EN = [
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
const WEEKDAYS_EN = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const WEEKDAYS_KO = ["일", "월", "화", "수", "목", "금", "토"];

type Part = { text: string } | { place: PlaceholderId };

function lookup(name: string): PlaceholderId | undefined {
  // `NAMES["constructor"]` 같은 객체 기본 속성에 걸리지 않게 자기 속성만 본다
  return Object.hasOwn(NAMES, name) ? NAMES[name] : undefined;
}

/** 한 번 훑으며 글자와 자리표시자로 나눈다 (코어 `parse`와 같다). */
function parse(text: string): Part[] {
  const parts: Part[] = [];
  let buf = "";
  let i = 0;
  while (i < text.length) {
    if (text[i] !== "{") {
      // 문자 단위가 아니라 UTF-16 단위로 옮겨도 된다: `{`·`}`는 서로게이트 쌍과 겹치지 않는다
      buf += text[i];
      i += 1;
      continue;
    }
    // ① `{{이름}}`
    if (text.startsWith("{{", i)) {
      const end = text.indexOf("}", i + 2);
      if (end >= 0 && text.startsWith("}}", end) && lookup(text.slice(i + 2, end)) !== undefined) {
        buf += `{${text.slice(i + 2, end)}}`;
        i = end + 2;
        continue;
      }
    }
    // ② `{이름}`
    const end = text.indexOf("}", i);
    const place = end < 0 ? undefined : lookup(text.slice(i + 1, end));
    if (place !== undefined) {
      if (buf) parts.push({ text: buf });
      buf = "";
      parts.push({ place });
      i = end + 1;
    } else {
      // ③ 그대로
      buf += "{";
      i += 1;
    }
  }
  if (buf) parts.push({ text: buf });
  return parts;
}

/** 표의 이름이 `{이름}` 꼴로 하나라도 있으면 true. `{{이름}}`으로 막은 것은 세지 않는다. */
export function hasPlaceholders(text: string): boolean {
  return parse(text).some((p) => "place" in p);
}

const pad2 = (n: number) => String(n).padStart(2, "0");

function render(p: PlaceholderId, now: Date, lang: "ko" | "en", clipboardLabel: string): string {
  const y = String(now.getFullYear()).padStart(4, "0");
  const m = now.getMonth() + 1;
  const d = now.getDate();
  switch (p) {
    case "date":
      return `${y}-${pad2(m)}-${pad2(d)}`;
    case "dateLong":
      return lang === "ko" ? `${now.getFullYear()}년 ${m}월 ${d}일` : `${MONTHS_EN[m - 1]} ${d}, ${now.getFullYear()}`;
    case "dateShort":
      return `${m}/${d}`;
    case "time":
      return `${pad2(now.getHours())}:${pad2(now.getMinutes())}`;
    case "weekday":
      return lang === "ko" ? `${WEEKDAYS_KO[now.getDay()]}요일` : WEEKDAYS_EN[now.getDay()];
    case "clipboard":
      return clipboardLabel;
  }
}

/** 미리 보기 글. `{클립보드}`는 `clipboardLabel`로 보이고, 클립보드는 읽지 않는다. */
export function expandPreview(text: string, now: Date, lang: "ko" | "en", clipboardLabel: string): string {
  return parse(text)
    .map((p) => ("text" in p ? p.text : render(p.place, now, lang, clipboardLabel)))
    .join("");
}
