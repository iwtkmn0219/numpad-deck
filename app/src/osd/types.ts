// Rust `app/src-tauri/src/osd.rs`의 `OsdPayload`와 같은 모양이어야 한다.

export type Mode = "deck" | "numbers" | "paused";

export interface OsdPayload {
  /** 같은 내용을 다시 보내도 강조 애니메이션이 다시 돌도록 매번 늘어난다. */
  seq: number;
  mode: Mode;
  /** 0부터 */
  page: number;
  pageCount: number;
  pageName: string;
  /** 키캡 글자("7", "+", "Enter" …) → 버튼 이름. 빈 자리는 없다. */
  buttons: Record<string, string>;
  /** 방금 누른 키캡 글자 */
  highlight: string | null;
  /** 방금 누른 버튼이 실패했으면 이유 */
  error: string | null;
}

export const OSD_EVENT = "osd://update";
