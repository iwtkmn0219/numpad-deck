import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Action } from "@/bindings/Action";
import type { Config } from "@/bindings/Config";

// Rust `app/src-tauri/src/lib.rs`의 command, `runtime.rs`의 이벤트와 맞아야 한다.

export function getConfig(): Promise<Config> {
  return invoke<Config>("get_config");
}

/** 검증에 실패하면 이유(문자열)로 reject된다. */
export function saveConfig(config: Config): Promise<void> {
  return invoke("save_config", { config });
}

/** 설정 파일이 편집기 밖에서 바뀌었을 때. */
export function onConfigChanged(handler: (config: Config) => void): Promise<UnlistenFn> {
  return listen<Config>("config://changed", (event) => handler(event.payload));
}

/** 지금 바로 한 번 실행한다. 실패하면 OSD에 이유가 뜬다. */
export function testAction(label: string, action: Action): Promise<void> {
  return invoke("test_action", { label, action });
}

export interface AppEntry {
  name: string;
  target: string;
}

let appsCache: Promise<AppEntry[]> | null = null;

/** 설치된 앱 목록. 처음 한 번만 읽고 기억한다. */
export function listApps(): Promise<AppEntry[]> {
  appsCache ??= invoke<AppEntry[]>("list_apps").catch((e) => {
    appsCache = null;
    throw e;
  });
  return appsCache;
}

/** `runtime.rs`의 `RecordResult`와 같은 모양 */
export type RecordResult =
  | { kind: "keys"; keys: string[] }
  | { kind: "cancelled" }
  | { kind: "rejected"; reason: "numpadKey" | "unknownKey" }
  | { kind: "timeout" };

/** 단축키 녹화. 녹화 중에는 모든 키가 막히고, 결과가 `handler`로 한 번 온다. */
export async function recordHotkey(handler: (result: RecordResult) => void): Promise<() => void> {
  let done = false;
  const unlisten = await listen<RecordResult>("hotkey://recorded", (event) => {
    if (done) return;
    done = true;
    unlisten();
    handler(event.payload);
  });
  await invoke("start_recording");
  return () => {
    if (done) return;
    done = true;
    unlisten();
    void invoke("cancel_recording");
  };
}

/** 파일 고르기. `extensions`가 비면 모든 파일. 취소하면 null. */
export async function pickFile(filterName: string, extensions: string[]): Promise<string | null> {
  const filters = extensions.length ? [{ name: filterName, extensions }] : undefined;
  const picked = await open({ multiple: false, directory: false, filters });
  return typeof picked === "string" ? picked : null;
}

export async function pickFolder(): Promise<string | null> {
  const picked = await open({ multiple: false, directory: true });
  return typeof picked === "string" ? picked : null;
}
