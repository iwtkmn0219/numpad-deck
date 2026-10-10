// 한국어 UI 문자열. 영어(en.ts)는 3-6에서 같은 모양으로 추가한다.

export const ko = {
  system: {
    "/": "◀ 이전",
    "*": "다음 ▶",
    "-": "화면",
    ".": "처음",
  },
  osd: {
    mode: { deck: "덱 모드", numbers: "숫자 모드", paused: "일시정지" },
    numlockLabel: "NumLock",
    numlockOn: "켜짐",
    numlockOff: "꺼짐",
    numbersHint: "NumLock을 끄면 버튼을 쓸 수 있어요.",
  },
  editor: {
    windowTitle: "Numpad Deck 설정",
    addPage: "페이지 추가",
    newPageName: "새 페이지",
    pageMenu: {
      rename: "이름 바꾸기",
      moveLeft: "왼쪽으로",
      moveRight: "오른쪽으로",
      delete: "삭제",
    },
    deletePage: {
      title: (name: string) => `"${name}" 페이지를 삭제할까요?`,
      body: (count: number) =>
        count > 0 ? `이 페이지의 버튼 ${count}개가 함께 지워집니다. 되돌리기(Ctrl+Z)로 복구할 수 있어요.` : "빈 페이지입니다.",
      cancel: "취소",
      confirm: "삭제",
      lastPage: "페이지가 하나뿐이라 삭제할 수 없어요.",
    },
    status: {
      saved: "저장됨",
      saving: "저장 중…",
      failed: (reason: string) => `저장하지 못했어요: ${reason}`,
    },
    undo: "되돌리기",
    redo: "다시 실행",
    grid: {
      empty: "비어 있음",
      systemHint: "모든 페이지에서 같은 시스템 키",
    },
    panel: {
      pickHint: "왼쪽에서 버튼을 골라 주세요.",
      systemTitle: (label: string) => `${label} 키`,
      systemBody: "페이지를 넘기거나 화면을 띄우는 시스템 키라 바꿀 수 없어요.",
      buttonTitle: (key: string) => `${key} 버튼`,
      emptyTitle: (key: string) => `${key} 자리 (비어 있음)`,
      label: "이름",
      labelPlaceholder: "OSD에 보일 이름",
      action: "동작",
      clear: "버튼 지우기",
      comingSoon: "동작 고르기·바꾸기는 다음 단계(3-2)에서 추가됩니다.",
    },
    action: {
      hotkey: (keys: string) => `단축키 ${keys}`,
      launch: (target: string) => `프로그램 실행·전환: ${target}`,
      open: (target: string) => `열기: ${target}`,
      text: (text: string) => `글자 입력: ${text}`,
      media: {
        playPause: "재생/정지",
        next: "다음 곡",
        prev: "이전 곡",
        stop: "정지",
        volumeUp: "볼륨 높이기",
        volumeDown: "볼륨 낮추기",
        volumeMute: "소리 끄기/켜기",
      },
      micMute: "마이크 음소거 켜기/끄기",
      sound: (file: string) => `효과음: ${file}`,
      sequence: (count: number) => `여러 동작 차례로 (${count}단계)`,
      delay: (ms: number) => `${ms / 1000}초 기다리기`,
    },
  },
};

export type Dict = typeof ko;
