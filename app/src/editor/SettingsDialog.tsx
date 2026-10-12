import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileJson, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useT } from "@/i18n";

/** `lib.rs`의 `AppInfo`와 모양이 같아야 한다. */
interface AppInfo {
  version: string;
  configPath: string;
}

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** 앱 설정: 설정 파일, 앱 정보. 버튼·페이지 편집과 상관없는 것만 둔다. */
export function SettingsDialog({ open, onOpenChange }: Props) {
  const t = useT().editor.settings;
  const [info, setInfo] = useState<AppInfo | null>(null);

  useEffect(() => {
    if (open) void invoke<AppInfo>("app_info").then(setInfo);
  }, [open]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t.title}</DialogTitle>
        </DialogHeader>

        <section className="space-y-2">
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
      </DialogContent>
    </Dialog>
  );
}
