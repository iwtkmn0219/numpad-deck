import { useState } from "react";
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
  /** 끌고 있는 탭과, 놓으면 들어갈 자리 표시 (그 탭의 앞/뒤) */
  const [dragging, setDragging] = useState<number | null>(null);
  const [dropAt, setDropAt] = useState<{ index: number; after: boolean } | null>(null);
  /** 메뉴가 열린 탭 (▾ 또는 오른쪽 클릭) */
  const [menuFor, setMenuFor] = useState<number | null>(null);
  const pages = config.pages;

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

  /** 끌어 놓기: 놓은 탭의 왼쪽 절반이면 앞, 오른쪽 절반이면 뒤로 */
  const dragOver = (e: React.DragEvent, i: number) => {
    if (dragging === null) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
    const rect = e.currentTarget.getBoundingClientRect();
    const after = e.clientX > rect.left + rect.width / 2;
    if (dropAt?.index !== i || dropAt.after !== after) setDropAt({ index: i, after });
  };

  const drop = (e: React.DragEvent) => {
    e.preventDefault();
    if (dragging !== null && dropAt) {
      let to = dropAt.after ? dropAt.index + 1 : dropAt.index;
      if (dragging < to) to -= 1;
      if (to !== dragging) move(dragging, to);
    }
    setDragging(null);
    setDropAt(null);
  };

  return (
    <div className="flex items-center gap-1 overflow-x-auto border-b px-4 pt-3">
      {pages.map((p, i) => (
        <div
          key={i}
          draggable={renaming !== i}
          onDragStart={(e) => {
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", String(i));
            setDragging(i);
          }}
          onDragOver={(e) => dragOver(e, i)}
          onDrop={drop}
          onDragEnd={() => {
            setDragging(null);
            setDropAt(null);
          }}
          style={
            dropAt?.index === i && dragging !== null
              ? { boxShadow: `${dropAt.after ? "inset -3px" : "inset 3px"} 0 0 #3b82f6` }
              : undefined
          }
          className={cn(
            "group flex shrink-0 items-center rounded-t-lg border border-b-0 text-sm",
            i === page ? "bg-background font-semibold" : "bg-muted/60 text-muted-foreground hover:bg-muted",
            dragging === i && "opacity-50",
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
