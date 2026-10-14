import { useEffect, useState } from "react";
import { TriangleAlert } from "lucide-react";
import type { ButtonNote } from "@/bindings/ButtonNote";
import type { Config } from "@/bindings/Config";
import type { InstallMode } from "@/bindings/InstallMode";
import type { Page } from "@/bindings/Page";
import type { Preview } from "@/bindings/Preview";
import type { KeyPlace } from "@/bindings/KeyPlace";
import type { SystemKeys } from "@/bindings/SystemKeys";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { buttonIcon, Icon } from "@/shared/icons";
import { BUTTON_KEYS, gridFor, gridStyle } from "@/shared/layout";
import { configPath, installShare, parentFolder } from "../api";
import { describeAction } from "../describe";

interface Props {
  preview: Preview;
  config: Config;
  onClose: () => void;
  /** 설치가 끝났다. `count`는 넣은 페이지 수, `backup`은 "바꾸기"일 때 자동 백업 경로. */
  onInstalled: (mode: InstallMode, count: number, backup: string | null) => void;
}

/**
 * 공유 파일 가져오기: 왼쪽 페이지 목록(체크), 오른쪽 고른 페이지 미리 보기.
 * 미리 보기가 바뀌면(다른 파일을 또 엶) 부모가 `key`로 새로 만든다.
 */
export function ImportDialog({ preview, config, onClose, onInstalled }: Props) {
  const tt = useT();
  const t = tt.editor.share;
  const [checked, setChecked] = useState<boolean[]>(() => preview.pages.map(() => true));
  const [focus, setFocus] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmReplace, setConfirmReplace] = useState(false);
  const [backupFolder, setBackupFolder] = useState<string | null>(null);

  const isBackup = preview.kind === "backup";
  useEffect(() => {
    if (isBackup) void configPath().then((p) => setBackupFolder(`${parentFolder(p)}\\backups`));
  }, [isBackup]);

  const picked = checked.flatMap((c, i) => (c ? [i] : []));
  const allChecked = picked.length === preview.pages.length;
  const current = preview.pages[focus];

  const install = async (mode: InstallMode) => {
    setBusy(true);
    setError(null);
    try {
      const backup = await installShare({ previewId: preview.id, pages: picked, mode });
      onInstalled(mode, picked.length, backup);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => !open && !busy && onClose()}>
      <DialogContent className="sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>{isBackup ? t.import.titleBackup : t.import.titlePages}</DialogTitle>
          <DialogDescription>{t.import.body(preview.app)}</DialogDescription>
        </DialogHeader>

        {preview.anyCaution && (
          <p className="flex items-start gap-2 rounded-lg border border-amber-500/30 bg-amber-50 px-3 py-2 text-sm text-amber-900">
            <TriangleAlert className="mt-0.5 size-4 shrink-0 text-amber-600" />
            {t.import.caution}
          </p>
        )}

        <div className="flex min-h-0 gap-4">
          <div className="w-52 shrink-0 space-y-1">
            <label className="flex cursor-pointer items-center gap-2 px-3 py-1.5 text-sm text-muted-foreground">
              <input
                type="checkbox"
                className="size-4 accent-blue-600"
                checked={allChecked}
                onChange={(e) => setChecked(preview.pages.map(() => e.target.checked))}
              />
              {t.import.selectAll}
            </label>
            <ul className="max-h-[420px] space-y-1 overflow-y-auto">
              {preview.pages.map((p, i) => (
                <li
                  key={i}
                  className={cn(
                    "flex items-center gap-2 rounded-lg px-3 py-2 text-sm",
                    i === focus ? "bg-selection-soft font-medium text-selection-foreground" : "hover:bg-muted",
                  )}
                >
                  <input
                    type="checkbox"
                    className="size-4 shrink-0 accent-blue-600"
                    checked={checked[i]}
                    aria-label={p.page.name}
                    onChange={(e) => setChecked((c) => c.map((v, j) => (j === i ? e.target.checked : v)))}
                  />
                  <button className="min-w-0 flex-1 text-left" onClick={() => setFocus(i)}>
                    <span className="block truncate">{p.page.name}</span>
                    <span className="block text-xs font-normal text-muted-foreground">
                      {t.import.buttons(p.notes.length)}
                    </span>
                  </button>
                  {p.notes.some((n) => n.caution) && <TriangleAlert className="size-3.5 shrink-0 text-amber-600" />}
                </li>
              ))}
            </ul>
          </div>

          {current && (
            <div className="min-w-0 flex-1 space-y-3">
              <PreviewGrid
                page={current.page}
                notes={current.notes}
                layout={preview.layout ?? config.layout}
                systemKeys={preview.systemKeys ?? config.systemKeys}
              />
              <ButtonList page={current.page} notes={current.notes} />
            </div>
          )}
        </div>

        {error && <p className="text-sm break-all text-destructive">{error}</p>}

        <DialogFooter>
          <Button variant="ghost" disabled={busy} onClick={onClose}>
            {t.import.close}
          </Button>
          {isBackup && (
            <Button variant="outline" disabled={busy || picked.length === 0} onClick={() => setConfirmReplace(true)}>
              {t.import.replace}
            </Button>
          )}
          <Button disabled={busy || picked.length === 0} onClick={() => void install("add")}>
            {t.import.add(picked.length)}
          </Button>
        </DialogFooter>

        <AlertDialog open={confirmReplace} onOpenChange={setConfirmReplace}>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>{t.replace.title}</AlertDialogTitle>
              <AlertDialogDescription asChild>
                <div className="space-y-2">
                  <p>{t.replace.body(picked.length)}</p>
                  <p className="font-medium text-foreground">{t.replace.backup}</p>
                  {backupFolder && <p className="font-mono text-xs break-all">{t.replace.where(backupFolder)}</p>}
                </div>
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t.replace.cancel}</AlertDialogCancel>
              <Button
                variant="destructive"
                onClick={() => {
                  setConfirmReplace(false);
                  void install("replace");
                }}
              >
                {t.replace.confirm}
              </Button>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </DialogContent>
    </Dialog>
  );
}

/** 키캡 미리 보기. 주의 버튼은 모서리에 ⚠, 이 PC에 없는 버튼은 흐리게. */
function PreviewGrid({
  page,
  notes,
  layout,
  systemKeys,
}: {
  page: Page;
  notes: ButtonNote[];
  layout?: KeyPlace[];
  systemKeys?: SystemKeys;
}) {
  const t = useT();
  const grid = gridFor(layout, systemKeys);
  const noteFor = (key: string) => notes.find((n) => n.key === key);
  return (
    <div className="rounded-[14px] bg-muted p-2.5">
      <div className="gap-1.5" style={gridStyle(grid, 52)}>
        {grid.slots.map((slot) => {
          if (slot.kind === "mode") {
            return <div key="mode" style={{ gridArea: slot.area }} />;
          }
          if (slot.kind === "system") {
            return (
              <div
                key={slot.key}
                style={{ gridArea: slot.area }}
                className="flex items-center justify-center rounded-lg border border-key-edge text-xs text-muted-foreground"
              >
                {t.system[slot.role]}
              </div>
            );
          }
          const button = page.buttons[slot.key];
          const note = noteFor(slot.key);
          return (
            <div
              key={slot.key}
              style={{ gridArea: slot.area }}
              title={button?.label}
              className={cn(
                "relative flex min-w-0 flex-col overflow-hidden rounded-lg px-2 py-1.5",
                button ? "keycap" : "border border-dashed border-input",
                note?.missing && "opacity-45",
              )}
            >
              {note?.caution && (
                <TriangleAlert className="absolute top-1 right-1 size-3.5 text-amber-600" aria-label={t.editor.share.import.cautionTag} />
              )}
              {button && (
                <>
                  <Icon name={buttonIcon(button)} className="size-4 text-foreground/80" />
                  <span className="mt-auto line-clamp-2 text-xs font-medium break-keep [overflow-wrap:anywhere]">
                    {button.label}
                  </span>
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** 버튼별 한 줄: 키 · 이름 · 하는 일 · [주의] · [이 PC에 없음] */
function ButtonList({ page, notes }: { page: Page; notes: ButtonNote[] }) {
  const tt = useT();
  const t = tt.editor.share.import;
  const keys = BUTTON_KEYS.filter((k) => page.buttons[k]);
  if (keys.length === 0) return <p className="text-sm text-muted-foreground">{t.empty}</p>;
  return (
    <ul className="max-h-48 divide-y overflow-y-auto rounded-lg border text-sm">
      {keys.map((k) => {
        const button = page.buttons[k]!;
        const note = notes.find((n) => n.key === k);
        return (
          <li key={k} className="flex items-center gap-2 px-3 py-1.5">
            <span className="w-11 shrink-0 text-xs text-muted-foreground tabular-nums">{k}</span>
            <span className="w-28 shrink-0 truncate font-medium" title={button.label}>
              {button.label}
            </span>
            <span className="min-w-0 flex-1 truncate text-muted-foreground" title={describeAction(button.action, tt)}>
              {describeAction(button.action, tt)}
            </span>
            {note?.caution && (
              <Badge variant="outline" className="border-amber-500/40 text-amber-800">
                <TriangleAlert /> {t.cautionTag}
              </Badge>
            )}
            {note?.missing && <Badge variant="destructive">{t.missingTag}</Badge>}
          </li>
        );
      })}
    </ul>
  );
}
