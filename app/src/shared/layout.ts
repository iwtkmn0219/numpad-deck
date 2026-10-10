// 넘패드 모양 배치. OSD와 편집기가 같이 쓴다. `core/src/layout.rs`의 키 배치와 맞아야 한다.
// 4열 × 5행, +와 Enter는 두 줄, 0은 두 칸.

import type { ButtonKeyName } from "@/bindings/ButtonKeyName";

export type SystemKey = "/" | "*" | "-" | ".";

export type Slot =
  | { kind: "button"; key: ButtonKeyName; area: string }
  | { kind: "system"; key: SystemKey; area: string }
  | { kind: "mode"; area: string };

export const SLOTS: Slot[] = [
  { kind: "mode", area: "1 / 1 / 2 / 2" },
  { kind: "system", key: "/", area: "1 / 2 / 2 / 3" },
  { kind: "system", key: "*", area: "1 / 3 / 2 / 4" },
  { kind: "system", key: "-", area: "1 / 4 / 2 / 5" },
  { kind: "button", key: "7", area: "2 / 1 / 3 / 2" },
  { kind: "button", key: "8", area: "2 / 2 / 3 / 3" },
  { kind: "button", key: "9", area: "2 / 3 / 3 / 4" },
  { kind: "button", key: "+", area: "2 / 4 / 4 / 5" },
  { kind: "button", key: "4", area: "3 / 1 / 4 / 2" },
  { kind: "button", key: "5", area: "3 / 2 / 4 / 3" },
  { kind: "button", key: "6", area: "3 / 3 / 4 / 4" },
  { kind: "button", key: "1", area: "4 / 1 / 5 / 2" },
  { kind: "button", key: "2", area: "4 / 2 / 5 / 3" },
  { kind: "button", key: "3", area: "4 / 3 / 5 / 4" },
  { kind: "button", key: "Enter", area: "4 / 4 / 6 / 5" },
  { kind: "button", key: "0", area: "5 / 1 / 6 / 3" },
  { kind: "system", key: ".", area: "5 / 3 / 6 / 4" },
];

/** 버튼 키 순서 (위에서 아래, 왼쪽에서 오른쪽). `layout.rs`의 `BUTTON_KEYS`와 같다. */
export const BUTTON_KEYS: ButtonKeyName[] = ["7", "8", "9", "+", "4", "5", "6", "1", "2", "3", "Enter", "0"];
