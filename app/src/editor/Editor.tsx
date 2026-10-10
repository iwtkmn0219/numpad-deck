import { useEffect, useState } from "react";
import { Redo2, Undo2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useT } from "@/i18n";
import { installDeckKeyForwarding } from "./api";
import { ButtonPanel } from "./ButtonPanel";
import { KeyGrid, type Selection } from "./KeyGrid";
import { PageTabs } from "./PageTabs";
import { useEditorConfig, type SaveStatus } from "./useEditorConfig";

export function Editor() {
  const t = useT().editor;
  const { config, status, update, undo, redo, canUndo, canRedo } = useEditorConfig();
  const [page, setPage] = useState(0);
  const [selected, setSelected] = useState<Selection | null>({ kind: "button", key: "7" });

  // 편집기가 앞에 있을 때도 넘패드 덱 키가 동작하게
  useEffect(() => installDeckKeyForwarding(), []);

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
        </div>
      </header>

      <PageTabs config={config} page={page} onSelect={setPage} update={update} />

      <main className="flex min-h-0 flex-1 gap-6 overflow-auto p-6">
        <div className="w-[480px] shrink-0">
          <KeyGrid page={current} selected={selected} onSelect={setSelected} />
        </div>
        <aside className="min-w-0 flex-1 rounded-xl border bg-card p-5">
          <ButtonPanel config={config} page={page} selected={selected} update={update} />
        </aside>
      </main>
    </div>
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
