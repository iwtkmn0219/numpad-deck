import type { Action } from "@/bindings/Action";
import type { Dict } from "@/i18n";

/** 동작을 사람이 읽는 한 줄로. */
export function describeAction(action: Action, t: Dict): string {
  const a = t.editor.action;
  switch (action.type) {
    case "hotkey":
      return a.hotkey(action.keys.join(" + "));
    case "launch":
      return a.launch(fileName(action.target));
    case "open":
      return a.open(action.target);
    case "text":
      return a.text(action.text.replace(/\n/g, "⏎"));
    case "media":
      return a.media[action.command];
    case "micMute":
      return a.micMute;
    case "sound":
      return a.sound(fileName(action.file));
    case "sequence":
      return a.sequence(action.steps.length);
    case "delay":
      return a.delay(action.ms);
  }
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}
