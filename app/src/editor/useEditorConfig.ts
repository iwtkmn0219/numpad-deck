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
  /** 저장 예약만 되고 아직 시작하지 않은 설정 (`flush`가 바로 저장한다) */
  const pending = useRef<Config | null>(null);
  /** 진행 중인 저장의 끝. 저장은 이 줄에 이어 붙여 하나씩만 한다. */
  const inflight = useRef<Promise<void>>(Promise.resolve());
  /** 마지막으로 저장한(또는 받아 온) 내용. 같은 내용을 다시 저장하거나 받지 않게. */
  const lastSaved = useRef<string>("");
  /** 마지막 변경의 묶음 키와 시각 (글자 입력을 기록 하나로 묶기 위해) */
  const lastEdit = useRef<{ key: string; at: number } | null>(null);
  /** 이 시각까지 밖에서 온 변경은 되돌리기 기록에 넣지 않는다 (`forgetHistory`) */
  const forgetUntil = useRef(0);

  const show = useCallback((next: Config) => {
    current.current = next;
    setConfig(next);
    setHistoryVersion((v) => v + 1);
  }, []);

  /** 저장한다. 앞선 저장이 끝난 뒤에 시작하고, 실패는 상태로만 알린다 (거부되지 않는다). */
  const persist = useCallback((next: Config) => {
    inflight.current = inflight.current.then(async () => {
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
    });
  }, []);

  const scheduleSave = useCallback(
    (next: Config) => {
      window.clearTimeout(timer.current);
      setStatus({ kind: "saving" });
      pending.current = next;
      timer.current = window.setTimeout(() => {
        pending.current = null;
        persist(next);
      }, SAVE_DELAY_MS);
    },
    [persist],
  );

  /**
   * 예약된 저장을 기다리지 않고 지금 하고, 진행 중인 저장까지 끝날 때까지 기다린다.
   * 코어가 설정을 바꾸기 전(파일 가져오기)에 불러서, 마지막 고침이 백업·추가에 들어가고
   * 늦은 저장이 새 설정을 덮어쓰지 않게 한다.
   */
  const flush = useCallback(async () => {
    window.clearTimeout(timer.current);
    const next = pending.current;
    pending.current = null;
    if (next) persist(next);
    await inflight.current;
  }, [persist]);

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
      if (Date.now() < forgetUntil.current) {
        forgetUntil.current = 0;
        past.current = [];
      } else if (current.current) past.current = [...past.current, current.current].slice(-HISTORY_LIMIT);
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

  /**
   * 코어가 저장한 설정(파일 가져오기)을 바로 받는다. 파일 감시로 같은 내용이 다시 와도 무시된다.
   * `resetHistory`면 되돌리기 기록을 비운다 (설정 바꾸기는 되돌리기 대상이 아니다).
   */
  const adopt = useCallback(
    (next: Config, resetHistory: boolean) => {
      // 가져오기 전 내용의 저장 예약이 새 설정을 덮어쓰지 않게
      window.clearTimeout(timer.current);
      pending.current = null;
      setStatus({ kind: "saved" });
      const text = canonical(next);
      // 파일 감시가 먼저 같은 내용을 보내왔으면 기록은 이미 쌓였다
      const already = current.current !== null && canonical(current.current) === text;
      lastSaved.current = text;
      if (resetHistory) past.current = [];
      else if (current.current && !already) past.current = [...past.current, current.current].slice(-HISTORY_LIMIT);
      future.current = [];
      lastEdit.current = null;
      show(next);
    },
    [show],
  );

  /**
   * 되돌리기 기록을 비우고, 곧(10초 안에) 파일 감시로 올 변경도 기록에 넣지 않는다.
   * 설정 바꾸기 뒤 설정을 바로 받지 못했을 때 쓴다.
   */
  const forgetHistory = useCallback(() => {
    past.current = [];
    future.current = [];
    lastEdit.current = null;
    forgetUntil.current = Date.now() + 10_000;
    setHistoryVersion((v) => v + 1);
  }, []);

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
    adopt,
    flush,
    forgetHistory,
    undo,
    redo,
    canUndo: past.current.length > 0,
    canRedo: future.current.length > 0,
  };
}
