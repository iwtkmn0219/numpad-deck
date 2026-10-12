import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Mode = "deck" | "numbers" | "paused";

/**
 * 지금 모드. 코어(훅의 NumLock 확인, 1초 주기)에서 받고, 이 화면이 받은 키의 NumLock 상태로
 * 바로 고친다 (편집기가 앞이면 훅이 키를 못 받아 늦을 수 있다).
 */
export function useMode(): Mode | null {
  const [mode, setMode] = useState<Mode | null>(null);
  useEffect(() => {
    void invoke<Mode>("get_mode").then(setMode);
    const unlisten = listen<Mode>("mode://changed", (e) => setMode(e.payload));
    const onKey = (e: KeyboardEvent) => {
      const on = e.getModifierState("NumLock");
      setMode((m) => (m === "paused" ? m : on ? "numbers" : "deck"));
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKey);
    return () => {
      void unlisten.then((f) => f());
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKey);
    };
  }, []);
  return mode;
}
