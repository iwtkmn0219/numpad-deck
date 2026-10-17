import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleHelp, Globe, LayoutGrid, Redo2, Settings, ShieldCheck, Undo2, X } from "lucide-react";
import type { Config } from "@/bindings/Config";
import type { InstallMode } from "@/bindings/InstallMode";
import type { Preview } from "@/bindings/Preview";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
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
import { getConfig, installDeckKeyForwarding, onShareOpen, openShare, pickShareOpen, takeShareRequest } from "./api";
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
import { useExport } from "./share/ExportFlow";
import { ImportDialog } from "./share/ImportDialog";

export function Editor() {
  const t = useT().editor;
  const { config, status, update, adopt, beginInstall, endInstall, forgetHistory, undo, redo, canUndo, canRedo } = useEditorConfig();
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<Selection | null>({ kind: "button", key: "7" });

  const [onboarding, setOnboarding] = useState(false);
  const [layoutOpen, setLayoutOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  // 편집기가 앞에 있을 때도 넘패드 덱 키가 동작하게
  useEffect(() => installDeckKeyForwarding(), []);

  // 첫 실행이면 온보딩부터
  const [onboardingChecked, setOnboardingChecked] = useState(false);
  useEffect(() => {
    void invoke<boolean>("onboarding_pending")
      .then((pending) => setOnboarding(pending))
      .catch(() => setOnboarding(false)) // 실패하면 대기 아님으로 보고 공유 요청은 계속 받는다
      .finally(() => setOnboardingChecked(true));
  }, []);

  // 공유 파일: 내보내기 흐름, 가져오기 창
  const exporter = useExport(config);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  /** "바꾸기" 뒤 자동 백업 경로 안내 */
  const [replacedBackup, setReplacedBackup] = useState<string | null>(null);
  /** 받은 미리 보기 중 가장 새 id. 코어는 나중에 끝난 열기의 묶음을 들고 있다(id가 가장 큼). */
  const latestPreview = useRef(0);

  const openShareFile = useCallback(async (path: string) => {
    try {
      const p = await openShare(path);
      if (p.id < latestPreview.current) return;
      latestPreview.current = p.id;
      setPreview(p);
    } catch (e) {
      setOpenError(String(e));
    }
  }, []);

  const pickAndImport = async () => {
    const path = await pickShareOpen();
    if (path) await openShareFile(path);
  };

  // 더블클릭한 파일: 시작할 때와 `share://open`을 받을 때 꺼낸다. 온보딩 중이면 끝난 뒤에.
  const [shareSignal, setShareSignal] = useState(0);
  useEffect(() => {
    const unlisten = onShareOpen(() => setShareSignal((n) => n + 1));
    return () => void unlisten.then((f) => f());
  }, []);
  useEffect(() => {
    if (!onboardingChecked || onboarding) return;
    void takeShareRequest().then((path) => {
      if (path) void openShareFile(path);
    });
  }, [onboardingChecked, onboarding, shareSignal, openShareFile]);

  const onInstalled = async (previewId: number, mode: InstallMode, count: number, backup: string | null) => {
    // 설치하는 사이 새 파일이 열렸으면 그 창은 닫지 않는다
    setPreview((p) => (p?.id === previewId ? null : p));
    setReplacedBackup(backup);
    // 파일 감시를 기다리지 않고 바로 받는다 (바꾸기는 되돌리기 기록을 비운다).
    // 못 받으면 파일 감시가 보내 준다.
    const next = await getConfig().catch(() => null);
    if (!next) {
      // 바꾸기는 파일 감시로 받을 때도 옛 설정을 되돌리기 기록에 남기지 않는다
      if (mode === "replace") forgetHistory();
      return;
    }
    adopt(next, mode === "replace");
    setPage(mode === "add" ? Math.max(0, next.pages.length - count) : 0);
  };

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

      {replacedBackup && (
        <div className="mx-5 mt-3 flex items-center justify-between gap-3 rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-2 text-sm">
          <span className="flex min-w-0 items-center gap-2">
            <ShieldCheck className="size-4 shrink-0 text-emerald-600" />
            <span className="break-all">{t.share.replaced(replacedBackup)}</span>
          </span>
          <Button variant="ghost" size="icon-sm" onClick={() => setReplacedBackup(null)} title={t.share.dismiss}>
            <X />
          </Button>
        </div>
      )}

      <PageTabs
        config={config}
        page={page}
        onSelect={setPage}
        update={update}
        onExport={(i) => exporter.start([i])}
        onImport={() => void pickAndImport()}
      />

      <main className="flex min-h-0 flex-1 gap-6 overflow-auto p-6">
        <div className="w-[480px] shrink-0">
          <KeyGrid grid={gridFor(config.layout, config.systemKeys)} page={current} selected={selected} onSelect={setSelected} />
        </div>
        <aside className="min-h-0 min-w-0 flex-1 overflow-y-auto rounded-xl border bg-card p-5">
          <ButtonPanel config={config} page={page} selected={selected} update={update} />
        </aside>
      </main>

      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        config={config}
        update={update}
        onExportAll={() => exporter.start(null)}
        onImportBackup={() => {
          setSettingsOpen(false);
          void pickAndImport();
        }}
      />
      <LayoutEditor open={layoutOpen} onOpenChange={setLayoutOpen} config={config} update={update} />

      {exporter.dialogs}

      {preview && !onboarding && (
        <ImportDialog
          key={preview.id}
          preview={preview}
          config={config}
          onClose={() => setPreview(null)}
          beforeInstall={beginInstall}
          endInstall={endInstall}
          onInstalled={onInstalled}
        />
      )}

      <AlertDialog open={openError !== null} onOpenChange={(open) => !open && setOpenError(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.share.openFailed}</AlertDialogTitle>
            <AlertDialogDescription className="break-all">{openError}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogAction>{t.share.ok}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

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
