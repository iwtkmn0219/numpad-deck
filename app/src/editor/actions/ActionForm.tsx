import { useEffect, useRef, useState } from "react";
import { ArrowDown, ArrowUp, Check, Clock, FileSearch, FolderOpen, Keyboard, Plus, X } from "lucide-react";
import type { Action } from "@/bindings/Action";
import type { MediaCommand } from "@/bindings/MediaCommand";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Slider } from "@/components/ui/slider";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { pickFile, pickFolder, recordHotkey, type RecordResult } from "../api";
import { ActionPicker } from "./ActionPicker";
import { AppPickerDialog } from "./AppPickerDialog";
import { emptyAction, KIND_ICONS, MAX_DELAY_MS } from "./meta";

export interface FormProps<A extends Action = Action> {
  action: A;
  onChange: (action: Action, hint?: { appName?: string }) => void;
}

/** 동작 종류에 맞는 입력 화면. */
export function ActionForm({ action, onChange }: FormProps) {
  switch (action.type) {
    case "hotkey":
      return <HotkeyForm action={action} onChange={onChange} />;
    case "launch":
      return <LaunchForm action={action} onChange={onChange} />;
    case "open":
      return <OpenForm action={action} onChange={onChange} />;
    case "text":
      return <TextForm action={action} onChange={onChange} />;
    case "media":
      return <MediaForm action={action} onChange={onChange} />;
    case "micMute":
      return <MicForm />;
    case "sound":
      return <SoundForm action={action} onChange={onChange} />;
    case "sequence":
      return <SequenceForm action={action} onChange={onChange} />;
    case "delay":
      return <DelayForm action={action} onChange={onChange} />;
  }
}

type Of<K extends Action["type"]> = Extract<Action, { type: K }>;

function HotkeyForm({ action, onChange }: FormProps<Of<"hotkey">>) {
  const f = useT().editor.forms.hotkey;
  const [recording, setRecording] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const stop = useRef<(() => void) | null>(null);

  // 화면을 떠나면 녹화를 끝낸다 (녹화 중에는 모든 키가 막힌다)
  useEffect(() => () => stop.current?.(), []);

  const start = async () => {
    setMessage(null);
    setRecording(true);
    stop.current = await recordHotkey((result: RecordResult) => {
      stop.current = null;
      setRecording(false);
      if (result.kind === "keys") onChange({ type: "hotkey", keys: result.keys });
      else if (result.kind === "rejected") setMessage(f.rejected[result.reason]);
      else if (result.kind === "timeout") setMessage(f.timeout);
    });
  };

  const cancel = () => {
    stop.current?.();
    stop.current = null;
    setRecording(false);
  };

  return (
    <div className="space-y-2">
      <Label>{f.keys}</Label>
      <div
        className={cn(
          "flex min-h-11 flex-wrap items-center gap-1.5 rounded-lg border px-3 py-2",
          recording && "border-blue-500 ring-2 ring-blue-500/30",
        )}
      >
        {recording ? (
          <span className="text-sm text-blue-600">{f.recording}</span>
        ) : action.keys.length ? (
          action.keys.map((k, i) => (
            <span key={i} className="flex items-center gap-1.5">
              {i > 0 && <span className="text-muted-foreground">+</span>}
              <kbd className="rounded-md border bg-muted px-2 py-0.5 font-sans text-sm shadow-xs">{k}</kbd>
            </span>
          ))
        ) : (
          <span className="text-sm text-muted-foreground">{f.none}</span>
        )}
      </div>
      {recording ? (
        <Button variant="outline" size="sm" onClick={cancel}>
          <X /> {f.cancel}
        </Button>
      ) : (
        <Button variant="outline" size="sm" onClick={start}>
          <Keyboard /> {action.keys.length ? f.rerecord : f.record}
        </Button>
      )}
      {message && <p className="text-xs text-destructive">{message}</p>}
    </div>
  );
}

function LaunchForm({ action, onChange }: FormProps<Of<"launch">>) {
  const f = useT().editor.forms.launch;
  const [listOpen, setListOpen] = useState(false);
  const [showArgs, setShowArgs] = useState(Boolean(action.args));

  return (
    <div className="space-y-3">
      <div className="space-y-2">
        <Label>{f.app}</Label>
        <TargetBox value={action.target} empty={f.none} />
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" size="sm" onClick={() => setListOpen(true)}>
            <SquareIcon /> {f.fromList}
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              const file = await pickFile(f.fileFilter, ["exe", "lnk", "bat", "cmd"]);
              if (file) onChange({ ...action, target: file });
            }}
          >
            <FileSearch /> {f.fromFile}
          </Button>
        </div>
      </div>
      {showArgs ? (
        <div className="space-y-1.5">
          <Label htmlFor="launch-args">{f.args}</Label>
          <Input
            id="launch-args"
            value={action.args ?? ""}
            onChange={(e) => {
              const args = e.currentTarget.value;
              const next: Of<"launch"> = { type: "launch", target: action.target };
              if (args) next.args = args;
              onChange(next);
            }}
          />
          <p className="text-xs text-muted-foreground">{f.argsHint}</p>
        </div>
      ) : (
        <button className="text-xs text-muted-foreground underline" onClick={() => setShowArgs(true)}>
          {f.args}
        </button>
      )}
      <AppPickerDialog
        open={listOpen}
        onOpenChange={setListOpen}
        onPick={(app) => {
          setListOpen(false);
          onChange({ type: "launch", target: app.target }, { appName: app.name });
        }}
      />
    </div>
  );
}

function SquareIcon() {
  const Icon = KIND_ICONS.launch;
  return <Icon />;
}

type OpenTab = "web" | "folder" | "file";

function guessTab(target: string): OpenTab {
  if (target === "" || /^[a-z]+:\/\//i.test(target) || !/[\\/]/.test(target)) return "web";
  return /\.[a-z0-9]{1,5}$/i.test(target) ? "file" : "folder";
}

function OpenForm({ action, onChange }: FormProps<Of<"open">>) {
  const f = useT().editor.forms.open;
  const [tab, setTab] = useState<OpenTab>(() => guessTab(action.target));

  return (
    <div className="space-y-3">
      <Tabs value={tab} onValueChange={(v) => setTab(v as OpenTab)}>
        <TabsList>
          <TabsTrigger value="web">{f.web}</TabsTrigger>
          <TabsTrigger value="folder">{f.folder}</TabsTrigger>
          <TabsTrigger value="file">{f.file}</TabsTrigger>
        </TabsList>
      </Tabs>
      {tab === "web" ? (
        <div className="space-y-1.5">
          <Label htmlFor="open-url">{f.url}</Label>
          <Input
            id="open-url"
            value={action.target}
            placeholder={f.urlPlaceholder}
            onChange={(e) => onChange({ type: "open", target: e.currentTarget.value })}
            // "youtube.com"처럼 적으면 https://를 붙여 준다
            onBlur={(e) => {
              const v = e.currentTarget.value.trim();
              if (v && !/^[a-z][a-z0-9+.-]*:/i.test(v)) onChange({ type: "open", target: `https://${v}` });
            }}
          />
        </div>
      ) : (
        <div className="space-y-2">
          <TargetBox value={action.target} empty={f.none} />
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              const picked = tab === "folder" ? await pickFolder() : await pickFile(f.file, []);
              if (picked) onChange({ type: "open", target: picked });
            }}
          >
            <FolderOpen /> {tab === "folder" ? f.pickFolder : f.pickFile}
          </Button>
        </div>
      )}
    </div>
  );
}

function TextForm({ action, onChange }: FormProps<Of<"text">>) {
  const f = useT().editor.forms.text;
  return (
    <div className="space-y-1.5">
      <Label htmlFor="text-body">{f.text}</Label>
      <Textarea
        id="text-body"
        rows={4}
        value={action.text}
        placeholder={f.placeholder}
        onChange={(e) => onChange({ type: "text", text: e.currentTarget.value })}
      />
    </div>
  );
}

const MEDIA: MediaCommand[] = ["playPause", "prev", "next", "stop", "volumeDown", "volumeUp", "volumeMute"];

function MediaForm({ action, onChange }: FormProps<Of<"media">>) {
  const t = useT().editor;
  return (
    <div className="space-y-1.5">
      <Label>{t.forms.media.command}</Label>
      <div className="grid grid-cols-2 gap-1.5">
        {MEDIA.map((command) => (
          <button
            key={command}
            onClick={() => onChange({ type: "media", command })}
            className={cn(
              "flex items-center justify-between rounded-lg border px-3 py-2 text-left text-sm",
              action.command === command ? "border-blue-500 bg-blue-500/5" : "hover:bg-muted/40",
            )}
          >
            {t.action.media[command]}
            {action.command === command && <Check className="size-4 text-blue-600" />}
          </button>
        ))}
      </div>
    </div>
  );
}

function MicForm() {
  const f = useT().editor.forms.micMute;
  return <p className="text-sm text-muted-foreground">{f.body}</p>;
}

function SoundForm({ action, onChange }: FormProps<Of<"sound">>) {
  const f = useT().editor.forms.sound;
  const volume = action.volume ?? 1;
  const set = (file: string, v: number) => {
    const next: Of<"sound"> = { type: "sound", file };
    if (v < 1) next.volume = Math.round(v * 100) / 100;
    onChange(next);
  };
  return (
    <div className="space-y-3">
      <div className="space-y-2">
        <Label>{f.file}</Label>
        <TargetBox value={action.file} empty={f.none} />
        <Button
          variant="outline"
          size="sm"
          onClick={async () => {
            const file = await pickFile(f.fileFilter, ["mp3", "wav", "ogg", "flac"]);
            if (file) set(file, volume);
          }}
        >
          <FileSearch /> {f.pick}
        </Button>
      </div>
      <div className="space-y-2">
        <Label>
          {f.volume} <span className="text-muted-foreground tabular-nums">{Math.round(volume * 100)}%</span>
        </Label>
        <Slider
          value={[Math.round(volume * 100)]}
          min={5}
          max={100}
          step={5}
          onValueChange={([v]) => set(action.file, v / 100)}
        />
      </div>
    </div>
  );
}

function DelayForm({ action, onChange }: FormProps<Of<"delay">>) {
  const f = useT().editor.forms.delay;
  return (
    <div className="flex items-center gap-2">
      <Clock className="size-4 text-muted-foreground" />
      <Label htmlFor="delay-sec" className="shrink-0">
        {f.seconds}
      </Label>
      <Input
        id="delay-sec"
        type="number"
        min={0.1}
        max={MAX_DELAY_MS / 1000}
        step={0.1}
        className="w-24"
        value={action.ms / 1000}
        onChange={(e) => {
          const sec = Number(e.currentTarget.value);
          if (Number.isFinite(sec)) onChange({ type: "delay", ms: Math.round(sec * 1000) });
        }}
      />
    </div>
  );
}

function SequenceForm({ action, onChange }: FormProps<Of<"sequence">>) {
  const t = useT().editor;
  const f = t.forms.sequence;
  const [adding, setAdding] = useState(action.steps.length === 0);
  const steps = action.steps;
  const set = (next: Action[]) => onChange({ type: "sequence", steps: next });

  return (
    <div className="space-y-3">
      <Label>{f.steps}</Label>
      {steps.length === 0 && !adding && <p className="text-sm text-muted-foreground">{f.empty}</p>}
      <ol className="space-y-2">
        {steps.map((step, i) => {
          const Icon = KIND_ICONS[step.type];
          return (
            <li key={i} className="rounded-xl border p-3">
              <div className="mb-2 flex items-center gap-2">
                <Badge variant="secondary" className="tabular-nums">
                  {i + 1}
                </Badge>
                <Icon className="size-4 text-muted-foreground" />
                <span className="text-sm font-medium">{t.kinds[step.type].title}</span>
                <span className="ml-auto flex gap-0.5">
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    title={f.moveUp}
                    disabled={i === 0}
                    onClick={() => set(swap(steps, i, i - 1))}
                  >
                    <ArrowUp />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    title={f.moveDown}
                    disabled={i === steps.length - 1}
                    onClick={() => set(swap(steps, i, i + 1))}
                  >
                    <ArrowDown />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon-xs"
                    title={f.remove}
                    onClick={() => set(steps.filter((_, j) => j !== i))}
                  >
                    <X />
                  </Button>
                </span>
              </div>
              <ActionForm action={step} onChange={(next) => set(steps.map((s, j) => (j === i ? next : s)))} />
            </li>
          );
        })}
      </ol>
      {adding ? (
        <div className="space-y-2 rounded-xl border border-dashed p-3">
          <ActionPicker
            compact
            inSequence
            onPick={(kind) => {
              set([...steps, emptyAction(kind)]);
              setAdding(false);
            }}
          />
          {steps.length > 0 && (
            <Button variant="ghost" size="sm" onClick={() => setAdding(false)}>
              {t.panel.cancelChange}
            </Button>
          )}
        </div>
      ) : (
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => setAdding(true)}>
            <Plus /> {f.addStep}
          </Button>
          <Button variant="outline" size="sm" onClick={() => set([...steps, emptyAction("delay")])}>
            <Clock /> {f.addDelay}
          </Button>
        </div>
      )}
    </div>
  );
}

function swap<T>(list: T[], a: number, b: number): T[] {
  const next = [...list];
  [next[a], next[b]] = [next[b], next[a]];
  return next;
}

/** 고른 파일·폴더·앱 경로를 보여주는 칸 */
function TargetBox({ value, empty }: { value: string; empty: string }) {
  return (
    <p
      className={cn(
        "truncate rounded-lg border bg-muted/40 px-3 py-2 text-sm",
        !value && "text-muted-foreground",
      )}
      title={value}
    >
      {value || empty}
    </p>
  );
}
