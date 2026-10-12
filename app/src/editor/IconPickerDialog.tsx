import { useMemo, useState } from "react";
import { Search } from "lucide-react";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { useT } from "@/i18n";
import { cn } from "@/lib/utils";
import { Icon, iconNames, type IconName } from "@/shared/icons";

/** 처음 열었을 때 보여줄 자주 쓰는 아이콘 */
const SUGGESTED: IconName[] = [
  "play", "pause", "skip-back", "skip-forward", "volume-2", "volume-x", "mic", "mic-off",
  "music", "headphones", "camera", "video", "video-off", "monitor", "screen-share", "image",
  "keyboard", "app-window", "globe", "folder", "file-text", "mail", "message-circle", "phone",
  "calculator", "terminal", "code", "search", "copy", "clipboard", "scissors", "save",
  "house", "settings", "lock", "power", "sun", "moon", "bell", "calendar",
  "star", "heart", "smile", "zap", "sparkles", "rocket", "gamepad-2", "trophy",
  "coffee", "briefcase", "shopping-cart", "gift", "timer", "clock", "radio", "podcast",
];

/** 한국어로 찾을 수 있게: 한국어 낱말 → 영어 아이콘 이름에 들어 있는 낱말 */
const KO_WORDS: Record<string, string[]> = {
  재생: ["play"], 정지: ["pause", "square", "stop"], 멈춤: ["pause", "stop"], 다음: ["skip-forward", "forward"],
  이전: ["skip-back", "rewind"], 볼륨: ["volume"], 소리: ["volume", "audio", "speaker"], 음소거: ["volume-x", "mic-off"],
  마이크: ["mic"], 음악: ["music", "disc"], 노래: ["music"], 헤드폰: ["headphones"], 카메라: ["camera"],
  영상: ["video", "film", "clapperboard"], 비디오: ["video"], 화면: ["monitor", "screen", "tv"], 모니터: ["monitor"],
  사진: ["image", "camera"], 그림: ["image", "palette"], 캡처: ["camera", "scan", "crop"], 키보드: ["keyboard"],
  창: ["app-window", "panel", "maximize", "minimize"], 앱: ["app-window", "layout-grid"], 인터넷: ["globe", "wifi"],
  웹: ["globe"], 폴더: ["folder"], 파일: ["file"], 문서: ["file-text", "notebook"], 메일: ["mail"], 편지: ["mail"],
  채팅: ["message", "messages"], 메시지: ["message"], 전화: ["phone"], 계산기: ["calculator"], 코드: ["code", "terminal"],
  검색: ["search"], 복사: ["copy"], 붙여넣기: ["clipboard"], 자르기: ["scissors"], 저장: ["save"], 집: ["house", "home"],
  홈: ["house"], 설정: ["settings", "cog"], 잠금: ["lock"], 전원: ["power"], 해: ["sun"], 달: ["moon"], 밤: ["moon"],
  알림: ["bell"], 달력: ["calendar"], 일정: ["calendar"], 별: ["star"], 하트: ["heart"], 웃음: ["smile", "laugh"],
  번개: ["zap"], 반짝: ["sparkles"], 로켓: ["rocket"], 게임: ["gamepad", "joystick", "swords"], 트로피: ["trophy"],
  커피: ["coffee"], 업무: ["briefcase"], 쇼핑: ["shopping"], 선물: ["gift"], 타이머: ["timer"], 시계: ["clock", "watch"],
  방송: ["radio", "podcast", "cast"], 녹화: ["circle-dot", "video"], 삭제: ["trash"], 휴지통: ["trash"], 연필: ["pencil"],
  화살표: ["arrow"], 위: ["arrow-up", "chevron-up"], 아래: ["arrow-down", "chevron-down"], 왼쪽: ["arrow-left", "panel-left"],
  오른쪽: ["arrow-right", "panel-right"], 사람: ["user"], 친구: ["users"], 링크: ["link"], 다운로드: ["download"],
  업로드: ["upload"], 새로고침: ["refresh", "rotate"], 되돌리기: ["undo"], 확대: ["zoom-in", "maximize"], 축소: ["zoom-out", "minimize"],
  프린터: ["printer"], 인쇄: ["printer"], 와이파이: ["wifi"], 블루투스: ["bluetooth"], 배터리: ["battery"], 눈: ["eye"],
};

/** 한 번에 그리는 개수 (아이콘을 그때그때 불러오므로 너무 많으면 느리다) */
const LIMIT = 180;

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  current: IconName;
  onPick: (name: IconName) => void;
}

export function IconPickerDialog({ open, onOpenChange, current, onPick }: Props) {
  const t = useT().editor.icon;
  const [query, setQuery] = useState("");

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return SUGGESTED;
    const words = [q, ...Object.entries(KO_WORDS).filter(([ko]) => ko.includes(q) || q.includes(ko)).flatMap(([, en]) => en)];
    return iconNames.filter((name) => words.some((w) => name.includes(w))).slice(0, LIMIT);
  }, [query]);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t.pickTitle}</DialogTitle>
        </DialogHeader>
        <div className="relative">
          <Search className="absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            autoFocus
            className="pl-8"
            placeholder={t.search}
            value={query}
            onChange={(e) => setQuery(e.currentTarget.value)}
          />
        </div>
        {!query.trim() && <p className="text-xs text-muted-foreground">{t.suggested}</p>}
        <div className="grid h-80 grid-cols-8 content-start gap-1 overflow-y-auto">
          {shown.length === 0 ? (
            <p className="col-span-8 p-2 text-sm text-muted-foreground">{t.empty}</p>
          ) : (
            shown.map((name) => (
              <button
                key={name}
                title={name}
                onClick={() => onPick(name)}
                className={cn(
                  "flex aspect-square items-center justify-center rounded-lg border hover:bg-muted",
                  name === current ? "border-blue-500 bg-blue-500/10 text-blue-700" : "border-transparent",
                )}
              >
                <Icon name={name} className="size-5" />
              </button>
            ))
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
