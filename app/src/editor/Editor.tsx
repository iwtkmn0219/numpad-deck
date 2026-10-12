import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleHelp, Globe, LayoutGrid, Redo2, Undo2 } from "lucide-react";
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
import { installDeckKeyForwarding } from "./api";
import { ButtonPanel } from "./ButtonPanel";
import { gridFor } from "@/shared/layout";
import { KeyGrid, type Selection } from "./KeyGrid";
import { LayoutEditor } from "./LayoutEditor";
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
        <h1 className="text-lg font-semibold">{t.windowTitle}</h1>
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
        </div>
      </header>

      <PageTabs config={config} page={page} onSelect={setPage} update={update} />

      <main className="flex min-h-0 flex-1 gap-6 overflow-auto p-6">
        <div className="w-[480px] shrink-0">
          <KeyGrid grid={gridFor(config.layout, config.systemKeys)} page={current} selected={selected} onSelect={setSelected} />
        </div>
        <aside className="min-h-0 min-w-0 flex-1 overflow-y-auto rounded-xl border bg-card p-5">
          <ButtonPanel config={config} page={page} selected={selected} update={update} />
        </aside>
      </main>

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

/** 언어: 자동(Windows 표시 언어) / 한국어 / English. 설정 파일의 `language`에 저장한다. */
function LanguageMenu({ config, update }: { config: Config; update: (recipe: (draft: Config) => void) => void }) {
  const t = useT().editor.language;
  const value = config.language ?? "auto";
  const choose = (v: string) => chooseLanguage(v as LanguageChoice, update);
  return (
    // modal을 끄면 메뉴가 열린 동안에도 버튼이 눌린다 (켜 두면 화면 전체가 눌리지 않아, 연달아
    // 누를 때 두 번째 누름의 눌림 효과가 없었다)
    <DropdownMenu modal={false}>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" title={t.label} className="active:translate-y-px">
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
