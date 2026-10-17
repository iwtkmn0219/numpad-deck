import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { Button as DeckButton } from "@/bindings/Button";
import type { ButtonKeyName } from "@/bindings/ButtonKeyName";
import type { Page } from "@/bindings/Page";
import { gridStyle, type Grid, type SystemKey } from "@/shared/layout";
import { Plus } from "lucide-react";
import { cn } from "@/lib/utils";
import { buttonIcon, Icon } from "@/shared/icons";
import { useT } from "@/i18n";

export type Selection = { kind: "button"; key: ButtonKeyName } | { kind: "system"; key: SystemKey };

interface Props {
  grid: Grid;
  page: Page;
  selected: Selection | null;
  onSelect: (selection: Selection) => void;
  /** 버튼을 끌어 `to` 칸에 놓았다. `to`에 버튼이 있으면 맞바꾸고, 비었으면 옮긴다. */
  onMove: (from: ButtonKeyName, to: ButtonKeyName) => void;
  /** 버튼을 놓을 수 없는 칸 (패널에서 아직 다 만들지 않은 버튼이 있는 칸: 덮어쓰면 되돌릴 수 없다) */
  blocked?: ButtonKeyName | null;
}

/** 실제 넘패드 모양 그리드. 칸을 누르면 오른쪽 패널에서 편집한다. 받침 위에 키캡처럼 그린다. */
export function KeyGrid({ grid, page, selected, onSelect, onMove, blocked = null }: Props) {
  const t = useT();
  const drag = useKeyDrag(page, onMove, blocked);

  return (
    <div
      ref={drag.container}
      className="rounded-[18px] bg-muted p-3.5 select-none"
      onPointerDown={drag.onPointerDown}
      onPointerMove={drag.onPointerMove}
      onPointerUp={drag.onPointerUp}
      onPointerCancel={drag.onPointerCancel}
      onLostPointerCapture={drag.onPointerCancel}
      onClickCapture={drag.onClickCapture}
    >
      <div className="gap-2" style={gridStyle(grid, 84)}>
        {grid.slots.map((slot) => {
          if (slot.kind === "mode") {
            return (
              <div
                key="mode"
                style={{ gridArea: slot.area }}
                className="flex items-center justify-center text-xs text-muted-foreground"
              >
                NumLock
              </div>
            );
          }
          const isSelected = selected?.kind === slot.kind && selected.key === slot.key;

          if (slot.kind === "system") {
            return (
              <button
                key={slot.key}
                style={{ gridArea: slot.area }}
                onClick={() => onSelect({ kind: "system", key: slot.key })}
                title={t.editor.grid.systemHint}
                className={cn(
                  "flex flex-col items-start rounded-xl border border-key-edge px-3 py-2 text-left text-muted-foreground transition-colors hover:bg-white/50",
                  isSelected && "key-selected",
                )}
              >
                <KeyNumber label={slot.key} selected={isSelected} />
                <span className="mt-auto text-sm">{t.system[slot.role]}</span>
              </button>
            );
          }

          const button = page.buttons[slot.key];
          const isSource = drag.active?.from === slot.key;
          const isTarget = drag.active?.target === slot.key;
          return (
            <button
              key={slot.key}
              data-key={slot.key}
              // 버튼이 있는 칸만 끌 수 있다 (빈 칸, 패널에서 아직 다 만들지 않은 버튼은 여기에 없다)
              data-draggable={button ? "" : undefined}
              style={{ gridArea: slot.area }}
              onClick={() => onSelect({ kind: "button", key: slot.key })}
              className={cn(
                "flex min-w-0 flex-col items-start overflow-hidden rounded-xl px-3 pt-2 pb-[11px] text-left",
                button
                  ? "keycap touch-none"
                  : "border-[1.5px] border-dashed border-input text-muted-foreground opacity-85 transition hover:bg-white/50 hover:opacity-100",
                isSelected && "key-selected",
                // 끄는 중: 원래 칸은 흐리게, 놓을 칸은 테두리와 옅은 파랑
                drag.active && "cursor-grabbing",
                // 원래 칸은 누른 모양(:active)으로 내려가지 않게
                isSource &&
                  "opacity-40 active:transform-none active:shadow-[inset_0_-3px_0_var(--key-edge),0_1px_2px_rgb(31_30_28/0.06)]",
                isTarget && "bg-selection-soft ring-[3px] ring-selection",
              )}
            >
              <KeyFace label={slot.key} button={button} selected={isSelected} emptyText={t.editor.grid.empty} />
            </button>
          );
        })}
      </div>
      {drag.ghost && createPortal(<DragGhost ghost={drag.ghost} />, document.body)}
    </div>
  );
}

/** 칸 안의 모양: 키 번호, 아이콘, 이름 (칸과 끄는 키캡이 같이 쓴다) */
function KeyFace({
  label,
  button,
  selected,
  emptyText,
}: {
  label: string;
  button?: DeckButton;
  selected: boolean;
  emptyText: string;
}) {
  return (
    <>
      <span className="flex w-full items-start justify-between">
        <KeyNumber label={label} selected={selected} />
        {button && <Icon name={buttonIcon(button)} className="size-5 text-foreground" />}
      </span>
      {button ? (
        <span className="mt-auto line-clamp-2 w-full text-[13.5px] font-medium break-keep [overflow-wrap:anywhere]">
          {button.label}
        </span>
      ) : (
        <span className="mt-auto flex items-center gap-1 text-[13px] font-medium">
          <Plus className="size-3.5" />
          {emptyText}
        </span>
      )}
    </>
  );
}

/** 왼쪽 위 키 번호. 선택한 칸은 파란 번호표로 (색만으로 알리지 않게). */
function KeyNumber({ label, selected }: { label: string; selected: boolean }) {
  return (
    <span
      className={cn(
        "text-[11px] leading-[18px] font-medium tabular-nums",
        selected ? "rounded-[5px] bg-selection px-[5px] text-white" : "text-muted-foreground",
      )}
    >
      {label}
    </span>
  );
}

interface Ghost {
  key: ButtonKeyName;
  button: DeckButton | undefined;
  width: number;
  height: number;
  x: number;
  y: number;
}

/** 끄는 동안 포인터를 따라다니는 반투명 키캡 */
function DragGhost({ ghost }: { ghost: Ghost }) {
  return (
    <div
      aria-hidden
      className="keycap pointer-events-none fixed top-0 left-0 z-50 flex min-w-0 flex-col items-start overflow-hidden rounded-xl px-3 pt-2 pb-[11px] text-left opacity-70 shadow-[inset_0_-3px_0_var(--key-edge),0_8px_20px_rgb(31_30_28/0.18)] transition-none select-none"
      style={{ width: ghost.width, height: ghost.height, transform: `translate(${ghost.x}px, ${ghost.y}px)` }}
    >
      <KeyFace label={ghost.key} button={ghost.button} selected={false} emptyText="" />
    </div>
  );
}

/** 끌기로 보는 거리(px). 이보다 적게 움직이면 그냥 클릭이다. */
const DRAG_THRESHOLD = 5;

interface DragState {
  from: ButtonKeyName;
  pointerId: number;
  /** 누른 칸 (끌기가 시작되면 여기에 포인터를 잡는다) */
  el: HTMLElement;
  startX: number;
  startY: number;
  /** 누른 순간 잰 칸의 위치와 크기 */
  rect: DOMRect;
  /** 문턱을 넘어 끌기가 시작됐다 */
  started: boolean;
  /** Esc로 취소했다. 손을 뗄 때까지 상태는 두고(뒤따르는 클릭을 막으려고) 아무것도 하지 않는다. */
  cancelled: boolean;
  /** 포인터 아래의 놓을 수 있는 칸 (원래 칸·시스템 키 칸·그리드 밖이면 null) */
  target: ButtonKeyName | null;
  x: number;
  y: number;
}

/**
 * 그리드에서 버튼 끌어 자리 옮기기·맞바꾸기. `PageTabs`의 `useTabDrag`와 같은 방식이다: 문턱을 넘으면
 * 포인터를 잡고, 끝난 직후의 클릭은 막는다. 놓을 칸은 칸의 실제 화면 위치로 찾으므로 설정의 `layout`으로
 * 바뀐 모양에서도 맞는다. 버튼이 있는 칸만 끌 수 있고 버튼 칸에만 놓을 수 있다. 그리드 밖·시스템 키 칸
 * 에 놓거나 Esc면 취소.
 */
function useKeyDrag(
  page: Page,
  onMove: (from: ButtonKeyName, to: ButtonKeyName) => void,
  blocked: ButtonKeyName | null,
) {
  const container = useRef<HTMLDivElement>(null);
  const [state, setStateRaw] = useState<DragState | null>(null);
  // 포인터 이벤트가 다시 그리기보다 먼저 올 수 있어 최신 값은 ref로도 든다
  const ref = useRef<DragState | null>(null);
  const setState = (next: DragState | null) => {
    ref.current = next;
    setStateRaw(next);
  };
  /** 끌기를 막 끝냈으면 뒤따르는 클릭(칸 고르기)을 무시한다 */
  const justDragged = useRef(false);

  /** 이 좌표 아래의 놓을 수 있는 버튼 칸. 칸 사이 틈·시스템 키 칸·그리드 밖·놓을 수 없는 칸이면 null. */
  const cellAt = (x: number, y: number): ButtonKeyName | null => {
    // `data-key`는 버튼 칸에만 있다 (시스템 키 칸에는 없다)
    for (const el of container.current?.querySelectorAll<HTMLElement>("[data-key]") ?? []) {
      const r = el.getBoundingClientRect();
      if (x >= r.left && x < r.right && y >= r.top && y < r.bottom) {
        const key = el.dataset.key as ButtonKeyName;
        return key === blocked ? null : key;
      }
    }
    return null;
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const el = (e.target as HTMLElement).closest<HTMLElement>("[data-draggable]");
    const key = el?.dataset.key as ButtonKeyName | undefined;
    if (!el || !key || !page.buttons[key]) return;
    // 누르는 순간 포인터를 잡는다: 문턱을 넘기 전에 포인터가 그리드를 벗어나도 손 뗌을 놓치지 않는다.
    // (칸은 안에 누를 것이 없는 버튼 하나라 클릭이 엉뚱한 곳으로 가지 않는다. 탭은 안에 단추가 있어 이렇게 못 한다.)
    // 남은 상태가 있으면 덮어쓴다.
    el.setPointerCapture(e.pointerId);
    setState({
      from: key,
      pointerId: e.pointerId,
      el,
      startX: e.clientX,
      startY: e.clientY,
      rect: el.getBoundingClientRect(),
      started: false,
      cancelled: false,
      target: null,
      x: e.clientX,
      y: e.clientY,
    });
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const s = ref.current;
    if (!s || e.pointerId !== s.pointerId || s.cancelled) return;
    // 왼쪽 버튼이 눌려 있지 않은데 상태가 남았다면(손 뗌을 놓침) 버린다
    if ((e.buttons & 1) === 0) {
      setState(null);
      return;
    }
    const started = s.started || Math.hypot(e.clientX - s.startX, e.clientY - s.startY) > DRAG_THRESHOLD;
    if (!started) return;
    const over = cellAt(e.clientX, e.clientY);
    setState({ ...s, started, x: e.clientX, y: e.clientY, target: over !== s.from ? over : null });
  };

  const finish = (e: React.PointerEvent, drop: boolean) => {
    const s = ref.current;
    if (!s || e.pointerId !== s.pointerId) return;
    setState(null);
    if (!s.started) return;
    justDragged.current = true;
    // 클릭이 따라오지 않는 경우(칸 밖에서 놓음)에도 다음 클릭은 받게
    window.setTimeout(() => (justDragged.current = false), 0);
    if (!drop || s.cancelled) return;
    const to = cellAt(e.clientX, e.clientY);
    if (to && to !== s.from) onMove(s.from, to);
  };

  const onPointerUp = (e: React.PointerEvent) => finish(e, true);
  // 포인터를 잃었거나 시스템이 끌기를 취소했다: 옮기지 않는다
  const onPointerCancel = (e: React.PointerEvent) => finish(e, false);

  const onClickCapture = (e: React.MouseEvent) => {
    if (justDragged.current) {
      justDragged.current = false;
      e.stopPropagation();
      e.preventDefault();
    }
  };

  // Esc: 끄는 중이면 취소. 손을 뗄 때까지 상태는 남겨 뒤따르는 클릭을 막는다.
  const dragging = state?.started === true && !state.cancelled;
  useEffect(() => {
    if (!dragging) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      const s = ref.current;
      if (s) setState({ ...s, cancelled: true, target: null });
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [dragging]);

  // 끄는 사이 그 버튼이 사라졌으면(되돌리기·밖에서 고침 등) 끌기를 취소한다. 상태는 손을 뗄 때까지 남겨
  // 뒤따르는 클릭이 원래 칸을 고르지 않게 한다.
  useEffect(() => {
    const s = ref.current;
    if (s && !s.cancelled && !page.buttons[s.from]) setState({ ...s, cancelled: true, target: null });
  }, [page]);

  const active = state?.started && !state.cancelled ? state : null;
  const ghost: Ghost | null = active
    ? {
        key: active.from,
        button: page.buttons[active.from],
        width: active.rect.width,
        height: active.rect.height,
        // 누른 자리와 키캡의 상대 위치를 유지한다
        x: active.rect.left + active.x - active.startX,
        y: active.rect.top + active.y - active.startY,
      }
    : null;

  return { container, active, ghost, onPointerDown, onPointerMove, onPointerUp, onPointerCancel, onClickCapture };
}
