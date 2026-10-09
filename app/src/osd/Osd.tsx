import { useEffect, useState } from "react";
import type { Mode, OsdPayload } from "./types";

/** 누른 버튼 강조 시간. `-`로 고정해 둔 동안에도 강조는 이만큼만 보인다. */
const HIGHLIGHT_MS = 800;

// 실제 넘패드와 같은 모양으로 배치한다. 4열 × 5행, +와 Enter는 두 줄, 0은 두 칸.
type Slot =
  | { kind: "button"; key: string; area: string }
  | { kind: "system"; key: string; label: string; area: string }
  | { kind: "mode"; area: string };

const SLOTS: Slot[] = [
  { kind: "mode", area: "1 / 1 / 2 / 2" },
  { kind: "system", key: "/", label: "◀ 이전", area: "1 / 2 / 2 / 3" },
  { kind: "system", key: "*", label: "다음 ▶", area: "1 / 3 / 2 / 4" },
  { kind: "system", key: "-", label: "화면", area: "1 / 4 / 2 / 5" },
  { kind: "button", key: "7", area: "2 / 1 / 3 / 2" },
  { kind: "button", key: "8", area: "2 / 2 / 3 / 3" },
  { kind: "button", key: "9", area: "2 / 3 / 3 / 4" },
  { kind: "button", key: "+", area: "2 / 4 / 4 / 5" },
  { kind: "button", key: "4", area: "3 / 1 / 4 / 2" },
  { kind: "button", key: "5", area: "3 / 2 / 4 / 3" },
  { kind: "button", key: "6", area: "3 / 3 / 4 / 4" },
  { kind: "button", key: "1", area: "4 / 1 / 5 / 2" },
  { kind: "button", key: "2", area: "4 / 2 / 5 / 3" },
  { kind: "button", key: "3", area: "4 / 3 / 5 / 4" },
  { kind: "button", key: "Enter", area: "4 / 4 / 6 / 5" },
  { kind: "button", key: "0", area: "5 / 1 / 6 / 3" },
  { kind: "system", key: ".", label: "처음", area: "5 / 3 / 6 / 4" },
];

const MODE_TEXT: Record<Mode, string> = {
  deck: "덱 모드",
  numbers: "숫자 모드",
  paused: "일시정지",
};

export function Osd({ payload }: { payload: OsdPayload }) {
  const { mode, error } = payload;
  const active = mode === "deck";
  const [highlight, setHighlight] = useState<string | null>(null);

  // 새 알림(seq)마다 강조를 켜고 잠시 뒤 끈다. 실패 표시는 다음 알림까지 둔다.
  useEffect(() => {
    setHighlight(payload.highlight);
    if (!payload.highlight || payload.error) return;
    const timer = setTimeout(() => setHighlight(null), HIGHLIGHT_MS);
    return () => clearTimeout(timer);
  }, [payload.seq, payload.highlight, payload.error]);

  return (
    <div className="flex h-screen w-screen items-end justify-end">
      <div className="w-[340px] rounded-2xl border border-white/10 bg-neutral-900 p-3 text-white shadow-2xl">
        <header className="mb-2 flex items-center justify-between gap-2 px-1">
          <div className="min-w-0 truncate text-sm font-semibold">
            <span className="mr-1.5 text-neutral-400 tabular-nums">
              {payload.page + 1}/{payload.pageCount}
            </span>
            {payload.pageName}
          </div>
          <span
            className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] font-medium ${
              active
                ? "bg-blue-500/20 text-blue-300"
                : mode === "paused"
                  ? "bg-amber-500/20 text-amber-300"
                  : "bg-white/10 text-neutral-300"
            }`}
          >
            {MODE_TEXT[mode]}
          </span>
        </header>

        <div
          className={`grid grid-cols-4 grid-rows-[repeat(5,52px)] gap-1.5 transition-opacity ${
            active ? "" : "opacity-40"
          }`}
        >
          {SLOTS.map((slot) => (
            <Cell
              key={slot.kind === "mode" ? "mode" : slot.key}
              slot={slot}
              payload={payload}
              lit={slot.kind !== "mode" && slot.key === highlight}
              failed={slot.kind !== "mode" && slot.key === highlight && error !== null}
            />
          ))}
        </div>

        {error && (
          <p className="mt-2 rounded-lg bg-red-500/15 px-2.5 py-1.5 text-xs leading-snug text-red-200">
            {error}
          </p>
        )}
        {mode === "numbers" && (
          <p className="mt-2 px-1 text-xs text-neutral-400">NumLock을 끄면 버튼을 쓸 수 있어요.</p>
        )}
      </div>
    </div>
  );
}

function Cell({
  slot,
  payload,
  lit,
  failed,
}: {
  slot: Slot;
  payload: OsdPayload;
  lit: boolean;
  failed: boolean;
}) {
  const base = "flex flex-col overflow-hidden rounded-lg px-2 py-1.5 transition-colors duration-150";

  if (slot.kind === "mode") {
    return (
      <div style={{ gridArea: slot.area }} className={`${base} items-center justify-center bg-white/5`}>
        <span className="text-[10px] text-neutral-500">NumLock</span>
        <span className="text-xs font-medium text-neutral-300">
          {payload.mode === "numbers" ? "켜짐" : "꺼짐"}
        </span>
      </div>
    );
  }

  if (slot.kind === "system") {
    return (
      <div
        style={{ gridArea: slot.area }}
        className={`${base} items-center justify-center ${lit ? "bg-white/25" : "bg-white/5"}`}
      >
        <span className="text-xs text-neutral-400">{slot.label}</span>
      </div>
    );
  }

  const label = payload.buttons[slot.key];
  const tone = failed
    ? "bg-red-500 text-white"
    : lit
      ? "bg-blue-500 text-white"
      : label
        ? "bg-white/10 text-white"
        : "border border-dashed border-white/10 text-neutral-600";

  return (
    <div style={{ gridArea: slot.area }} className={`${base} ${tone}`}>
      <span className={`text-[10px] leading-none ${lit ? "text-white/70" : "text-neutral-500"}`}>
        {slot.key}
      </span>
      <span className="mt-auto line-clamp-2 text-[12.5px] leading-tight font-medium break-keep">
        {label ?? ""}
      </span>
    </div>
  );
}
