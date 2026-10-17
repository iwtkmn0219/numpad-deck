import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Download, FileDown, FileJson, FileUp, FolderOpen, RefreshCw } from "lucide-react";
import type { Action } from "@/bindings/Action";
import type { Config } from "@/bindings/Config";
import { Button } from "@/components/ui/button";
import { Dialog, DialogBody, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useT } from "@/i18n";

/** `lib.rs`의 `AppInfo`와 모양이 같아야 한다. */
interface AppInfo {
  version: string;
  configPath: string;
}

/** `lib.rs`의 `AutostartState`와 모양이 같아야 한다. */
interface Autostart {
  enabled: boolean;
  /** 개발 빌드면 false */
  available: boolean;
}

/** `update.rs`의 `State`·`Status`와 모양이 같아야 한다. */
type UpdateStatus =
  | { kind: "idle" | "checking" | "latest" | "installing" | "unavailable" }
  | { kind: "available"; version: string }
  | { kind: "failed"; error: string };
interface UpdateState {
  current: string;
  status: UpdateStatus;
}

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  config: Config;
  update: (recipe: (draft: Config) => void) => void;
  /** 설정 전체를 파일로 */
  onExportAll: () => void;
  /** 백업 파일 골라 가져오기 */
  onImportBackup: () => void;
}

/** 어느 페이지에든(묶음 안 포함) 마이크 끄기 버튼이 있는가. 코어 `Config::uses_mic_mute`와 같은 규칙. */
function usesMicMute(config: Config): boolean {
  const has = (a: Action): boolean => a.type === "micMute" || (a.type === "sequence" && a.steps.some(has));
  return config.pages.some((p) => Object.values(p.buttons).some((b) => b && has(b.action)));
}

/** 업데이트 상태 (설정 창·편집기 띠가 같이 쓴다). */
function useUpdate(): [UpdateState | null, (s: UpdateState) => void] {
  const [update, setUpdate] = useState<UpdateState | null>(null);
  useEffect(() => {
    // 창을 열기 전에 이미 찾은 업데이트도 보이게 처음에 한 번 묻고, 그 뒤로는 바뀔 때 받는다
    void invoke<UpdateState>("get_update").then(setUpdate);
    const unlisten = listen<UpdateState>("update://changed", (e) => setUpdate(e.payload));
    return () => void unlisten.then((f) => f());
  }, []);
  return [update, setUpdate];
}

/** 편집기 위쪽 띠: 새 버전이 있으면 바로 설치할 수 있게. */
export function UpdateBanner() {
  const t = useT().editor.settings;
  const [update, setUpdate] = useUpdate();
  const s = update?.status;
  if (!s || (s.kind !== "available" && s.kind !== "installing")) return null;
  return (
    <div className="mx-5 mt-3 flex items-center justify-between gap-3 rounded-lg border border-sky-500/30 bg-sky-500/10 px-3 py-2 text-sm">
      <span className="flex items-center gap-2">
        <Download className="size-4 text-sky-600" />
        {s.kind === "available" ? t.banner(s.version) : t.update.installing}
      </span>
      {s.kind === "available" && (
        <Button size="sm" onClick={() => void invoke<UpdateState>("install_update").then(setUpdate)}>
          {t.update.install(s.version)}
        </Button>
      )}
    </div>
  );
}

/** 앱 설정: Windows 시작 시 실행, 설정 파일, 앱 정보. 버튼·페이지 편집과 상관없는 것만 둔다. */
export function SettingsDialog({ open, onOpenChange, config, update: updateConfig, onExportAll, onImportBackup }: Props) {
  const t = useT().editor.settings;
  const ts = useT().editor.share;
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [autostart, setAutostart] = useState<Autostart | null>(null);
  const [update, setUpdate] = useUpdate();

  useEffect(() => {
    if (!open) return;
    void invoke<AppInfo>("app_info").then(setInfo);
    void invoke<Autostart>("get_autostart").then(setAutostart);
    void invoke<UpdateState>("get_update").then(setUpdate);
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t.title}</DialogTitle>
        </DialogHeader>

        <DialogBody className="space-y-4">
          <section className="space-y-2">
            <h3 className="font-medium">{t.startTitle}</h3>
            <label className="flex cursor-pointer items-start gap-3">
              <input
                type="checkbox"
                className="mt-0.5 size-4 accent-blue-600"
                checked={autostart?.enabled ?? false}
                disabled={!autostart?.available}
                onChange={(e) => void invoke<Autostart>("set_autostart", { on: e.target.checked }).then(setAutostart)}
              />
              <span>
                <span className="block">{t.autostart}</span>
                <span className="block text-muted-foreground">
                  {autostart && !autostart.available ? t.autostartDev : t.autostartHint}
                </span>
              </span>
            </label>
          </section>

          <section className="space-y-2 border-t pt-4">
            <h3 className="font-medium">{t.micTitle}</h3>
            <label className="flex cursor-pointer items-start gap-3">
              <input
                type="checkbox"
                className="mt-0.5 size-4 accent-blue-600"
                checked={usesMicMute(config) && config.micIndicator !== false}
                disabled={!usesMicMute(config)}
                onChange={(e) =>
                  updateConfig((draft) => {
                    // 켜기 = 기본값이므로 파일에서 지운다
                    if (e.target.checked) delete draft.micIndicator;
                    else draft.micIndicator = false;
                  })
                }
              />
              <span>
                <span className="block">{t.micIndicator}</span>
                <span className="block text-muted-foreground">
                  {usesMicMute(config) ? t.micIndicatorHint : t.micIndicatorNoButton}
                </span>
              </span>
            </label>
          </section>

          {update && (
            <section className="space-y-2 border-t pt-4">
              <h3 className="font-medium">{t.updateTitle}</h3>
              <p className="text-muted-foreground">
                {(() => {
                  const s = update.status;
                  switch (s.kind) {
                    case "available":
                      return t.update.available(s.version);
                    case "failed":
                      return t.update.failed;
                    default:
                      return t.update[s.kind];
                  }
                })()}
              </p>
              {update.status.kind === "available" ? (
                <Button size="sm" onClick={() => void invoke<UpdateState>("install_update").then(setUpdate)}>
                  <Download /> {t.update.install(update.status.version)}
                </Button>
              ) : (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={["checking", "installing", "unavailable"].includes(update.status.kind)}
                  onClick={() => void invoke<UpdateState>("check_update").then(setUpdate)}
                >
                  <RefreshCw /> {t.update.check}
                </Button>
              )}
            </section>
          )}

          <section className="space-y-2 border-t pt-4">
            <h3 className="font-medium">{ts.backupTitle}</h3>
            <p className="text-muted-foreground">{ts.backupBody}</p>
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={onExportAll}>
                <FileDown /> {ts.exportAll}
              </Button>
              <Button variant="outline" size="sm" onClick={onImportBackup}>
                <FileUp /> {ts.importBackup}
              </Button>
            </div>
          </section>

          <section className="space-y-2 border-t pt-4">
            <h3 className="font-medium">{t.configTitle}</h3>
            <p className="text-muted-foreground">{t.configBody}</p>
            {info && (
              <p className="rounded-md bg-muted px-2.5 py-1.5 font-mono text-xs break-all">{info.configPath}</p>
            )}
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={() => void invoke("open_config_folder")}>
                <FolderOpen /> {t.openFolder}
              </Button>
              <Button variant="outline" size="sm" onClick={() => void invoke("open_config")}>
                <FileJson /> {t.openFile}
              </Button>
            </div>
          </section>

          <section className="space-y-1 border-t pt-4">
            <h3 className="font-medium">{t.aboutTitle}</h3>
            <p>
              Numpad Deck <span className="text-muted-foreground">{info && t.version(info.version)}</span>
            </p>
            <p className="text-muted-foreground">{t.privacy}</p>
          </section>
        </DialogBody>
      </DialogContent>
    </Dialog>
  );
}
