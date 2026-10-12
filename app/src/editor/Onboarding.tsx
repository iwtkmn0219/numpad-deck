import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Check, CircleAlert, Keyboard, Pin } from "lucide-react";
import type { Config } from "@/bindings/Config";
import type { Preset } from "@/bindings/Preset";
import { Button } from "@/components/ui/button";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { buttonIcon, Icon } from "@/shared/icons";
import { numpadLabel } from "./keymap";
import { listPresets } from "./PresetDialog";

type Mode = "deck" | "numbers" | "paused";

interface Props {
  config: Config;
  update: (recipe: (draft: Config) => void) => void;
  onClose: () => void;
}

/**
 * 첫 실행 안내 3단계: ① NumLock 끄기 ② 넘패드 1 눌러 보기(음악·영상 프리셋의 YouTube — 눈에 보이는 결과) ③ 프리셋 고르기 + 트레이 아이콘 고정.
 * 편집기 창이 앞이면 키가 저수준 훅을 거치지 않으므로, NumLock·넘패드 키는 이 화면이 받은
 * 키로 판단한다 (덱 버튼 실행은 편집기의 `installDeckKeyForwarding`이 맡는다).
 */
export function Onboarding({ config, update, onClose }: Props) {
  const t = useT().onboarding;
  const [step, setStep] = useState(0);
  const [checking, setChecking] = useState(false);
  const [chosen, setChosen] = useState<string[]>([]);
  const mode = useMode();

  const close = () => {
    void invoke("finish_onboarding");
    onClose();
  };

  const finish = (presets: Preset[]) => {
    const picked = presets.filter((p) => chosen.includes(p.id));
    if (picked.length > 0) {
      update((c) => {
        for (const p of picked) c.pages.push(structuredClone(p.page));
      });
    }
    close();
  };

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center overflow-y-auto bg-background p-6">
      <div className="flex w-full max-w-2xl flex-col gap-6">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-1.5">
            {[0, 1, 2].map((i) => (
              <span key={i} className={cn("h-1.5 w-8 rounded-full", i <= step ? "bg-blue-500" : "bg-muted")} />
            ))}
            <span className="ml-2 text-xs text-muted-foreground">{t.step(step + 1)}</span>
          </div>
          <Button variant="ghost" size="sm" onClick={close}>
            {t.skip}
          </Button>
        </div>

        {checking ? (
          <KeyCheck onClose={() => setChecking(false)} />
        ) : step === 0 ? (
          <NumLockStep mode={mode} />
        ) : step === 1 ? (
          <PressStep config={config} onNoResponse={() => setChecking(true)} />
        ) : (
          <PresetStep config={config} chosen={chosen} setChosen={setChosen} />
        )}

        {!checking && (
          <div className="flex justify-between">
            <Button variant="outline" onClick={() => setStep(step - 1)} className={cn(step === 0 && "invisible")}>
              {t.back}
            </Button>
            {step < 2 ? (
              <Button onClick={() => setStep(step + 1)}>{t.next}</Button>
            ) : (
              <Button onClick={() => void listPresets().then(finish)}>{t.finish}</Button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * 지금 모드. 코어(훅의 NumLock 확인, 1초 주기)에서 받고, 이 화면이 받은 키의 NumLock 상태로
 * 바로 고친다 (편집기가 앞이면 훅이 키를 못 받아 늦을 수 있다).
 */
function useMode(): Mode | null {
  const [mode, setMode] = useState<Mode | null>(null);
  useEffect(() => {
    void invoke<Mode>("get_mode").then(setMode);
    const unlisten = listen<Mode>("mode://changed", (e) => setMode(e.payload));
    const onKey = (e: KeyboardEvent) => {
      const on = e.getModifierState("NumLock");
      setMode((m) => (m === "paused" ? m : on ? "numbers" : "deck"));
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKey);
    return () => {
      void unlisten.then((f) => f());
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKey);
    };
  }, []);
  return mode;
}

function StepTitle({ title, body }: { title: string; body: string }) {
  return (
    <div className="space-y-2">
      <h2 className="text-2xl font-semibold">{title}</h2>
      <p className="text-muted-foreground">{body}</p>
    </div>
  );
}

function NumLockStep({ mode }: { mode: Mode | null }) {
  const t = useT().onboarding.numlock;
  const off = mode === "deck";
  return (
    <div className="space-y-5">
      <StepTitle title={t.title} body={t.body} />
      <div
        className={cn(
          "flex items-center gap-4 rounded-xl border p-5",
          off ? "border-blue-500/40 bg-blue-500/5" : "bg-card",
        )}
      >
        <span
          className={cn(
            "flex size-12 shrink-0 items-center justify-center rounded-lg border text-xs font-semibold",
            mode === "numbers" ? "border-green-500 bg-green-500/10 text-green-700" : "text-muted-foreground",
          )}
        >
          Num
          <br />
          Lock
        </span>
        <div className="space-y-1">
          <p className="font-medium">{mode === "paused" ? t.paused : off ? t.off : t.on}</p>
          <p className="text-sm text-muted-foreground">{off ? t.done : t.keyHint}</p>
        </div>
        {off && <Check className="ml-auto size-6 text-blue-600" />}
      </div>
      <p className="text-sm text-muted-foreground">{t.arrowsNote}</p>
    </div>
  );
}

/** ② 단계에서 눌러 볼 키. 첫 실행 설정(음악·영상)에서 YouTube 열기다. */
const TRY_KEY = "1";

type PressState = { kind: "waiting" } | { kind: "done" } | { kind: "numlockOn" } | { kind: "other"; key: string };

function PressStep({ config, onNoResponse }: { config: Config; onNoResponse: () => void }) {
  const t = useT().onboarding.press;
  const [state, setState] = useState<PressState>({ kind: "waiting" });
  const button = config.pages[0]?.buttons[TRY_KEY] ?? null;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat || e.code === "NumLock") return;
      if (numpadLabel(e.code) === TRY_KEY) {
        setState(e.getModifierState("NumLock") ? { kind: "numlockOn" } : { kind: "done" });
      } else {
        setState((s) => (s.kind === "done" ? s : { kind: "other", key: e.code || e.key }));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="space-y-5">
      <StepTitle title={t.title} body={t.body(button?.label ?? null)} />
      <div className="flex items-center gap-5 rounded-xl border bg-card p-5">
        <span
          className={cn(
            "flex size-20 shrink-0 flex-col justify-between rounded-lg border p-2 transition-colors",
            state.kind === "done" ? "border-blue-500 bg-blue-500 text-white" : "bg-background",
          )}
        >
          <span className="flex items-start justify-between text-xs">
            {TRY_KEY}
            {button && <Icon name={buttonIcon(button)} className="size-5" />}
          </span>
          <span className="truncate text-xs font-medium">{button?.label}</span>
        </span>
        <p className={cn("text-sm", state.kind === "done" ? "font-medium text-blue-700" : "text-muted-foreground")}>
          {state.kind === "done" && <Check className="mr-1 inline size-4" />}
          {state.kind === "waiting" && t.waiting}
          {state.kind === "done" && t.done}
          {state.kind === "numlockOn" && t.numlockOn}
          {state.kind === "other" && t.otherKey(state.key)}
        </p>
      </div>
      <Button variant="link" className="px-0" onClick={onNoResponse}>
        <Keyboard /> {t.noResponse}
      </Button>
    </div>
  );
}

interface Seen {
  id: number;
  text: string;
  ok: boolean;
}

/** 넘패드를 눌러도 반응이 없을 때: 이 화면이 받은 키를 그대로 보여준다. */
function KeyCheck({ onClose }: { onClose: () => void }) {
  const t = useT().onboarding.check;
  const [seen, setSeen] = useState<Seen[]>([]);

  useEffect(() => {
    let id = 0;
    const onKey = (e: KeyboardEvent) => {
      if (e.repeat) return;
      const label = numpadLabel(e.code);
      const numlock = e.getModifierState("NumLock");
      const entry: Seen = label
        ? { id: id++, text: numlock ? t.numlockOn(label) : t.deck(label), ok: !numlock }
        : { id: id++, text: t.notNumpad(e.code || e.key), ok: false };
      setSeen((s) => [entry, ...s].slice(0, 8));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [t]);

  return (
    <div className="space-y-5">
      <StepTitle title={t.title} body={t.body} />
      <ul className="min-h-40 space-y-1 rounded-xl border bg-card p-3 text-sm">
        {seen.length === 0 && <li className="p-2 text-muted-foreground">{t.empty}</li>}
        {seen.map((s) => (
          <li key={s.id} className="flex items-center gap-2 rounded-md px-2 py-1">
            {s.ok ? <Check className="size-4 text-blue-600" /> : <CircleAlert className="size-4 text-amber-600" />}
            {s.text}
          </li>
        ))}
      </ul>
      <div className="space-y-1 text-sm text-muted-foreground">
        <p className="font-medium text-foreground">{t.helpTitle}</p>
        <ul className="list-disc space-y-1 pl-5">
          {t.help.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      </div>
      <Button variant="outline" onClick={onClose}>
        {t.close}
      </Button>
    </div>
  );
}

function PresetStep({
  config,
  chosen,
  setChosen,
}: {
  config: Config;
  chosen: string[];
  setChosen: (ids: string[]) => void;
}) {
  const t = useT().onboarding.presets;
  const [presets, setPresets] = useState<Preset[]>([]);

  useEffect(() => {
    void listPresets().then(setPresets);
  }, []);

  const toggle = (id: string) =>
    setChosen(chosen.includes(id) ? chosen.filter((c) => c !== id) : [...chosen, id]);

  return (
    <div className="space-y-5">
      <StepTitle title={t.title} body={t.body} />
      <div className="grid grid-cols-2 gap-2">
        {presets.map((p) => {
          const already = config.pages.some((page) => page.name === p.name);
          const on = chosen.includes(p.id);
          return (
            <button
              key={p.id}
              disabled={already}
              onClick={() => toggle(p.id)}
              className={cn(
                "flex items-start gap-3 rounded-xl border p-3 text-left disabled:opacity-60",
                on ? "border-blue-500 bg-blue-500/5" : "hover:bg-muted",
              )}
            >
              <span
                className={cn(
                  "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded border",
                  (on || already) && "border-blue-500 bg-blue-500 text-white",
                )}
              >
                {(on || already) && <Check className="size-3.5" />}
              </span>
              <span className="min-w-0 space-y-0.5">
                <span className="block font-medium">{p.name}</span>
                <span className="line-clamp-2 block text-xs text-muted-foreground">
                  {already ? t.already : p.description}
                </span>
              </span>
            </button>
          );
        })}
      </div>
      <div className="flex gap-3 rounded-xl border bg-muted/40 p-4">
        <Pin className="mt-0.5 size-5 shrink-0 text-blue-600" />
        <div className="space-y-1">
          <p className="font-medium">{t.pinTitle}</p>
          <p className="text-sm text-muted-foreground">{t.pinBody}</p>
        </div>
      </div>
    </div>
  );
}
