import { useEffect, useRef, useState } from "react";
import { Play, Replace, Square, Trash2, TriangleAlert } from "lucide-react";
import type { Action } from "@/bindings/Action";
import type { Button as DeckButton } from "@/bindings/Button";
import type { ButtonKeyName } from "@/bindings/ButtonKeyName";
import type { Config } from "@/bindings/Config";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useT } from "@/i18n";
import { testAction } from "./api";
import { ActionForm } from "./actions/ActionForm";
import { ActionPicker } from "./actions/ActionPicker";
import { emptyAction, incompleteReason, KIND_ICONS, suggestLabel } from "./actions/meta";
import { IconPickerDialog } from "./IconPickerDialog";
import { buttonIcon, Icon } from "@/shared/icons";
import type { Selection } from "./KeyGrid";

interface Props {
  config: Config;
  page: number;
  selected: Selection | null;
  update: (recipe: (draft: Config) => void, coalesce?: string) => void;
}

/** 오른쪽 패널: 고른 칸의 버튼 만들기·편집. */
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
  return (
    <ButtonEditor
      // 칸이 바뀌면 편집 상태를 새로 시작한다
      key={`${page}:${selected.key}`}
      stored={config.pages[page]?.buttons[selected.key] ?? null}
      page={page}
      buttonKey={selected.key}
      update={update}
    />
  );
}

interface EditorProps {
  stored: DeckButton | null;
  page: number;
  buttonKey: ButtonKeyName;
  update: Props["update"];
}

/**
 * 버튼 하나의 편집. 동작이 완성되지 않은 동안(예: 단축키를 아직 녹화하지 않음)은 화면에만
 * 두고 저장하지 않는다. 완성되면 바로 저장한다.
 */
function ButtonEditor({ stored, page, buttonKey, update }: EditorProps) {
  const t = useT();
  const p = t.editor.panel;
  const [draft, setDraft] = useState<DeckButton | null>(stored);
  const [picking, setPicking] = useState(stored === null);
  const [iconOpen, setIconOpen] = useState(false);
  /** 사용자가 이름을 직접 고쳤는지. 안 고쳤으면 동작에 맞춰 이름을 지어 준다. */
  const labelTouched = useRef(stored !== null && stored.label !== suggestLabel(stored.action, t));
  /** 마지막으로 저장한 버튼. 되돌리기 등으로 저장된 값이 바뀌면 화면을 맞춘다. */
  const committed = useRef(JSON.stringify(stored));

  useEffect(() => {
    const json = JSON.stringify(stored);
    if (json === committed.current) return;
    committed.current = json;
    setDraft(stored);
    setPicking(stored === null);
    labelTouched.current = stored !== null && stored.label !== suggestLabel(stored.action, t);
  }, [stored]);

  const commit = (next: DeckButton | null, coalesce?: string) => {
    setDraft(next);
    if (next && incompleteReason(next.action, t) !== null) return;
    committed.current = JSON.stringify(next);
    update((c) => {
      const buttons = c.pages[page].buttons;
      if (next) buttons[buttonKey] = next;
      else delete buttons[buttonKey];
    }, coalesce);
  };

  const setAction = (action: Action, hint?: { appName?: string }) => {
    // 이름을 직접 고친 적이 없거나, 지금 이름이 이전 동작에서 지어 준 이름 그대로면 새로 짓는다
    const auto = !draft || !labelTouched.current || draft.label === suggestLabel(draft.action, t);
    const label = auto ? suggestLabel(action, t, hint?.appName) : draft.label;
    commit({ ...(draft ?? {}), label, action }, `action:${page}:${buttonKey}`);
  };

  const reason = draft ? incompleteReason(draft.action, t) : null;
  const KindIcon = draft ? KIND_ICONS[draft.action.type] : null;

  return (
    <div className="space-y-5">
      <h2 className="text-base font-semibold">
        {draft || !picking ? p.buttonTitle(buttonKey) : p.emptyTitle(buttonKey)}
      </h2>

      {picking ? (
        <div className="space-y-3">
          <p className="text-sm text-muted-foreground">{p.pickAction}</p>
          <ActionPicker
            onPick={(kind) => {
              setPicking(false);
              setAction(emptyAction(kind));
            }}
          />
          {draft && (
            <Button variant="ghost" size="sm" onClick={() => setPicking(false)}>
              {p.cancelChange}
            </Button>
          )}
        </div>
      ) : (
        draft && (
          <>
            <div className="space-y-1.5">
              <Label htmlFor="button-label">{p.label}</Label>
              <Input
                id="button-label"
                value={draft.label}
                placeholder={p.labelPlaceholder}
                onChange={(e) => {
                  labelTouched.current = true;
                  commit({ ...draft, label: e.currentTarget.value }, `label:${page}:${buttonKey}`);
                }}
              />
            </div>

            <div className="space-y-1.5">
              <Label>{t.editor.icon.label}</Label>
              <div className="flex items-center gap-2">
                <span className="flex size-10 items-center justify-center rounded-lg border bg-muted/40">
                  <Icon name={buttonIcon(draft)} className="size-5" />
                </span>
                <Button variant="outline" size="sm" onClick={() => setIconOpen(true)}>
                  {t.editor.icon.change}
                </Button>
                {draft.icon ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    title={t.editor.icon.autoHint}
                    onClick={() => {
                      const { icon: _, ...rest } = draft;
                      commit(rest);
                    }}
                  >
                    {t.editor.icon.auto}
                  </Button>
                ) : (
                  <span className="text-xs text-muted-foreground">{t.editor.icon.autoHint}</span>
                )}
              </div>
              <IconPickerDialog
                open={iconOpen}
                onOpenChange={setIconOpen}
                current={buttonIcon(draft)}
                onPick={(name) => {
                  setIconOpen(false);
                  commit({ ...draft, icon: name });
                }}
              />
            </div>

            <div className="space-y-3">
              <div className="flex items-center gap-2">
                {KindIcon && <KindIcon className="size-4 text-blue-600" />}
                <span className="text-sm font-medium">{t.editor.kinds[draft.action.type].title}</span>
                <Button variant="ghost" size="xs" className="ml-auto" onClick={() => setPicking(true)}>
                  <Replace /> {p.changeAction}
                </Button>
              </div>
              <ActionForm action={draft.action} onChange={setAction} />
            </div>

            {reason && (
              <p className="flex items-start gap-1.5 rounded-lg bg-amber-500/10 px-3 py-2 text-xs text-amber-800">
                <TriangleAlert className="mt-px size-3.5 shrink-0" />
                {p.incomplete(reason)}
              </p>
            )}

            <div className="flex gap-2 border-t pt-4">
              <Button
                size="sm"
                disabled={reason !== null}
                title={p.testHint}
                onClick={() => void testAction(draft.label, draft.action)}
              >
                <Play /> {p.test}
              </Button>
              {playsSound(draft.action) && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void testAction(p.stopTest, { type: "stopSounds" })}
                >
                  <Square /> {p.stopTest}
                </Button>
              )}
              <Button variant="outline" size="sm" className="ml-auto text-destructive" onClick={() => {
                commit(null);
                setPicking(true);
                labelTouched.current = false;
              }}>
                <Trash2 /> {p.clear}
              </Button>
            </div>
          </>
        )
      )}
    </div>
  );
}

/** 효과음을 내는 동작인지 (시험 실행 옆에 "소리 멈추기"를 보여줄지) */
function playsSound(action: Action): boolean {
  if (action.type === "sound") return true;
  return action.type === "sequence" && action.steps.some(playsSound);
}
