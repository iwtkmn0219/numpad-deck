import {
  AudioLines,
  Clock,
  Globe,
  Keyboard,
  ListOrdered,
  MicOff,
  Music,
  SquareMousePointer,
  Type,
  VolumeX,
  type LucideIcon,
} from "lucide-react";
import type { Action } from "@/bindings/Action";
import type { Dict } from "@/i18n";

export type Kind = Action["type"];

/** 카드에 보이는 순서 */
export const KINDS: Kind[] = [
  "hotkey",
  "launch",
  "open",
  "text",
  "media",
  "micMute",
  "sound",
  "stopSounds",
  "sequence",
];

export const KIND_ICONS: Record<Kind, LucideIcon> = {
  hotkey: Keyboard,
  launch: SquareMousePointer,
  open: Globe,
  text: Type,
  media: Music,
  micMute: MicOff,
  sound: AudioLines,
  stopSounds: VolumeX,
  sequence: ListOrdered,
  delay: Clock,
};

/** 카드를 골랐을 때 만드는 빈 동작. 아직 완성되지 않았을 수 있다. */
export function emptyAction(kind: Kind): Action {
  switch (kind) {
    case "hotkey":
      return { type: "hotkey", keys: [] };
    case "launch":
      return { type: "launch", target: "" };
    case "open":
      return { type: "open", target: "" };
    case "text":
      // 새로 만드는 문장 버튼은 붙여넣기로 시작한다 (method가 없는 기존 설정은 직접 입력)
      return { type: "text", text: "", method: "paste" };
    case "media":
      return { type: "media", command: "playPause" };
    case "micMute":
      return { type: "micMute" };
    case "sound":
      return { type: "sound", file: "" };
    case "stopSounds":
      return { type: "stopSounds" };
    case "sequence":
      return { type: "sequence", steps: [] };
    case "delay":
      return { type: "delay", ms: 500 };
  }
}

export const MAX_DELAY_MS = 60_000;

/**
 * 완성되지 않았으면 이유, 완성됐으면 null. 코어 검증(`config.rs`)보다 너그럽지 않아야
 * 한다: 여기서 통과한 동작은 저장에 성공해야 한다.
 */
export function incompleteReason(action: Action, t: Dict, inSequence = false): string | null {
  const f = t.editor.forms;
  switch (action.type) {
    case "hotkey":
      return action.keys.length === 0 ? f.hotkey.incomplete : null;
    case "launch":
      return action.target.trim() === "" ? f.launch.incomplete : null;
    case "open":
      return action.target.trim() === "" ? f.open.incomplete : null;
    case "text":
      return action.text === "" ? f.text.incomplete : null;
    case "sound":
      return action.file.trim() === "" ? f.sound.incomplete : null;
    case "delay":
      return !inSequence || action.ms <= 0 || action.ms > MAX_DELAY_MS ? f.delay.incomplete : null;
    case "sequence":
      if (inSequence || action.steps.length === 0) return f.sequence.incomplete;
      return action.steps.some((s) => incompleteReason(s, t, true) !== null) ? f.sequence.incomplete : null;
    case "media":
    case "micMute":
    case "stopSounds":
      return null;
  }
}

/** 동작에서 버튼 이름을 짐작한다 (사용자가 이름을 직접 바꾸지 않았을 때 쓴다). */
export function suggestLabel(action: Action, t: Dict, appName?: string): string {
  switch (action.type) {
    case "hotkey":
      return action.keys.length ? action.keys.join("+") : t.editor.kinds.hotkey.title;
    case "launch":
      return appName ?? (fileStem(action.target) || t.editor.kinds.launch.title);
    case "open":
      return shortTarget(action.target) || t.editor.kinds.open.title;
    case "text":
      return action.text.split("\n")[0].slice(0, 12) || t.editor.kinds.text.title;
    case "media":
      return t.editor.action.media[action.command];
    case "micMute":
      return t.editor.kinds.micMute.title;
    case "sound":
      return fileStem(action.file) || t.editor.kinds.sound.title;
    case "stopSounds":
      return t.editor.kinds.stopSounds.title;
    case "sequence":
      return t.editor.kinds.sequence.title;
    case "delay":
      return t.editor.kinds.delay.title;
  }
}

function fileStem(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? "";
  return name.replace(/\.[^.]+$/, "");
}

function shortTarget(target: string): string {
  const t = target.trim();
  try {
    if (/^[a-z]+:\/\//i.test(t)) return new URL(t).hostname.replace(/^www\./, "");
  } catch {
    // 주소가 아니면 아래에서 파일·폴더 이름으로
  }
  return t.split(/[\\/]/).filter(Boolean).pop() ?? t;
}
