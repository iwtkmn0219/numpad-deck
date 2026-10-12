import type { ButtonKeyName } from "@/bindings/ButtonKeyName";
import type { Page } from "@/bindings/Page";
import { gridStyle, type Grid, type SystemKey } from "@/shared/layout";
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

/** 실제 넘패드 모양 그리드. 칸을 누르면 오른쪽 패널에서 편집한다. */
export function KeyGrid({ grid, page, selected, onSelect }: Props) {
  const t = useT();

  return (
    <div className="gap-2" style={gridStyle(grid, 84)}>
      {grid.slots.map((slot) => {
        if (slot.kind === "mode") {
          return (
            <div
              key="mode"
              style={{ gridArea: slot.area }}
              className="flex flex-col items-center justify-center rounded-xl border border-dashed text-xs text-muted-foreground"
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
                "flex flex-col items-start rounded-xl bg-muted px-3 py-2 text-left text-muted-foreground transition-colors",
                isSelected ? "ring-2 ring-ring" : "hover:bg-muted/70",
              )}
            >
              <span className="text-[11px]">{slot.key}</span>
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
              "flex min-w-0 flex-col items-start overflow-hidden rounded-xl px-3 py-2 text-left transition-colors",
              button
                ? "border bg-card shadow-xs hover:border-foreground/30"
                : "border border-dashed text-muted-foreground hover:border-foreground/30",
              isSelected && "border-blue-500 ring-2 ring-blue-500/40",
            )}
          >
            <span className="flex w-full items-start justify-between">
              <span className="text-[11px] text-muted-foreground">{slot.key}</span>
              {button && <Icon name={buttonIcon(button)} className="size-5 text-foreground/80" />}
            </span>
            <span className="mt-auto line-clamp-2 w-full text-sm font-medium break-keep [overflow-wrap:anywhere]">
              {button ? button.label : t.editor.grid.empty}
            </span>
          </button>
        );
      })}
    </div>
  );
}
