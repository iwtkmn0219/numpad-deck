import { useEffect, useState } from "react";
import { gridFor, gridStyle, type Slot } from "@/shared/layout";
import { buttonIcon, Icon } from "@/shared/icons";
import { useT, type Dict } from "@/i18n";
import type { OsdPayload } from "./types";

/** 누른 버튼 강조 시간. `-`로 고정해 둔 동안에도 강조는 이만큼만 보인다. */
const HIGHLIGHT_MS = 800;

export function Osd({ payload }: { payload: OsdPayload }) {
  const t = useT();
  const { mode, error } = payload;
  const active = mode === "deck";
  const grid = gridFor(payload.layout, payload.systemKeys);
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
            {t.osd.mode[mode]}
          </span>
        </header>

        <div
          style={gridStyle(grid, 52)}
          className={`gap-1.5 transition-opacity ${active ? "" : "opacity-40"}`}
        >
          {grid.slots.map((slot) => (
            <Cell
              key={slot.kind === "mode" ? "mode" : slot.key}
              slot={slot}
              payload={payload}
              t={t}
              lit={slot.kind !== "mode" && slot.key === highlight}
              failed={
                slot.kind !== "mode" && slot.key === highlight && error !== null
              }
            />
          ))}
        </div>

        {error && (
          <p className="mt-2 rounded-lg bg-red-500/15 px-2.5 py-1.5 text-xs leading-snug text-red-200">
            {error}
          </p>
        )}
        {mode === "numbers" && (
          <p className="mt-2 px-1 text-xs text-neutral-400">
            {t.osd.numbersHint}
          </p>
        )}
      </div>
    </div>
  );
}

function Cell({
  slot,
  payload,
  t,
  lit,
  failed,
}: {
  slot: Slot;
  payload: OsdPayload;
  t: Dict;
  lit: boolean;
  failed: boolean;
}) {
  const base =
    "flex flex-col overflow-hidden rounded-lg px-2 py-1.5 transition-colors duration-150";

  if (slot.kind === "mode") {
    return (
      <div
        style={{ gridArea: slot.area }}
        className={`${base} items-center justify-center bg-white/5`}
      >
        <span className="text-[10px] text-neutral-500">
          {t.osd.numlockLabel}
        </span>
        <span className="text-xs font-medium text-neutral-300">
          {payload.mode === "numbers" ? t.osd.numlockOn : t.osd.numlockOff}
        </span>
      </div>
    );
  }

  if (slot.kind === "system") {
    return (
      <div
        style={{ gridArea: slot.area }}
        className={`${base} items-center justify-center ${lit ? "bg-white/5 ring-2 ring-white/70" : "bg-white/5"}`}
      >
        <span className="text-xs text-neutral-400">{t.system[slot.role]}</span>
      </div>
    );
  }

  const button = payload.buttons[slot.key];
  const label = button?.label;
  // 마이크 음소거 버튼은 지금 상태를 보인다: 켜져 있으면 초록 불, 꺼져 있으면 마이크 꺼짐 아이콘
  const mic = button?.action.type === "micMute" ? payload.micMuted : null;
  // 누른 버튼은 흰 테두리로 강조한다. 마이크 버튼은 누를 때 바뀐 상태 색(켜짐 초록, 꺼짐 빨강)을 테두리와 함께 바로 보인다
  const tone = failed
    ? "bg-red-500 text-white"
    : lit && mic !== null
      ? `${mic ? "bg-red-600" : "bg-green-600"} text-white ring-2 ring-white/70`
      : lit
        ? "bg-white/15 text-white ring-2 ring-white/70"
        : mic === false
          ? "bg-green-600 text-white"
          : label
            ? "bg-white/10 text-white"
            : "border border-dashed border-white/10 text-neutral-600";

  return (
    <div style={{ gridArea: slot.area }} className={`${base} ${tone}`}>
      <span className="flex w-full items-start justify-between">
        <span
          className={`text-[10px] leading-none ${lit ? "text-white/70" : "text-neutral-500"}`}
        >
          {slot.key}
        </span>
        {button && (
          <Icon
            name={
              mic === true
                ? "mic-off"
                : mic === false
                  ? "mic"
                  : buttonIcon(button)
            }
            className="size-4 opacity-90"
          />
        )}
      </span>
      <span className="mt-auto line-clamp-2 w-full text-[12.5px] leading-tight font-medium break-keep [overflow-wrap:anywhere]">
        {label ?? ""}
      </span>
    </div>
  );
}
