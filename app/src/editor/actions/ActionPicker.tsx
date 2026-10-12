import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { KIND_ICONS, KINDS, type Kind } from "./meta";

interface Props {
  onPick: (kind: Kind) => void;
  /** 묶음 안에서 고를 때: 묶음은 빼고 기다리기를 넣는다. */
  inSequence?: boolean;
  compact?: boolean;
}

/** 동작 종류 카드 목록. */
export function ActionPicker({ onPick, inSequence = false, compact = false }: Props) {
  const t = useT().editor.kinds;
  const kinds: Kind[] = inSequence ? [...KINDS.filter((k) => k !== "sequence"), "delay"] : KINDS;

  return (
    <div className={cn("grid gap-2", compact ? "grid-cols-3" : "grid-cols-2")}>
      {kinds.map((kind) => {
        const Icon = KIND_ICONS[kind];
        return (
          <button
            key={kind}
            onClick={() => onPick(kind)}
            className="flex items-start gap-3 rounded-xl border bg-card p-3 text-left transition-colors hover:border-foreground/30 hover:bg-muted/40"
          >
            <span className="mt-0.5 rounded-lg bg-selection-soft p-1.5 text-selection">
              <Icon className="size-4" />
            </span>
            <span className="min-w-0">
              <span className="block text-sm font-medium">{t[kind].title}</span>
              {!compact && <span className="block text-xs text-muted-foreground">{t[kind].body}</span>}
            </span>
          </button>
        );
      })}
    </div>
  );
}
