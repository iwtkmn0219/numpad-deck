import { MicOff } from "lucide-react";
import { useT } from "@/i18n";

/** 마이크가 꺼져 있는 동안만 창이 보인다 (보이고 숨기는 것은 코어가 한다). */
export function MicBadge() {
  const t = useT();
  return (
    <div className="flex h-screen w-screen items-center justify-center">
      <div className="flex items-center gap-2 rounded-full border border-white/10 bg-[#1f1e1c] py-1.5 pr-4 pl-2 text-sm font-semibold text-white shadow-2xl">
        <span className="flex size-7 items-center justify-center rounded-full bg-red-600">
          <MicOff className="size-4" />
        </span>
        {t.micBadge.muted}
      </div>
    </div>
  );
}
