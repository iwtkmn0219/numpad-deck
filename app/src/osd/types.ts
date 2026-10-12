// Rust `app/src-tauri/src/osd.rs`의 `OsdPayload`와 같은 모양이어야 한다.

import type { Button } from "@/bindings/Button";
import type { KeyPlace } from "@/bindings/KeyPlace";
import type { SystemKeys } from "@/bindings/SystemKeys";

export type Mode = "deck" | "numbers" | "paused";

export interface OsdPayload {
  /** 같은 내용을 다시 보내도 강조 애니메이션이 다시 돌도록 매번 늘어난다. */
  seq: number;
  mode: Mode;
  /** 0부터 */
  page: number;
  pageCount: number;
  pageName: string;
  /** 키캡 글자("7", "+", "Enter" …) → 버튼. 빈 자리는 없다. */
  buttons: Partial<Record<string, Button>>;
  /** 방금 누른 키캡 글자 */
  highlight: string | null;
  /** 방금 누른 버튼이 실패했으면 이유 */
  error: string | null;
  /** 마이크 음소거 버튼으로 마지막에 바꾼 상태 (아직 안 눌렀으면 null) */
  micMuted: boolean | null;
  /** 넘패드 모양 (null이면 표준) */
  layout: KeyPlace[] | null;
  /** 시스템 키 역할 (null이면 기본) */
  systemKeys: SystemKeys | null;
}

export const OSD_EVENT = "osd://update";
