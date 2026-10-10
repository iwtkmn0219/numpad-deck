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
  const pages = config.pages;

  const addPage = () => {
    update((c) => {
      c.pages.push({ name: t.newPageName, buttons: {} });
    });
    onSelect(pages.length);
    setRenaming(pages.length);
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
          className={cn(
            "group flex shrink-0 items-center rounded-t-lg border border-b-0 text-sm",
            i === page ? "bg-background font-semibold" : "bg-muted/60 text-muted-foreground hover:bg-muted",
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
              title={p.name}
            >
              <span className="mr-1.5 text-xs text-muted-foreground tabular-nums">{i + 1}</span>
              {p.name}
            </button>
          )}
          <DropdownMenu>
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
      <Button variant="ghost" size="sm" className="mb-0.5 shrink-0" onClick={addPage}>
        <Plus /> {t.addPage}
      </Button>

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
