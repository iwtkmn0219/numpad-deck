// 버튼 아이콘. 설정에는 Lucide 아이콘 이름("play", "mic-off" …)만 저장한다.
// 아이콘은 이름으로 필요할 때만 불러온다(DynamicIcon) — 상주하는 OSD에 1,500여 개를 다
// 넣지 않기 위해서다.

import { DynamicIcon, iconNames, type IconName } from "lucide-react/dynamic";
import type { Action } from "@/bindings/Action";
import type { Button } from "@/bindings/Button";

const KNOWN = new Set<string>(iconNames);

export function isIconName(name: string): name is IconName {
  return KNOWN.has(name);
}

/** 아이콘을 고르지 않았을 때 동작에 맞춰 보여줄 아이콘. */
export function defaultIcon(action: Action): IconName {
  switch (action.type) {
    case "hotkey":
      return "keyboard";
    case "launch":
      return "app-window";
    case "open": {
      const t = action.target.trim();
      if (t === "" || /^[a-z][a-z0-9+.-]*:/i.test(t) && !/^[a-z]:[\\/]/i.test(t)) return "globe";
      return /\.[a-z0-9]{1,5}$/i.test(t) ? "file" : "folder";
    }
    case "text":
      return "type";
    case "media":
      return (
        {
          playPause: "play",
          next: "skip-forward",
          prev: "skip-back",
          stop: "square",
          volumeUp: "volume-2",
          volumeDown: "volume-1",
          volumeMute: "volume-x",
        } as const
      )[action.command];
    case "micMute":
      return "mic-off";
    case "sound":
      return "audio-lines";
    case "stopSounds":
      return "volume-off";
    case "sequence":
      return "list-ordered";
    case "delay":
      return "clock";
  }
}

/** 버튼에 보여줄 아이콘: 고른 아이콘, 없거나 모르는 이름이면 동작에 맞는 기본 아이콘. */
export function buttonIcon(button: Button): IconName {
  return button.icon && isIconName(button.icon) ? button.icon : defaultIcon(button.action);
}

/** 이름으로 그리는 아이콘. 불러오는 동안은 같은 크기의 빈 자리. */
export function Icon({ name, className }: { name: IconName; className?: string }) {
  return <DynamicIcon name={name} className={className} fallback={() => <span className={className} />} />;
}

export { iconNames, type IconName };
