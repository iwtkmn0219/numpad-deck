import { useRef, useState } from "react";
import { ChevronDown, Plus } from "lucide-react";
import type { Config } from "@/bindings/Config";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { cn } from "@/lib/utils";
import { useT } from "@/i18n";
import type { Page } from "@/bindings/Page";
import { PresetDialog } from "./PresetDialog";

interface Props {
  config: Config;
  page: number;
  onSelect: (page: number) => void;
  update: (recipe: (draft: Config) => void, coalesce?: string) => void;
}

/** 페이지 탭: 고르기, 추가, 이름 바꾸기(두 번 클릭), 순서 바꾸기, 삭제. */
export function PageTabs({ config, page, onSelect, update }: Props) {
  const t = useT().editor;
  const [renaming, setRenaming] = useState<number | null>(null);
  const [deleting, setDeleting] = useState<number | null>(null);
  const [presetOpen, setPresetOpen] = useState(false);
  /** 메뉴가 열린 탭 (▾ 또는 오른쪽 클릭) */
  const [menuFor, setMenuFor] = useState<number | null>(null);
  const pages = config.pages;
  const drag = useTabDrag(pages.length, (from, to) => move(from, to));

  const addPage = () => {
    update((c) => {
      c.pages.push({ name: t.newPageName, buttons: {} });
    });
    onSelect(pages.length);
    setRenaming(pages.length);
  };

  const addPresetPage = (page: Page) => {
    setPresetOpen(false);
    update((c) => {
      c.pages.push(structuredClone(page));
    });
    onSelect(pages.length);
  };

  const move = (from: number, to: number) => {
    if (to < 0 || to >= pages.length) return;
    update((c) => {
      const [p] = c.pages.splice(from, 1);
      c.pages.splice(to, 0, p);
    });
    onSelect(to);
  };

  const remove = (index: number) => {
    update((c) => {
      c.pages.splice(index, 1);
    });
    onSelect(Math.max(0, Math.min(page, pages.length - 2)));
  };

  const target = deleting !== null ? pages[deleting] : null;

  return (
    <div className="flex items-center gap-1 overflow-x-auto border-b px-4 pt-3">
      {pages.map((p, i) => (
        <div
          key={i}
          ref={(el) => void (drag.tabs.current[i] = el)}
          onPointerDown={(e) => renaming !== i && drag.onPointerDown(e, i)}
          onPointerMove={drag.onPointerMove}
          onPointerUp={drag.onPointerUp}
          onPointerCancel={drag.onPointerUp}
          onClickCapture={drag.onClickCapture}
          style={drag.style(i)}
          className={cn(
            "group relative flex shrink-0 items-center rounded-t-lg border border-b-0 text-sm",
            i === page ? "bg-background font-semibold" : "bg-muted/60 text-muted-foreground hover:bg-muted",
            drag.active === i && "z-10 shadow-md",
          )}
        >
          {renaming === i ? (
            <Input
              autoFocus
              defaultValue={p.name}
              className="h-8 w-36 rounded-b-none border-0 shadow-none focus-visible:ring-0"
              onFocus={(e) => e.currentTarget.select()}
              onBlur={(e) => {
                const name = e.currentTarget.value.trim();
                if (name && name !== p.name) update((c) => void (c.pages[i].name = name));
                setRenaming(null);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
                if (e.key === "Escape") setRenaming(null);
              }}
            />
          ) : (
            <button
              className="h-8 max-w-48 truncate pr-1 pl-3"
              onClick={() => onSelect(i)}
              onDoubleClick={() => setRenaming(i)}
              onContextMenu={(e) => {
                e.preventDefault();
                onSelect(i);
                setMenuFor(i);
              }}
              title={p.name}
            >
              <span className="mr-1.5 text-xs text-muted-foreground tabular-nums">{i + 1}</span>
              {p.name}
            </button>
          )}
          <DropdownMenu open={menuFor === i} onOpenChange={(open) => setMenuFor(open ? i : null)}>
            <DropdownMenuTrigger asChild>
              <button
                data-no-drag
                className="mr-1 rounded p-0.5 text-muted-foreground opacity-60 group-hover:opacity-100 hover:bg-muted"
                aria-label={p.name}
              >
                <ChevronDown className="size-3.5" />
              </button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuItem onSelect={() => setRenaming(i)}>{t.pageMenu.rename}</DropdownMenuItem>
              <DropdownMenuItem disabled={i === 0} onSelect={() => move(i, i - 1)}>
                {t.pageMenu.moveLeft}
              </DropdownMenuItem>
              <DropdownMenuItem disabled={i === pages.length - 1} onSelect={() => move(i, i + 1)}>
                {t.pageMenu.moveRight}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem
                variant="destructive"
                disabled={pages.length <= 1}
                onSelect={() => setDeleting(i)}
                title={pages.length <= 1 ? t.deletePage.lastPage : undefined}
              >
                {t.pageMenu.delete}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ))}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm" className="mb-0.5 shrink-0">
            <Plus /> {t.addPage}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          <DropdownMenuItem onSelect={addPage}>{t.addEmptyPage}</DropdownMenuItem>
          <DropdownMenuItem onSelect={() => setPresetOpen(true)}>{t.addPresetPage}</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      <PresetDialog open={presetOpen} onOpenChange={setPresetOpen} onAdd={addPresetPage} layout={config.layout} systemKeys={config.systemKeys} />

      <AlertDialog open={target !== null} onOpenChange={(open) => !open && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{target && t.deletePage.title(target.name)}</AlertDialogTitle>
            <AlertDialogDescription>
              {target && t.deletePage.body(Object.keys(target.buttons).length)}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t.deletePage.cancel}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => deleting !== null && remove(deleting)}>
              {t.deletePage.confirm}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

/** 끌기로 보는 거리(px). 이보다 적게 움직이면 그냥 클릭이다. */
const DRAG_THRESHOLD = 5;

interface DragState {
  from: number;
  pointerId: number;
  startX: number;
  dx: number;
  started: boolean;
  /** 끌기 시작할 때 잰 탭 위치 */
  rects: DOMRect[];
}

/**
 * 페이지 탭 끌어서 순서 바꾸기. 끄는 탭은 좌우로만 움직이고(탭 줄 양 끝까지), 다른 탭들이
 * 비켜서 들어갈 자리를 보인다. HTML 끌어 놓기는 끄는 모양이 마우스를 자유롭게 따라다녀서 쓰지 않는다.
 */
function useTabDrag(count: number, onMove: (from: number, to: number) => void) {
  const tabs = useRef<(HTMLDivElement | null)[]>([]);
  const [state, setState] = useState<DragState | null>(null);
  /** 끌기를 막 끝냈으면 뒤따르는 클릭(탭 고르기)을 무시한다 */
  const justDragged = useRef(false);

  /** 끄는 탭 가운데가 지나간 다른 탭 수로 들어갈 자리를 정한다 */
  const targetIndex = (s: DragState) => {
    const r = s.rects[s.from];
    const center = r.left + r.width / 2 + s.dx;
    let to = s.from;
    s.rects.forEach((o, j) => {
      const c = o.left + o.width / 2;
      if (j > s.from && center > c) to = j;
      if (j < s.from && center < c && to === s.from) to = j;
    });
    return to;
  };

  const onPointerDown = (e: React.PointerEvent, i: number) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("[data-no-drag]")) return;
    const rects = tabs.current.slice(0, count).map((el) => el!.getBoundingClientRect());
    setState({ from: i, pointerId: e.pointerId, startX: e.clientX, dx: 0, started: false, rects });
  };

  const onPointerMove = (e: React.PointerEvent) => {
    if (!state || e.pointerId !== state.pointerId) return;
    let dx = e.clientX - state.startX;
    const started = state.started || Math.abs(dx) > DRAG_THRESHOLD;
    if (!started) return;
    if (!state.started) e.currentTarget.setPointerCapture(e.pointerId);
    // 탭 줄 양 끝을 넘지 않게
    const r = state.rects[state.from];
    const first = state.rects[0];
    const last = state.rects[state.rects.length - 1];
    dx = Math.max(first.left - r.left, Math.min(last.right - r.right, dx));
    setState({ ...state, dx, started });
  };

  const onPointerUp = (e: React.PointerEvent) => {
    if (!state || e.pointerId !== state.pointerId) return;
    if (state.started) {
      justDragged.current = true;
      // 클릭이 따라오지 않는 경우(탭 밖에서 놓음)에도 다음 클릭은 받게
      window.setTimeout(() => (justDragged.current = false), 0);
      const to = targetIndex(state);
      if (to !== state.from) onMove(state.from, to);
    }
    setState(null);
  };

  const onClickCapture = (e: React.MouseEvent) => {
    if (justDragged.current) {
      justDragged.current = false;
      e.stopPropagation();
      e.preventDefault();
    }
  };

  /** 끄는 탭은 마우스를 따라 좌우로, 사이에 있는 탭은 한 칸 비켜선다 */
  const style = (i: number): React.CSSProperties | undefined => {
    if (!state?.started) return undefined;
    if (i === state.from) return { transform: `translateX(${state.dx}px)` };
    const to = targetIndex(state);
    const r = state.rects[state.from];
    // 비켜서는 거리 = 끄는 탭 너비 + 탭 사이 간격. 간격은 옆 탭과 잰다 (맨 오른쪽 탭이면 왼쪽 탭과).
    const after = state.rects[state.from + 1];
    const before = state.rects[state.from - 1];
    const gap = after ? after.left - r.right : before ? r.left - before.right : 0;
    const step = r.width + gap;
    let shift = 0;
    if (state.from < i && i <= to) shift = -step;
    if (to <= i && i < state.from) shift = step;
    return { transform: `translateX(${shift}px)`, transition: "transform 150ms ease" };
  };

  return {
    tabs,
    active: state?.started ? state.from : null,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onClickCapture,
    style,
  };
}
