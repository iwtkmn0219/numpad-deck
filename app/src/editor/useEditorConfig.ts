import { useCallback, useEffect, useRef, useState } from "react";
import type { Config } from "@/bindings/Config";
import { getConfig, onConfigChanged, saveConfig } from "./api";

/** 바뀐 뒤 이만큼 아무 변화가 없으면 저장한다 (글자를 칠 때마다 저장하지 않게). */
const SAVE_DELAY_MS = 300;
const HISTORY_LIMIT = 50;
/** 같은 입력란을 이만큼 안에 다시 고치면 되돌리기 기록 하나로 묶는다. */
const COALESCE_MS = 1000;

/**
 * 내용 비교용 문자열. 키 순서를 정렬한다: 코어가 파일에 쓰는 순서(`version`·`language`·`pages`,
 * 버튼은 넘패드 배치 순)와 화면이 객체에 넣은 순서가 달라도 같은 내용으로 본다. 그러지 않으면
 * 저장한 내용이 파일 감시로 돌아올 때 밖에서 바뀐 것으로 여겨 되돌리기 기록이 하나 더 생긴다.
 */
function canonical(value: unknown): string {
  return JSON.stringify(value, (_key, v: unknown) =>
    v && typeof v === "object" && !Array.isArray(v)
      ? Object.fromEntries(Object.entries(v).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)))
      : v,
  );
}

export type SaveStatus = { kind: "saved" } | { kind: "saving" } | { kind: "failed"; reason: string };

/**
 * 편집 중인 설정. 바꿀 때마다 자동 저장하고, 되돌리기·다시 실행을 지원한다.
 * 설정 파일이 밖에서 바뀌면(메모장으로 고침 등) 그 내용으로 바꾼다.
 *
 * 현재 값과 기록은 ref에 두고 화면 갱신용으로만 state를 쓴다. setState 갱신 함수 안에서
 * 기록을 바꾸면 개발 모드(StrictMode)에서 두 번 실행되기 때문이다.
 */
export function useEditorConfig() {
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<SaveStatus>({ kind: "saved" });
  const [, setHistoryVersion] = useState(0);
  const current = useRef<Config | null>(null);
  const past = useRef<Config[]>([]);
  const future = useRef<Config[]>([]);
  const timer = useRef<number | undefined>(undefined);
  /** 마지막으로 저장한(또는 받아 온) 내용. 같은 내용을 다시 저장하거나 받지 않게. */
  const lastSaved = useRef<string>("");
  /** 마지막 변경의 묶음 키와 시각 (글자 입력을 기록 하나로 묶기 위해) */
  const lastEdit = useRef<{ key: string; at: number } | null>(null);

  const show = useCallback((next: Config) => {
    current.current = next;
    setConfig(next);
    setHistoryVersion((v) => v + 1);
  }, []);

  const scheduleSave = useCallback((next: Config) => {
    window.clearTimeout(timer.current);
    setStatus({ kind: "saving" });
    timer.current = window.setTimeout(async () => {
      const text = canonical(next);
      if (text === lastSaved.current) {
        setStatus({ kind: "saved" });
        return;
      }
      try {
        await saveConfig(next);
        lastSaved.current = text;
        setStatus({ kind: "saved" });
      } catch (reason) {
        setStatus({ kind: "failed", reason: String(reason) });
      }
    }, SAVE_DELAY_MS);
  }, []);

  useEffect(() => {
    let alive = true;
    getConfig().then((c) => {
      if (!alive) return;
      lastSaved.current = canonical(c);
      show(c);
    });
    const unlisten = onConfigChanged((c) => {
      const text = canonical(c);
      if (text === lastSaved.current) return;
      lastSaved.current = text;
      if (current.current) past.current = [...past.current, current.current].slice(-HISTORY_LIMIT);
      future.current = [];
      show(c);
    });
    return () => {
      alive = false;
      unlisten.then((f) => f());
    };
  }, [show]);

  /**
   * 설정을 바꾼다. `recipe`는 사본을 받아 고친다. `coalesce`를 주면 같은 키로 잇따라 고친
   * 것(예: 한 입력란에 글자 치기)을 되돌리기 기록 하나로 묶는다.
   */
  const update = useCallback(
    (recipe: (draft: Config) => void, coalesce?: string) => {
      const before = current.current;
      if (!before) return;
      const next = structuredClone(before);
      recipe(next);
      const now = Date.now();
      const merge =
        coalesce !== undefined &&
        lastEdit.current?.key === coalesce &&
        now - lastEdit.current.at < COALESCE_MS &&
        past.current.length > 0;
      if (!merge) past.current = [...past.current, before].slice(-HISTORY_LIMIT);
      lastEdit.current = coalesce !== undefined ? { key: coalesce, at: now } : null;
      future.current = [];
      show(next);
      scheduleSave(next);
    },
    [show, scheduleSave],
  );

  const undo = useCallback(() => {
    const before = current.current;
    const previous = past.current.at(-1);
    if (!before || !previous) return;
    past.current = past.current.slice(0, -1);
    future.current = [before, ...future.current];
    lastEdit.current = null;
    show(previous);
    scheduleSave(previous);
  }, [show, scheduleSave]);

  const redo = useCallback(() => {
    const before = current.current;
    const next = future.current[0];
    if (!before || !next) return;
    future.current = future.current.slice(1);
    past.current = [...past.current, before];
    lastEdit.current = null;
    show(next);
    scheduleSave(next);
  }, [show, scheduleSave]);

  // Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y. 글자 입력 칸 안에서는 브라우저 기본 되돌리기를 쓴다.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      if (!e.ctrlKey) return;
      const key = e.key.toLowerCase();
      if (key === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
      } else if ((key === "z" && e.shiftKey) || key === "y") {
        e.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  return {
    config,
    status,
    update,
    undo,
    redo,
    canUndo: past.current.length > 0,
    canRedo: future.current.length > 0,
  };
}
