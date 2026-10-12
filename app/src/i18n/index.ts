import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Lang } from "@/bindings/Lang";
import { en } from "./en";
import { ko, type Dict } from "./ko";

// 언어는 코어가 정한다 (설정의 language, 없으면 Windows 표시 언어). 창마다 시작할 때 한 번
// 묻고, 바뀌면 `lang://changed`를 받는다.

const DICTS: Record<Lang, Dict> = { ko, en };

let current: Lang = navigator.language.toLowerCase().startsWith("ko") ? "ko" : "en";
const subscribers = new Set<() => void>();

function setLang(lang: Lang) {
  if (lang === current) return;
  current = lang;
  document.documentElement.lang = lang;
  subscribers.forEach((f) => f());
}

/** 창의 진입점에서 한 번 부른다. */
export function initI18n() {
  document.documentElement.lang = current;
  void invoke<Lang>("get_lang").then(setLang);
  void listen<Lang>("lang://changed", (e) => setLang(e.payload));
}

function subscribe(f: () => void) {
  subscribers.add(f);
  return () => subscribers.delete(f);
}

export function useLang(): Lang {
  return useSyncExternalStore(subscribe, () => current);
}

export function useT(): Dict {
  return DICTS[useLang()];
}

export type { Dict };
