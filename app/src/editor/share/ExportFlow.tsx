import { useState, type ReactNode } from "react";
import type { Config } from "@/bindings/Config";
import type { ExportReport } from "@/bindings/ExportReport";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { useT, type Dict } from "@/i18n";
import { exportCheck, exportShare, parentFolder, pickShareSave, testAction } from "../api";

/** 파일 이름에 못 쓰는 글자는 `_`로. */
function safeFileName(name: string): string {
  const s = name.replace(/[\\/:*?"<>|]/g, "_").trim();
  return s || "Numpad Deck";
}

/** 오늘 날짜 (현지) YYYY-MM-DD */
function today(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function defaultName(config: Config, pages: number[] | null, t: Dict["editor"]["share"]): string {
  if (pages === null) return t.backupFileName(today());
  return `${safeFileName(config.pages[pages[0]]?.name ?? "")}.npdeck`;
}

/** 확인할 게 있으면 저장 창 전에 확인 창을 띄운다. */
function needsCheck(r: ExportReport): boolean {
  return r.textButtons > 0 || r.missingSounds.length > 0 || r.tooBigSounds.length > 0 || r.soundsTotalTooBig;
}

type Step =
  | { kind: "idle" }
  | { kind: "check"; pages: number[] | null; report: ExportReport }
  | { kind: "saved"; path: string }
  | { kind: "failed"; reason: string };

/**
 * 내보내기 흐름: 확인(필요할 때만) → 저장 창 → 저장 → 결과.
 * `start(null)`은 설정 전체(백업). 돌려받은 `dialogs`를 화면 어딘가에 그린다.
 */
export function useExport(config: Config | null): { start: (pages: number[] | null) => void; dialogs: ReactNode } {
  const t = useT().editor.share;
  const [step, setStep] = useState<Step>({ kind: "idle" });

  const save = async (pages: number[] | null) => {
    if (!config) return;
    setStep({ kind: "idle" });
    const path = await pickShareSave(defaultName(config, pages, t));
    if (!path) return;
    try {
      await exportShare(pages, path);
      setStep({ kind: "saved", path });
    } catch (e) {
      setStep({ kind: "failed", reason: String(e) });
    }
  };

  const start = (pages: number[] | null) => {
    void (async () => {
      try {
        const report = await exportCheck(pages);
        if (needsCheck(report)) setStep({ kind: "check", pages, report });
        else await save(pages);
      } catch (e) {
        setStep({ kind: "failed", reason: String(e) });
      }
    })();
  };

  const close = (open: boolean) => !open && setStep({ kind: "idle" });

  const dialogs = (
    <>
      <AlertDialog open={step.kind === "check"} onOpenChange={close}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.check.title}</AlertDialogTitle>
            {step.kind === "check" && (
              <AlertDialogDescription asChild>
                <div className="space-y-3">
                  <CheckBody report={step.report} t={t} />
                </div>
              </AlertDialogDescription>
            )}
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t.check.cancel}</AlertDialogCancel>
            {step.kind === "check" && (
              <Button disabled={step.report.tooBigSounds.length > 0} onClick={() => void save(step.pages)}>
                {t.check.next}
              </Button>
            )}
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={step.kind === "saved"} onOpenChange={close}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.saved.title}</AlertDialogTitle>
            <AlertDialogDescription className="break-all">
              {step.kind === "saved" && t.saved.body(step.path)}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            {step.kind === "saved" && (
              <Button
                variant="outline"
                onClick={() => void testAction(t.saved.openFolder, { type: "open", target: parentFolder(step.path) })}
              >
                {t.saved.openFolder}
              </Button>
            )}
            <AlertDialogAction>{t.saved.ok}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <AlertDialog open={step.kind === "failed"} onOpenChange={close}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t.exportFailed}</AlertDialogTitle>
            <AlertDialogDescription className="break-all">{step.kind === "failed" && step.reason}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogAction>{t.ok}</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );

  return { start, dialogs };
}

function CheckBody({ report, t }: { report: ExportReport; t: Dict["editor"]["share"] }) {
  return (
    <>
      {report.tooBigSounds.length > 0 && (
        <div>
          <p className="font-medium text-destructive">{t.check.tooBig}</p>
          <FileList files={report.tooBigSounds} />
        </div>
      )}
      {report.textButtons > 0 && <p>{t.check.text(report.textButtons)}</p>}
      {report.missingSounds.length > 0 && (
        <div>
          <p>{t.check.missing}</p>
          <FileList files={report.missingSounds} />
        </div>
      )}
      {report.soundsTotalTooBig && <p>{t.check.totalTooBig}</p>}
    </>
  );
}

function FileList({ files }: { files: string[] }) {
  return (
    <ul className="mt-1 space-y-0.5 rounded-md bg-muted px-2.5 py-1.5 font-mono text-xs break-all">
      {files.map((f) => (
        <li key={f}>{f}</li>
      ))}
    </ul>
  );
}
