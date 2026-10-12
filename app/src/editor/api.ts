import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { Action } from "@/bindings/Action";
import type { Config } from "@/bindings/Config";
import { codeToVk, numpadLabel } from "./keymap";

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

/**
 * 녹화 결과. `keys`·`cancelled`·`rejected`는 `runtime.rs`의 `RecordResult`와 같은 모양이고,
 * `timeout`·`blurred`는 화면에서 만든다.
 */
export type RecordResult =
  | { kind: "keys"; keys: string[] }
  | { kind: "cancelled" }
  | { kind: "rejected"; reason: "numpadKey" | "unknownKey" }
  | { kind: "timeout" }
  | { kind: "blurred" };

/** 키 입력이 이만큼 없으면 녹화를 끝낸다. */
const RECORD_IDLE_MS = 5000;

/** 녹화 중이면 넘패드 덱 키 전달을 멈춘다 (`installDeckKeyForwarding`). */
let recordingActive = false;

/**
 * 단축키 녹화. 편집기 화면이 받은 키를 코어의 녹화 로직에 넘긴다 (전역 키는 막지 않는다).
 * 결과는 `handler`로 한 번 온다. 돌려받은 함수를 부르면 취소.
 */
export async function recordHotkey(handler: (result: RecordResult) => void): Promise<() => void> {
  await invoke("start_recording");
  recordingActive = true;
  let done = false;
  let idle = 0;

  const finish = (result: RecordResult | null) => {
    if (done) return;
    done = true;
    recordingActive = false;
    window.clearTimeout(idle);
    window.removeEventListener("keydown", onKey, true);
    window.removeEventListener("keyup", onKey, true);
    window.removeEventListener("blur", onBlur);
    if (result) handler(result);
    else void invoke("cancel_recording");
  };
  const resetIdle = () => {
    window.clearTimeout(idle);
    idle = window.setTimeout(() => {
      void invoke("cancel_recording");
      finish({ kind: "timeout" });
    }, RECORD_IDLE_MS);
  };
  const onKey = (e: KeyboardEvent) => {
    // 녹화 중에는 편집기의 다른 단축키(Ctrl+Z 등)와 버튼 동작이 일어나지 않게 막는다
    e.preventDefault();
    e.stopPropagation();
    if (e.repeat) return;
    resetIdle();
    const { vk, extended } = codeToVk(e.code);
    void invoke<RecordResult | null>("record_key", { vk, extended, down: e.type === "keydown" }).then(
      (result) => result && finish(result),
    );
  };
  // Win+D처럼 Windows가 먼저 처리하는 조합은 창이 포커스를 잃는다
  const onBlur = () => {
    void invoke("cancel_recording");
    finish({ kind: "blurred" });
  };

  window.addEventListener("keydown", onKey, true);
  window.addEventListener("keyup", onKey, true);
  window.addEventListener("blur", onBlur);
  resetIdle();
  return () => finish(null);
}

/**
 * 편집기 창이 앞에 있을 때도 넘패드 덱 키가 동작하게 한다. 편집기가 앞이면 키보드 훅이
 * 키를 받지 못하므로, 화면이 받은 넘패드 키(NumLock 꺼짐)를 코어에 넘긴다. 훅이 키를
 * 받는 환경에서는 훅이 키를 가로채 화면에 오지 않으므로 두 번 실행되지 않는다.
 */
/** 0보다 크면 넘패드 덱 키 전달을 멈춘다 (배치 편집 중 등). */
let forwardingPaused = 0;

/** 넘패드 덱 키 전달을 잠시 멈춘다. 돌려받은 함수를 부르면 다시 켠다. */
export function pauseDeckForwarding(): () => void {
  forwardingPaused++;
  let resumed = false;
  return () => {
    if (resumed) return;
    resumed = true;
    forwardingPaused--;
  };
}

export function installDeckKeyForwarding(): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (recordingActive || forwardingPaused > 0) return;
    const label = numpadLabel(e.code);
    if (!label || e.getModifierState("NumLock")) return;
    e.preventDefault();
    if (e.type === "keydown" && !e.repeat) void invoke("press_deck_key", { key: label });
  };
  window.addEventListener("keydown", onKey);
  window.addEventListener("keyup", onKey);
  return () => {
    window.removeEventListener("keydown", onKey);
    window.removeEventListener("keyup", onKey);
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
