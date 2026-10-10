import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
