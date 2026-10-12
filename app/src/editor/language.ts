import { invoke } from "@tauri-apps/api/core";
import type { Config } from "@/bindings/Config";
import type { Lang } from "@/bindings/Lang";
import { listPresets } from "./PresetDialog";
import { relabel } from "./relabel";

export type LanguageChoice = "auto" | Lang;

/**
 * 언어를 바꾼다 (`auto` = Windows 표시 언어). 사용자가 직접 정하지 않은 페이지·버튼 이름도
 * 새 언어로 바꾸며, 되돌리기 한 번에 함께 돌아간다. 편집기 언어 메뉴와 온보딩이 같이 쓴다.
 */
export async function chooseLanguage(
  choice: LanguageChoice,
  update: (recipe: (draft: Config) => void) => void,
): Promise<void> {
  const explicit = choice === "auto" ? null : choice;
  const [to, ko, en] = await Promise.all([
    explicit ?? invoke<Lang>("system_lang"),
    listPresets("ko"),
    listPresets("en"),
  ]);
  update((c) => {
    if (explicit) c.language = explicit;
    else delete c.language;
    relabel(c, to, { ko, en });
  });
}
