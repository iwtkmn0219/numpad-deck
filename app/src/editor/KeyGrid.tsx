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
}

/** 실제 넘패드 모양 그리드. 칸을 누르면 오른쪽 패널에서 편집한다. 받침 위에 키캡처럼 그린다. */
export function KeyGrid({ grid, page, selected, onSelect }: Props) {
  const t = useT();

  return (
    <div className="rounded-[18px] bg-muted p-3.5">
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
          return (
            <button
              key={slot.key}
              style={{ gridArea: slot.area }}
              onClick={() => onSelect({ kind: "button", key: slot.key })}
              className={cn(
                "flex min-w-0 flex-col items-start overflow-hidden rounded-xl px-3 pt-2 pb-[11px] text-left",
                button
                  ? "keycap"
                  : "border-[1.5px] border-dashed border-input text-muted-foreground opacity-85 transition hover:bg-white/50 hover:opacity-100",
                isSelected && "key-selected",
              )}
            >
              <span className="flex w-full items-start justify-between">
                <KeyNumber label={slot.key} selected={isSelected} />
                {button && <Icon name={buttonIcon(button)} className="size-5 text-foreground" />}
              </span>
              {button ? (
                <span className="mt-auto line-clamp-2 w-full text-[13.5px] font-medium break-keep [overflow-wrap:anywhere]">
                  {button.label}
                </span>
              ) : (
                <span className="mt-auto flex items-center gap-1 text-[13px] font-medium">
                  <Plus className="size-3.5" />
                  {t.editor.grid.empty}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
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
