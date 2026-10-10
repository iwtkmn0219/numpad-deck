// 편집기 화면이 받은 키(KeyboardEvent.code = 물리 키 위치) → Windows 가상 키.
//
// 편집기 창이 앞에 있으면 키 다운이 저수준 키보드 훅을 거치지 않아서(3-2 실측), 단축키
// 녹화와 넘패드 덱 키는 화면이 받은 키로 처리한다. `key`나 `keyCode`가 아니라 `code`를
// 쓰는 이유: 한글 입력 모드에서는 keyCode가 229("Process")로 와서 어떤 키인지 알 수 없다.
//
// `extended`는 코어 `key::identify`가 넘패드 키와 전용 키를 가르는 데 쓰는 확장 키 표시다.

export interface RawKey {
  vk: number;
  extended: boolean;
}

const TABLE: Record<string, [number, boolean]> = {
  ControlLeft: [0xa2, false],
  ControlRight: [0xa3, true],
  ShiftLeft: [0xa0, false],
  ShiftRight: [0xa1, false],
  AltLeft: [0xa4, false],
  AltRight: [0xa5, true],
  MetaLeft: [0x5b, true],
  MetaRight: [0x5c, true],
  Escape: [0x1b, false],
  Tab: [0x09, false],
  Enter: [0x0d, false],
  Space: [0x20, false],
  Backspace: [0x08, false],
  Delete: [0x2e, true],
  Insert: [0x2d, true],
  Home: [0x24, true],
  End: [0x23, true],
  PageUp: [0x21, true],
  PageDown: [0x22, true],
  ArrowLeft: [0x25, true],
  ArrowUp: [0x26, true],
  ArrowRight: [0x27, true],
  ArrowDown: [0x28, true],
  PrintScreen: [0x2c, true],
  Pause: [0x13, false],
  CapsLock: [0x14, false],
  ScrollLock: [0x91, false],
  ContextMenu: [0x5d, true],
  Lang1: [0x15, false], // 한/영
  Lang2: [0x19, false], // 한자
  Semicolon: [0xba, false],
  Equal: [0xbb, false],
  Comma: [0xbc, false],
  Minus: [0xbd, false],
  Period: [0xbe, false],
  Slash: [0xbf, false],
  Backquote: [0xc0, false],
  BracketLeft: [0xdb, false],
  Backslash: [0xdc, false],
  BracketRight: [0xdd, false],
  Quote: [0xde, false],
  // 넘패드: 코어가 넘패드 키로 알아보고 녹화를 거부한다
  NumpadDecimal: [0x6e, false],
  NumpadDivide: [0x6f, true],
  NumpadMultiply: [0x6a, false],
  NumpadSubtract: [0x6d, false],
  NumpadAdd: [0x6b, false],
  NumpadEnter: [0x0d, true],
  NumLock: [0x90, true],
};

/** 알 수 없는 키는 vk 0 (코어가 "녹화할 수 없는 키"로 거부한다). */
export function codeToVk(code: string): RawKey {
  const fixed = TABLE[code];
  if (fixed) return { vk: fixed[0], extended: fixed[1] };
  let m = /^Key([A-Z])$/.exec(code);
  if (m) return { vk: m[1].charCodeAt(0), extended: false };
  m = /^Digit([0-9])$/.exec(code);
  if (m) return { vk: 0x30 + Number(m[1]), extended: false };
  m = /^F([0-9]{1,2})$/.exec(code);
  if (m && Number(m[1]) >= 1 && Number(m[1]) <= 24) return { vk: 0x70 + Number(m[1]) - 1, extended: false };
  m = /^Numpad([0-9])$/.exec(code);
  if (m) return { vk: 0x60 + Number(m[1]), extended: false };
  return { vk: 0, extended: false };
}

const NUMPAD_LABELS: Record<string, string> = {
  NumpadDecimal: ".",
  NumpadDivide: "/",
  NumpadMultiply: "*",
  NumpadSubtract: "-",
  NumpadAdd: "+",
  NumpadEnter: "Enter",
};

/** 넘패드 키면 키캡 글자(코어 `NumpadKey::label`과 같음), 아니면 null. */
export function numpadLabel(code: string): string | null {
  const m = /^Numpad([0-9])$/.exec(code);
  if (m) return m[1];
  return NUMPAD_LABELS[code] ?? null;
}
