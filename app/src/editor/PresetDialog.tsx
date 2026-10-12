import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Page } from "@/bindings/Page";
import type { Preset } from "@/bindings/Preset";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { buttonIcon, Icon } from "@/shared/icons";
import { SLOTS } from "@/shared/layout";

let cache: Promise<Preset[]> | null = null;

/** 프리셋 목록. 언어 선택은 3-6에서 붙인다. */
export function listPresets(): Promise<Preset[]> {
  cache ??= invoke<Preset[]>("list_presets", { lang: "ko" });
  return cache;
}

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onAdd: (page: Page) => void;
}

/** 프리셋에서 페이지 추가: 왼쪽 목록, 오른쪽 미리보기. */
export function PresetDialog({ open, onOpenChange, onAdd }: Props) {
  const t = useT().editor.presets;
  const [presets, setPresets] = useState<Preset[]>([]);
  const [selected, setSelected] = useState(0);

  useEffect(() => {
    if (open) void listPresets().then(setPresets);
  }, [open]);

  const preset = presets[selected];

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t.title}</DialogTitle>
          <DialogDescription>{t.body}</DialogDescription>
        </DialogHeader>
        <div className="flex gap-4">
          <ul className="w-48 shrink-0 space-y-1">
            {presets.map((p, i) => (
              <li key={p.id}>
                <button
                  onClick={() => setSelected(i)}
                  className={cn(
                    "w-full rounded-lg px-3 py-2 text-left text-sm",
                    i === selected ? "bg-blue-500/10 font-medium text-blue-700" : "hover:bg-muted",
                  )}
                >
                  {p.name}
                </button>
              </li>
            ))}
          </ul>
          {preset && (
            <div className="min-w-0 flex-1 space-y-3">
              <p className="text-sm text-muted-foreground">{preset.description}</p>
              <MiniGrid page={preset.page} />
              <div className="flex justify-end">
                <Button onClick={() => onAdd(preset.page)}>{t.add(preset.name)}</Button>
              </div>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

/** 프리셋 미리보기용 작은 넘패드 */
function MiniGrid({ page }: { page: Page }) {
  const t = useT();
  return (
    <div className="grid grid-cols-4 grid-rows-[repeat(5,52px)] gap-1.5">
      {SLOTS.map((slot) => {
        if (slot.kind === "mode") {
          return <div key="mode" style={{ gridArea: slot.area }} className="rounded-lg border border-dashed" />;
        }
        if (slot.kind === "system") {
          return (
            <div
              key={slot.key}
              style={{ gridArea: slot.area }}
              className="flex items-center justify-center rounded-lg bg-muted text-xs text-muted-foreground"
            >
              {t.system[slot.key]}
            </div>
          );
        }
        const button = page.buttons[slot.key];
        return (
          <div
            key={slot.key}
            style={{ gridArea: slot.area }}
            className={cn(
              "flex min-w-0 flex-col overflow-hidden rounded-lg px-2 py-1.5",
              button ? "border bg-card" : "border border-dashed",
            )}
          >
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
  );
}
