import { useEffect, useRef, useState } from "react";
import type { Config } from "@/bindings/Config";
import type { KeyPlace } from "@/bindings/KeyPlace";
import type { LayoutKeyName } from "@/bindings/LayoutKeyName";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { gridFor, isStandard, LAYOUT_MAX, normalized, sameLayout, STANDARD_LAYOUT } from "@/shared/layout";
import { pauseDeckForwarding } from "./api";
import { numpadLabel } from "./keymap";

/** 칸 한 변(px)과 칸 사이 간격 */
const CELL = 44;
const GAP = 4;
const STEP = CELL + GAP;

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  config: Config;
  update: (recipe: (draft: Config) => void) => void;
}

interface Drag {
  key: LayoutKeyName;
  /** 블록 왼쪽 위에서 잡은 곳까지 (px) */
  offX: number;
  offY: number;
  x: number;
  y: number;
  ok: boolean;
}

/** 다른 키와 겹치지 않고 칸 안에 들어가는가 */
function fits(places: KeyPlace[], key: string, x: number, y: number, w: number, h: number): boolean {
  if (x < 0 || y < 0 || x + w > LAYOUT_MAX || y + h > LAYOUT_MAX) return false;
  return places.every(
    (p) => p.key === key || x + w <= p.x || p.x + p.w <= x || y + h <= p.y || p.y + p.h <= y,
  );
}

/**
 * 화면 속 넘패드 모양을 내 키보드에 맞추기: 키를 끌어 옮기고, 가로 2칸·세로 2줄로 바꾼다.
 * 모양만 바뀌고 키가 하는 일은 그대로다. 넘패드 키를 누르면 그 키가 선택된다.
 */
export function LayoutEditor({ open, onOpenChange, config, update }: Props) {
  const t = useT().editor.layout;
  const saved = config.layout ?? STANDARD_LAYOUT;
  // 편집 중인 배치 (판 위 좌표). 열 때 판 가운데에 놓아 사방에 옮길 자리를 둔다.
  // 저장은 왼쪽 위로 붙여서 한다. 되돌리기 등으로 저장된 모양이 바뀌면 다시 가운데에 놓는다.
  const [places, setPlaces] = useState<KeyPlace[]>(() => centered(saved));
  const [selected, setSelected] = useState<LayoutKeyName | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const canvas = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) setPlaces((current) => (sameLayout(current, saved) ? current : centered(saved)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, JSON.stringify(saved)]);

  const commit = (next: KeyPlace[]) => {
    setPlaces(next);
    update((c) => {
      if (isStandard(next)) delete c.layout;
      else c.layout = normalized(next);
    });
  };

  // 열려 있는 동안 넘패드 키는 버튼을 실행하지 않고 그 키를 고른다
  useEffect(() => {
    if (!open) return;
    const resume = pauseDeckForwarding();
    const onKey = (e: KeyboardEvent) => {
      const key = e.code === "NumLock" ? "NumLock" : numpadLabel(e.code);
      if (!key) return;
      if (e.code !== "NumLock") e.preventDefault();
      if (e.type === "keydown") {
        setSelected(key as LayoutKeyName);
        setNotice(null);
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKey);
    return () => {
      resume();
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKey);
    };
  }, [open]);

  const cellAt = (e: React.PointerEvent, d: Pick<Drag, "offX" | "offY">) => {
    const rect = canvas.current!.getBoundingClientRect();
    return {
      x: Math.round((e.clientX - rect.left - d.offX) / STEP),
      y: Math.round((e.clientY - rect.top - d.offY) / STEP),
    };
  };

  const onDown = (e: React.PointerEvent, p: KeyPlace) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    const rect = e.currentTarget.getBoundingClientRect();
    setSelected(p.key);
    setNotice(null);
    setDrag({ key: p.key, offX: e.clientX - rect.left, offY: e.clientY - rect.top, x: p.x, y: p.y, ok: true });
  };

  const onMove = (e: React.PointerEvent, p: KeyPlace) => {
    if (!drag || drag.key !== p.key) return;
    const { x, y } = cellAt(e, drag);
    if (x !== drag.x || y !== drag.y) setDrag({ ...drag, x, y, ok: fits(places, p.key, x, y, p.w, p.h) });
  };

  const onUp = (p: KeyPlace) => {
    if (!drag || drag.key !== p.key) return;
    if (drag.ok && (drag.x !== p.x || drag.y !== p.y)) {
      commit(places.map((q) => (q.key === p.key ? { ...q, x: drag.x, y: drag.y } : q)));
    } else if (!drag.ok) {
      setNotice(t.blocked);
    }
    setDrag(null);
  };

  const current = places.find((p) => p.key === selected) ?? null;
  const resize = (w: number, h: number) => {
    if (!current) return;
    if (!fits(places, current.key, current.x, current.y, w, h)) {
      setNotice(t.blocked);
      return;
    }
    setNotice(null);
    commit(places.map((q) => (q.key === current.key ? { ...q, w, h } : q)));
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t.title}</DialogTitle>
          <DialogDescription>{t.body}</DialogDescription>
        </DialogHeader>
        <div className="flex gap-5">
          <div
            ref={canvas}
            className="relative shrink-0 rounded-lg bg-[radial-gradient(circle,var(--color-border)_1.5px,transparent_1.5px)]"
            style={{
              width: LAYOUT_MAX * STEP - GAP,
              height: LAYOUT_MAX * STEP - GAP,
              backgroundSize: `${STEP}px ${STEP}px`,
              backgroundPosition: `${CELL / 2}px ${CELL / 2}px`,
            }}
          >
            {drag && (
              <div
                className={cn(
                  "pointer-events-none absolute rounded-lg border-2 border-dashed",
                  drag.ok ? "border-blue-500 bg-blue-500/10" : "border-red-500 bg-red-500/10",
                )}
                style={box(drag.x, drag.y, places.find((p) => p.key === drag.key)!)}
              />
            )}
            {places.map((p) => (
              <button
                key={p.key}
                onPointerDown={(e) => onDown(e, p)}
                onPointerMove={(e) => onMove(e, p)}
                onPointerUp={() => onUp(p)}
                className={cn(
                  "absolute flex touch-none items-center justify-center rounded-lg border text-sm font-medium select-none",
                  p.key === selected ? "border-blue-500 bg-blue-500 text-white" : "bg-card hover:bg-muted",
                  drag?.key === p.key ? "cursor-grabbing opacity-60" : "cursor-grab",
                )}
                style={box(p.x, p.y, p)}
              >
                {p.key === "NumLock" ? "Num" : p.key}
              </button>
            ))}
          </div>

          <div className="min-w-0 flex-1 space-y-3 text-sm">
            {current ? (
              <>
                <p className="font-medium">{t.selected(current.key === "NumLock" ? "NumLock" : current.key)}</p>
                <div className="flex flex-col gap-2">
                  <Button
                    variant={current.w === 2 ? "default" : "outline"}
                    size="sm"
                    onClick={() => resize(current.w === 2 ? 1 : 2, current.h)}
                  >
                    {t.wide}
                  </Button>
                  <Button
                    variant={current.h === 2 ? "default" : "outline"}
                    size="sm"
                    onClick={() => resize(current.w, current.h === 2 ? 1 : 2)}
                  >
                    {t.tall}
                  </Button>
                </div>
              </>
            ) : (
              <p className="text-muted-foreground">{t.pickHint}</p>
            )}
            {notice && <p className="text-destructive">{notice}</p>}
          </div>
        </div>
        <div className="flex justify-between">
          <Button
            variant="outline"
            disabled={!config.layout}
            onClick={() => {
              setNotice(null);
              commit(centered(STANDARD_LAYOUT));
            }}
          >
            {t.reset}
          </Button>
          <Button onClick={() => onOpenChange(false)}>{t.done}</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** 판 가운데에 놓은 배치 */
function centered(layout: KeyPlace[]): KeyPlace[] {
  const { cols, rows } = gridFor(layout);
  return normalized(layout, Math.floor((LAYOUT_MAX - cols) / 2), Math.floor((LAYOUT_MAX - rows) / 2));
}

function box(x: number, y: number, p: Pick<KeyPlace, "w" | "h">): React.CSSProperties {
  return { left: x * STEP, top: y * STEP, width: p.w * STEP - GAP, height: p.h * STEP - GAP };
}
