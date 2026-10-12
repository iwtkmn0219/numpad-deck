// 넘패드 모양 배치. OSD와 편집기가 같이 쓴다.
// 기본은 표준 넘패드(4열 × 5행, +와 Enter는 두 줄, 0은 두 칸). 설정의 `layout`이 있으면 그 모양으로
// 그린다 (노트북 등 모양이 다른 키보드에 맞추기). 키의 역할은 `core/src/layout.rs`가 정하고,
// 여기서는 그리는 자리만 다룬다.

import type { ButtonKeyName } from "@/bindings/ButtonKeyName";
import type { KeyPlace } from "@/bindings/KeyPlace";
import type { LayoutKeyName } from "@/bindings/LayoutKeyName";

export type SystemKey = "/" | "*" | "-" | ".";

export type Slot =
  | { kind: "button"; key: ButtonKeyName; area: string }
  | { kind: "system"; key: SystemKey; area: string }
  | { kind: "mode"; area: string };

/** 배치 칸의 가로·세로 최대 수 (코어 `LAYOUT_MAX`와 같다) */
export const LAYOUT_MAX = 8;

const place = (key: LayoutKeyName, x: number, y: number, w = 1, h = 1): KeyPlace => ({ key, x, y, w, h });

/** 표준 넘패드 */
export const STANDARD_LAYOUT: KeyPlace[] = [
  place("NumLock", 0, 0),
  place("/", 1, 0),
  place("*", 2, 0),
  place("-", 3, 0),
  place("7", 0, 1),
  place("8", 1, 1),
  place("9", 2, 1),
  place("+", 3, 1, 1, 2),
  place("4", 0, 2),
  place("5", 1, 2),
  place("6", 2, 2),
  place("1", 0, 3),
  place("2", 1, 3),
  place("3", 2, 3),
  place("Enter", 3, 3, 1, 2),
  place("0", 0, 4, 2, 1),
  place(".", 2, 4),
];

const SYSTEM_KEYS: string[] = ["/", "*", "-", "."];

export interface Grid {
  slots: Slot[];
  cols: number;
  rows: number;
}

/** 배치(없으면 표준) → 그릴 칸. 빈 테두리는 잘라 왼쪽 위에 붙인다. */
export function gridFor(layout?: KeyPlace[] | null): Grid {
  const places = layout ?? STANDARD_LAYOUT;
  const minX = Math.min(...places.map((p) => p.x));
  const minY = Math.min(...places.map((p) => p.y));
  const cols = Math.max(...places.map((p) => p.x + p.w)) - minX;
  const rows = Math.max(...places.map((p) => p.y + p.h)) - minY;
  const slots = places.map((p): Slot => {
    const area = `${p.y - minY + 1} / ${p.x - minX + 1} / ${p.y - minY + p.h + 1} / ${p.x - minX + p.w + 1}`;
    if (p.key === "NumLock") return { kind: "mode", area };
    if (SYSTEM_KEYS.includes(p.key)) return { kind: "system", key: p.key as SystemKey, area };
    return { kind: "button", key: p.key as ButtonKeyName, area };
  });
  return { slots, cols, rows };
}

/**
 * 그리드 컨테이너 스타일: 열은 같은 너비(바깥 너비는 그대로), 행은 `rowHeight` px.
 * 표준(5행)보다 행이 많으면 전체 높이가 같도록 줄인다.
 */
export function gridStyle(grid: Grid, rowHeight: number): React.CSSProperties {
  const h = grid.rows <= 5 ? rowHeight : Math.floor((rowHeight * 5) / grid.rows);
  return {
    display: "grid",
    gridTemplateColumns: `repeat(${grid.cols}, minmax(0, 1fr))`,
    gridTemplateRows: `repeat(${grid.rows}, ${h}px)`,
  };
}

/** 표준 배치와 같은가 (같으면 설정에 적지 않는다) */
export function isStandard(layout: KeyPlace[]): boolean {
  return (
    layout.length === STANDARD_LAYOUT.length &&
    STANDARD_LAYOUT.every((s) => {
      const p = layout.find((l) => l.key === s.key);
      return p && p.x === s.x && p.y === s.y && p.w === s.w && p.h === s.h;
    })
  );
}

/** 버튼 키 순서 (위에서 아래, 왼쪽에서 오른쪽). `layout.rs`의 `BUTTON_KEYS`와 같다. */
export const BUTTON_KEYS: ButtonKeyName[] = ["7", "8", "9", "+", "4", "5", "6", "1", "2", "3", "Enter", "0"];
