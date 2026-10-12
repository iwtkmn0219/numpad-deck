import { useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useT } from "@/i18n";
import { listApps, type AppEntry } from "../api";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onPick: (app: AppEntry) => void;
}

/** 설치된 앱(시작 메뉴의 "모든 앱")에서 고르기. */
export function AppPickerDialog({ open, onOpenChange, onPick }: Props) {
  const f = useT().editor.forms.launch;
  const [apps, setApps] = useState<AppEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");

  useEffect(() => {
    if (!open || apps) return;
    listApps()
      .then(setApps)
      .catch((e) => setError(String(e)));
  }, [open, apps]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (apps ?? []).filter((a) => !q || a.name.toLowerCase().includes(q));
  }, [apps, query]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{f.listTitle}</DialogTitle>
        </DialogHeader>
        <div className="relative">
          <Search className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            className="pl-8"
            placeholder={f.search}
            value={query}
            onChange={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && shown.length > 0) onPick(shown[0]);
            }}
          />
        </div>
        <div className="-mx-2 h-80 overflow-y-auto">
          {error ? (
            <p className="p-2 text-sm text-destructive">{error}</p>
          ) : !apps ? (
            <p className="p-2 text-sm text-muted-foreground">{f.loading}</p>
          ) : shown.length === 0 ? (
            <p className="p-2 text-sm text-muted-foreground">{f.empty}</p>
          ) : (
            shown.map((app) => (
              <button
                key={`${app.name}|${app.target}`}
                onClick={() => onPick(app)}
                className="block w-full truncate rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted"
                title={app.target}
              >
                {app.name}
              </button>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
