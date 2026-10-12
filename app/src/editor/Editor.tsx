import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleHelp, Globe, LayoutGrid, Redo2, Settings, Undo2 } from "lucide-react";
import type { Config } from "@/bindings/Config";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { installDeckKeyForwarding } from "./api";
import { ButtonPanel } from "./ButtonPanel";
import { gridFor } from "@/shared/layout";
import { KeyGrid, type Selection } from "./KeyGrid";
import { LayoutEditor } from "./LayoutEditor";
import { useMode } from "./useMode";
import { SettingsDialog, UpdateBanner } from "./SettingsDialog";
import { Onboarding } from "./Onboarding";
import { chooseLanguage, type LanguageChoice } from "./language";
import { PageTabs } from "./PageTabs";
import { useEditorConfig, type SaveStatus } from "./useEditorConfig";

export function Editor() {
  const t = useT().editor;
  const { config, status, update, undo, redo, canUndo, canRedo } = useEditorConfig();
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<Selection | null>({ kind: "button", key: "7" });

  const [onboarding, setOnboarding] = useState(false);
  const [layoutOpen, setLayoutOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  // 편집기가 앞에 있을 때도 넘패드 덱 키가 동작하게
  useEffect(() => installDeckKeyForwarding(), []);

  // 첫 실행이면 온보딩부터
  useEffect(() => {
    void invoke<boolean>("onboarding_pending").then(setOnboarding);
  }, []);

  // 되돌리기·밖에서 고침으로 페이지가 줄면 범위 안으로
  const pageCount = config?.pages.length ?? 0;
  useEffect(() => {
    if (pageCount > 0 && page >= pageCount) setPage(pageCount - 1);
  }, [page, pageCount]);

  if (!config) return null;
  const current = config.pages[Math.min(page, config.pages.length - 1)];

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center justify-between px-5 pt-4">
        <div className="flex items-center gap-3">
          <h1 className="text-lg font-semibold">{t.windowTitle}</h1>
          <ModeBadge />
        </div>
        <div className="flex items-center gap-1">
          <StatusText status={status} />
          <Button variant="ghost" size="icon-sm" onClick={undo} disabled={!canUndo} title={`${t.undo} (Ctrl+Z)`}>
            <Undo2 />
          </Button>
          <Button variant="ghost" size="icon-sm" onClick={redo} disabled={!canRedo} title={`${t.redo} (Ctrl+Y)`}>
            <Redo2 />
          </Button>
          <Button variant="ghost" size="icon-sm" onClick={() => setLayoutOpen(true)} title={t.layout.button}>
            <LayoutGrid />
          </Button>
          <LanguageMenu config={config} update={update} />
          <Button variant="ghost" size="icon-sm" onClick={() => setOnboarding(true)} title={t.help}>
            <CircleHelp />
          </Button>
          <Button variant="ghost" size="icon-sm" onClick={() => setSettingsOpen(true)} title={t.settings.button}>
            <Settings />
          </Button>
        </div>
      </header>

      <UpdateBanner />

      <PageTabs config={config} page={page} onSelect={setPage} update={update} />

      <main className="flex min-h-0 flex-1 gap-6 overflow-auto p-6">
        <div className="w-[480px] shrink-0">
          <KeyGrid grid={gridFor(config.layout, config.systemKeys)} page={current} selected={selected} onSelect={setSelected} />
        </div>
        <aside className="min-h-0 min-w-0 flex-1 overflow-y-auto rounded-xl border bg-card p-5">
          <ButtonPanel config={config} page={page} selected={selected} update={update} />
        </aside>
      </main>

      <SettingsDialog open={settingsOpen} onOpenChange={setSettingsOpen} config={config} update={update} />
      <LayoutEditor open={layoutOpen} onOpenChange={setLayoutOpen} config={config} update={update} />

      {onboarding && (
        <Onboarding
          config={config}
          update={update}
          onClose={() => {
            setOnboarding(false);
            setPage(0);
          }}
        />
      )}
    </div>
  );
}

/**
 * 지금 모드 (덱 / 숫자 / 일시정지). "현재 모드가 항상 보임" — 편집기 안에서도.
 * 색만으로 알리지 않게 점 모양도 다르다: 덱은 채운 점, 숫자는 속 빈 점, 일시정지는 주황 점.
 */
function ModeBadge() {
  const t = useT().osd;
  const mode = useMode();
  if (!mode) return null;
  const look = {
    deck: { face: "bg-selection-soft text-selection-foreground", dot: "bg-selection" },
    numbers: { face: "bg-muted text-muted-foreground", dot: "border-[1.5px] border-muted-foreground" },
    paused: { face: "bg-amber-50 text-amber-800", dot: "bg-amber-500" },
  }[mode];
  const numlock = mode === "paused" ? null : mode === "deck" ? t.numlockOff : t.numlockOn;
  return (
    <span className={cn("inline-flex h-[22px] items-center gap-1.5 rounded-full px-2.5 text-xs font-medium", look.face)}>
      <span className={cn("size-[7px] rounded-full", look.dot)} />
      {t.mode[mode]}
      {numlock && (
        <span className="opacity-70">
          · {t.numlockLabel} {numlock}
        </span>
      )}
    </span>
  );
}

/** 언어: 자동(Windows 표시 언어) / 한국어 / English. 설정 파일의 `language`에 저장한다. */
function LanguageMenu({ config, update }: { config: Config; update: (recipe: (draft: Config) => void) => void }) {
  const t = useT().editor.language;
  const value = config.language ?? "auto";
  const choose = (v: string) => chooseLanguage(v as LanguageChoice, update);
  // 눌림 효과를 직접 그린다. 메뉴 버튼은 누르는 순간 메뉴가 열리고 닫히며 포커스가 오가서
  // CSS :active가 들쭉날쭉했다 (닫은 뒤 다시 열 때 효과 없음).
  const [pressed, setPressed] = useState(false);
  const release = () => setPressed(false);
  return (
    // modal을 끄면 메뉴가 열린 동안에도 버튼이 눌린다 (켜 두면 화면 전체가 눌리지 않아, 연달아
    // 누를 때 두 번째 누름의 눌림 효과가 없었다)
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          title={t.label}
          className={pressed ? "translate-y-px" : undefined}
          onPointerDown={(e) => e.button === 0 && setPressed(true)}
          onPointerUp={release}
          onPointerLeave={release}
          onPointerCancel={release}
        >
          <Globe />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>{t.label}</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={value} onValueChange={(v) => void choose(v)}>
          <DropdownMenuRadioItem value="auto">{t.auto}</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="ko">{t.ko}</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="en">{t.en}</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function StatusText({ status }: { status: SaveStatus }) {
  const t = useT().editor.status;
  switch (status.kind) {
    case "saved":
      return <span className="mr-2 text-xs text-muted-foreground">{t.saved}</span>;
    case "saving":
      return <span className="mr-2 text-xs text-muted-foreground">{t.saving}</span>;
    case "failed":
      return (
        <span className="mr-2 max-w-md truncate text-xs text-destructive" title={status.reason}>
          {t.failed(status.reason)}
        </span>
      );
  }
}
