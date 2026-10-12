// 언어를 바꿀 때, 사용자가 직접 정하지 않은 이름을 새 언어로 바꾼다.
//
// "직접 정하지 않은 이름"은 지금 이름이 어느 언어로든 기본값과 같은 것이다:
// - 페이지: 프리셋 이름, "새 페이지"
// - 버튼: 같은 자리·같은 동작인 프리셋 버튼의 이름, 동작에서 짐작한 이름(`suggestLabel`)
// 하나라도 고친 이름은 그대로 둔다.

import type { Action } from "@/bindings/Action";
import type { Config } from "@/bindings/Config";
import type { Lang } from "@/bindings/Lang";
import type { Preset } from "@/bindings/Preset";
import { DICTS } from "@/i18n";
import { suggestLabel } from "./actions/meta";

const LANGS: Lang[] = ["ko", "en"];

export type PresetsByLang = Record<Lang, Preset[]>;

function sameAction(a: Action, b: Action): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** `config`(편집 중인 사본)의 기본 이름을 `to` 언어로 바꾼다. */
export function relabel(config: Config, to: Lang, presets: PresetsByLang): void {
  const target = (id: string) => presets[to].find((p) => p.id === id);

  for (const page of config.pages) {
    const presetPage = LANGS.flatMap((l) => presets[l]).find((p) => p.name === page.name);
    if (presetPage) page.name = target(presetPage.id)?.name ?? page.name;
    else if (LANGS.some((l) => DICTS[l].editor.newPageName === page.name)) page.name = DICTS[to].editor.newPageName;

    for (const [key, button] of Object.entries(page.buttons)) {
      if (!button) continue;
      const k = key as keyof typeof page.buttons;
      const fromPreset = LANGS.flatMap((l) => presets[l]).find((p) => {
        const b = p.page.buttons[k];
        return b && b.label === button.label && sameAction(b.action, button.action);
      });
      if (fromPreset) {
        button.label = target(fromPreset.id)?.page.buttons[k]?.label ?? button.label;
      } else if (LANGS.some((l) => suggestLabel(button.action, DICTS[l]) === button.label)) {
        button.label = suggestLabel(button.action, DICTS[to]);
      }
    }
  }
}
