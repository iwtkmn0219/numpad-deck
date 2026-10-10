import { Trash2 } from "lucide-react";
import type { Config } from "@/bindings/Config";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useT } from "@/i18n";
import { describeAction } from "./describe";
import type { Selection } from "./KeyGrid";

interface Props {
  config: Config;
  page: number;
  selected: Selection | null;
  update: (recipe: (draft: Config) => void, coalesce?: string) => void;
}

/** 오른쪽 패널: 고른 버튼의 이름·동작 편집. 동작 편집은 3-2에서 채운다. */
export function ButtonPanel({ config, page, selected, update }: Props) {
  const t = useT();
  const p = t.editor.panel;

  if (!selected) {
    return <p className="text-sm text-muted-foreground">{p.pickHint}</p>;
  }

  if (selected.kind === "system") {
    return (
      <div className="space-y-2">
        <h2 className="text-base font-semibold">{p.systemTitle(t.system[selected.key])}</h2>
        <p className="text-sm text-muted-foreground">{p.systemBody}</p>
      </div>
    );
  }

  const key = selected.key;
  const button = config.pages[page]?.buttons[key];

  if (!button) {
    return (
      <div className="space-y-2">
        <h2 className="text-base font-semibold">{p.emptyTitle(key)}</h2>
        <p className="text-sm text-muted-foreground">{p.comingSoon}</p>
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <h2 className="text-base font-semibold">{p.buttonTitle(key)}</h2>

      <div className="space-y-1.5">
        <Label htmlFor="button-label">{p.label}</Label>
        <Input
          id="button-label"
          // 되돌리기 결과가 바로 보이도록 항상 현재 값을 보여준다
          value={button.label}
          placeholder={p.labelPlaceholder}
          onChange={(e) => {
            const label = e.currentTarget.value;
            update((c) => {
              const b = c.pages[page].buttons[key];
              if (b) b.label = label;
            }, `label:${page}:${key}`);
          }}
        />
      </div>

      <div className="space-y-1.5">
        <Label>{p.action}</Label>
        <p className="rounded-lg border bg-muted/40 px-3 py-2 text-sm">{describeAction(button.action, t)}</p>
        <p className="text-xs text-muted-foreground">{p.comingSoon}</p>
      </div>

      <Button
        variant="outline"
        size="sm"
        className="text-destructive"
        onClick={() =>
          update((c) => {
            delete c.pages[page].buttons[key];
          })
        }
      >
        <Trash2 /> {p.clear}
      </Button>
    </div>
  );
}
