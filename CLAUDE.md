# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 현재 상태

**2단계 완료, M2 통과(2026-10-10). 3단계 진행 중(계획 `docs/stage3-plan.md`, 검증 `docs/m3-checklist.md`): 3-1 편집기 뼈대 완료(수동 검증 통과), 3-2 액션 편집 완료(수동 검증 통과), 3-3 아이콘 완료(수동 검증 통과), 3-4 프리셋 완료(수동 검증 통과, OBS는 미확인), 3-5 온보딩 완료(수동 검증 통과), 3-6 한국어·영어 완료(수동 검증 통과, 영어 Windows 첫 실행은 미확인). M3 테스터 시험 준비(안내문 `docs/m3-tester-guide.md`).** 1단계는 M1 통과(2026-10-09). Cargo 워크스페이스에 `core/` 크레이트(`numpad-deck-core`, lib 이름 `deck_core`)와 `app/src-tauri`(Tauri 2 앱 `numpad-deck`)가 있다. 앱은 트레이와 OSD 창 하나로 이뤄진다. 프런트엔드는 Vite 다중 진입점: `osd.html`(OSD), `index.html`(설정 편집기 자리, 3단계). 기획 근거는 `Numpad Deck 프로젝트 제안서.md`, M1 수동 검증 항목과 결과는 `docs/m1-checklist.md`, 2단계 계획(설정 스키마·액션 명세·OSD 동작 결정)은 `docs/stage2-plan.md`, 액션 수동 검증은 `docs/m2-checklist.md`(시험용 설정 `docs/m2-test-config.json`).

개념 검증용 AutoHotkey v2 프로토타입이 `~/Downloads/numpad-deck.ahk`에 있다(저장소 밖). 키 배치·페이지 전환·클릭 통과 OSD의 참고 구현이다.

`core/` 구성:

- `key.rs`: VK + 확장 플래그 → `NumpadKey`. NumLock ON 형태(`VK_NUMPAD7`)와 OFF 형태(`VK_HOME`)를 같은 키로 식별한다(키 다운/업 짝짓기용). Windows 의존 없음.
- `gate.rs`: 다운/업 판정(`Pass`/`Swallow`/`Fire`), 자동 반복 억제, 키 업 놓침 복구. Windows 의존 없음.
- `layout.rs`: v1.0 고정 키 배치.
- `config.rs`: 설정 JSON 스키마(`Config`/`Page`/`Button`/`Action`, 선택 항목 `language`·`layout`·`systemKeys` — `layout`은 화면에 그리는 넘패드 모양 `KeyPlace`만, `systemKeys`는 시스템 키 네 개 안에서 역할 맞바꾸기, 역할은 `Config::role`로 조회), 검증, 원자적 저장, 폴더 감시로 즉시 반영. 기본 설정은 `presets::default_config`(음악·영상 프리셋 한 페이지). 경로는 `%APPDATA%\NumpadDeck\config.json`.
- `deck.rs`: 현재 페이지와 디스패처. 넘패드 키 → `Dispatch`(버튼/페이지/OSD 토글). 페이지 키는 끝에서 반대쪽으로 돈다. 설정이 바뀌어 페이지가 줄면 마지막 페이지로 옮긴다. Windows 의존 없음.
- `record.rs`: 단축키 녹화. 녹화 모드에서 훅이 보낸 키 이벤트 → 이름 배열(보조 키 순서 Ctrl·Shift·Alt·Win, 좌우 구분 없음, Esc 취소, 넘패드·이름 없는 키 거부). Windows 의존 없음.
- `presets.rs` + `presets/*.json`: 프리셋 6개(음악·영상, 창 관리·업무, Zoom, Teams, Meet, OBS — OBS는 키보드에 없는 F13–F22). 이름·버튼 이름은 `{ko, en}`, `Lang`으로 골라 `Page`를 만든다. 첫 실행 기본 설정·편집기·온보딩이 같이 쓴다.
- `i18n.rs`: 언어(`Lang`)와 코어 메시지(설정 오류·액션 실패) 언어. 전역으로 두고 `tr(한국어, 영어)`로 고른다. 설정의 `language`, 없으면 Windows 표시 언어(`GetUserDefaultUILanguage`, 한국어가 아니면 영어).
- `keys.rs`: 단축키 키 이름(`"Ctrl"`, `"Win"`, `"F5"` …) ↔ VK·확장 플래그. 설정 검증과 (3단계) 단축키 녹화가 쓴다. Windows 의존 없음.
- `hook.rs`: 훅 전용 스레드(**창을 두지 않는다**: 창이 있으면 이 앱 창이 앞일 때 IME/TSF가 끼어 훅이 키를 못 받는다 — 3-2 실측), 주기적·세션 잠금 해제·절전 복귀 때 재설치(알림은 별도 스레드의 숨은 창이 받아 전달), NumLock 상태 변화 알림, 일시정지, **녹화 모드**(주입 아닌 모든 키를 막고 `Recorded`로 보냄, 5초 무입력이면 훅이 스스로 끝냄 — 키보드 먹통 방지), `mpsc` 채널로 `HookEvent` 전달.
- `inject.rs`: `dwExtraInfo` 서명(`SIGNATURE`)을 붙인 `SendInput`. 단축키·유니코드 텍스트 주입 전에 사용자가 누르고 있는 보조 키를 뗀다(다시 누르지는 않는다: 끼임 방지). Alt·Win을 뗄 때는 마스크 키 `0xE8`을 끼운다.
- `action/`: 액션 실행기(`Executor`). 전용 스레드 + 큐, 묶음은 별도 스레드, 실패는 `ExecEvent`로 알림. `shell.rs`(ShellExecuteEx, 오류 창 없음, `%환경변수%` 펼치기), `window.rs`(실행 파일로 창 찾기·포커스·같은 앱 다음 창), `mic.rs`(Core Audio로 연결된 **모든** 녹음 장치 음소거 토글 — 기본 장치만 바꾸면 오디오 인터페이스가 빠진다. ASIO 앱에는 적용 안 됨), `audio.rs`(rodio, 재생할 때만 장치 열고 3초 유휴 뒤 닫음 — 절전 방해 방지), `apps.rs`(`shell:AppsFolder`로 설치된 앱 목록: 일반 앱은 exe 경로, 스토어 앱은 `shell:AppsFolder\ID`).
- `bin/deck-probe.rs`: M1·M2 검증용 콘솔 프로그램.

`app/src-tauri/src/` 구성:

- `lib.rs`: Tauri 진입점. 단일 인스턴스(훅이 둘이면 버튼이 두 번 실행됨, 두 번째 실행은 편집기를 연다), 창 없이 상주(`ExitRequested` 막음), 트레이 메뉴 이벤트, command `get_config`·`save_config`·`test_action`·`list_apps`(spawn_blocking + COM)·`start_recording`·`cancel_recording`·`record_key`·`press_deck_key`·`list_presets`·`get_mode`·`get_lang`·`onboarding_pending`·`finish_onboarding`, 첫 실행(설정 파일 없음)이면 편집기를 열어 온보딩, `tauri-plugin-dialog`(파일·폴더 고르기).
- `editor.rs`: 편집기 창. 열 때 만들고 닫으면 파기. OSD와 같은 `BROWSER_ARGS`.
- `runtime.rs`: 코어 연결. 훅 이벤트를 받는 디스패처 스레드 → `Deck` → `Executor`. 설정 감시(밖에서 바뀌면 편집기에 `config://changed`), 편집기 저장(`save_config`: 코어 검증·저장 후 즉시 반영), 일시정지(`HookHandle::set_paused`), 상태를 트레이에 반영.
- `tray.rs`: 트레이 아이콘(32×32 RGBA를 코드로 그림: 숫자 모드=테두리, 덱 모드=파란 칸, 일시정지=회색·가운데 줄 빔)과 메뉴. 왼쪽 클릭 = 편집기, 오른쪽 클릭 = 메뉴. 메뉴는 `TrackPopupMenuEx`로 직접 띄워 작업 영역 안에 둔다(전체 화면에서 작업 표시줄에 가려지던 문제).
- `osd.rs`: OSD 창. 시작할 때 숨긴 채 만들고 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` + `set_ignore_cursor_events`(클릭 통과). 마우스가 있는 모니터 작업 영역 우측 하단에 `SWP_NOACTIVATE`로 띄운다. 창 조작은 모두 비동기(`SWP_ASYNCWINDOWPOS`, `ShowWindowAsync`). 내용은 `osd://update` 이벤트로 보낸다. 숨김 예약은 세대 번호로 무효화, `-` 키는 고정 토글. WebView2는 `--disable-gpu --disable-gpu-compositing`으로 띄운다(상주 메모리 약 116MB, GPU를 켜면 약 223MB). 편집기 창도 같은 인자를 써야 한다.
- **교착 주의**: `runtime.rs`는 상태 잠금 안에서 계산만 하고 트레이·OSD 갱신은 잠금을 푼 뒤 한다(`Effects` → `apply`). 트레이 갱신은 메인 스레드를 기다리는데 메뉴 처리(메인 스레드)도 같은 잠금을 잡기 때문이다.

`app/src/`: `osd/`(OSD React 화면, `types.ts`의 `OsdPayload`는 `osd.rs`의 `Payload`와 모양이 같아야 함), `editor/`(편집기: `useEditorConfig`가 자동 저장 0.3초·되돌리기 50개·같은 입력란 연속 입력 묶기, `ButtonPanel`은 동작이 완성되기 전까지 화면에만 두고 저장하지 않음, `actions/`는 동작 카드·종류별 입력 화면·`meta.ts`의 완성 검사 — 코어 검증보다 너그러우면 안 됨, `Onboarding.tsx`는 첫 실행 3단계 안내와 키 입력 확인 화면 — NumLock·넘패드는 화면이 받은 키로 판단), `shared/layout.ts`(OSD·편집기 공용 넘패드 배치: 표준 배치 + 설정의 `layout`으로 그리는 자리만 바꿈, 끌어서 바꾸는 화면은 `editor/LayoutEditor.tsx`), `shared/icons.tsx`(Lucide 아이콘을 이름으로 필요할 때만 불러옴 `lucide-react/dynamic`, 고르지 않은 버튼은 동작에 맞는 기본 아이콘 `defaultIcon` — OSD는 버튼 전체를 받아 같은 규칙으로 그린다), `i18n/`(UI 문자열 `ko.ts`·`en.ts` — 모양이 같아야 함, 언어는 코어가 정하고 `lang://changed`로 알림, 창 진입점에서 `initI18n()`), `bindings/`(**ts-rs 생성물, 손으로 고치지 말 것**: `core/src/config.rs` 타입을 바꾸면 `cargo test -p numpad-deck-core`로 다시 만든다), `components/ui/`(shadcn/ui, Nova 프리셋, 글꼴은 Segoe UI·맑은 고딕), `index.css`(Tailwind v4).

## 작업 방식

- 사용자에게 물어볼 것이 있으면 텍스트로 나열하지 말고 **질의응답 도구(AskUserQuestion)로 한 번에 하나씩** 묻는다. 답을 받은 뒤 다음 질문으로 넘어간다.
- 커밋 메시지에 `Co-Authored-By` 줄을 넣지 않는다 (사용자 요청으로 기존 커밋에서도 모두 지웠다).
- 사용자 PC에 클릭·글자 입력을 보내는 UI 자동 시험을 하지 않는다 (창이 앞으로 오지 않아 다른 앱에 입력된 사고가 있었다). 화면 캡처는 괜찮고, 넘패드 키 주입은 F24처럼 무해한 동작에만. 클릭·타이핑이 필요한 확인은 체크리스트로 사용자에게 맡긴다.

## 빌드·실행 명령

Rust stable(MSVC) + VS 2022 Build Tools 필요. Git Bash에서는 `~/.cargo/bin`이 PATH에 없을 수 있다.

```
cargo build
cargo test                              # 단위 테스트 (action::shell·window 일부는 실제 Windows API 호출)
cargo clippy --workspace --all-targets
cargo fmt
cargo run --bin deck-probe              # 훅을 걸고 판정 결과 출력 (Ctrl+C로 종료)
cargo run --bin deck-probe -- --quiet --loop-test --reinstall-secs 10
cargo run --bin deck-probe -- --config  # 설정 파일을 읽고 감시, 저장하면 즉시 반영
cargo run --bin deck-probe -- --quiet --config --config-file docs/m2-test-config.json --run  # 액션 실제 실행
cargo run --bin deck-probe -- --mics [--mute-test]  # 녹음 장치 목록·기본 장치·음소거 상태 진단
cargo run -p numpad-deck                # 트레이 앱 실행 (OSD 화면은 Vite 개발 서버가 있어야 보임)
cd app && npm install && npm run tauri dev   # 앱 + OSD 개발 실행
cd app && npm run build                 # 프런트엔드 타입 검사·빌드
cd app && npm run tauri build -- --bundles nsis   # 시험용 설치 파일 (target/release/bundle/nsis, 서명 없음)
cargo test -p numpad-deck-core gate::   # 테스트 하나만
```

## 이 앱이 하는 일

Windows 상주 앱. 본체 키보드의 넘패드를 **NumLock이 꺼졌을 때만** 스트림덱 같은 매크로 패드로 바꾼다. NumLock ON이면 평소처럼 숫자가 입력된다. 대상 키는 NumLock을 제외한 넘패드 16개(0–9, `.` `/` `*` `-` `+` Enter)다.

핵심 통찰: 키보드는 NumLock 상태와 무관하게 같은 스캔 코드를 보내고 해석은 OS가 한다. 따라서 NumLock OFF 상태의 넘패드 키는 전용 Home·방향키와 **구분 가능한 별도 키**로 다룰 수 있다. 기존 키 배치를 전혀 건드리지 않고 "숨은 키 16개"를 얻는 것이 이 프로젝트의 전제다.

주 사용자는 **개발 지식이 없는 일반 PC 사용자**다. 설계 판단이 갈릴 때는 이 기준으로 결정한다: 설치 5분 내 첫 버튼 실행, 코드·단축키 문자열·파일 경로를 모르고도 편집 가능, 현재 모드가 항상 보임, 보안 경고 없이 설치됨.

## 아키텍처

Rust 코어(훅·입력 판정·액션 실행)와 React UI(설정 편집기·OSD)를 Tauri 2로 묶는다. 둘은 **Tauri command와 event로만** 통신한다. 코어는 UI 없이도 동작해야 하며, 개발 1단계에서 콘솔 프로그램으로 먼저 검증한 뒤 화면을 붙인다.

- **코어**: Rust + `windows-rs`. GC가 없어 훅 응답 시간이 일정하다.
- **UI**: React + TypeScript + Tailwind CSS + shadcn/ui.
- **설정 저장**: JSON 파일(`serde`) + 파일 변경 감지. 저장하면 재시작 없이 즉시 반영된다. 사람이 읽고 백업·공유할 수 있어야 한다.
- **상주·배포**: Tauri 트레이 + autostart 플러그인, NSIS 설치 파일, Tauri updater. 배포 채널은 Microsoft Store / winget / GitHub Releases.

### 훅 스레드 규율 (가장 중요한 제약)

저수준 키보드 훅이 처리 제한 시간을 넘기면 **Windows가 훅을 조용히 제거한다.** 따라서:

1. 훅은 메시지 루프를 가진 **전용 스레드**에서 돌린다.
2. 훅 콜백은 **판정과 큐 적재만** 하고 즉시 반환한다. 액션 실행은 별도 스레드가 맡는다.
3. 훅 상태를 주기적으로 점검해 제거되었으면 재설치한다.

훅 콜백 안에서 블로킹 작업·I/O·액션 실행을 하는 코드는 이 설계를 깨뜨린다.

### 덱 모드 판정

조건은 두 개다: **NumLock이 꺼져 있다** AND **넘패드 키다.** 둘 다 만족하는 입력만 가로채 디스패처로 넘기고, 나머지는 손대지 않고 통과시킨다.

넘패드 키 식별은 가상 키 코드 + 확장 키 플래그(`LLKHF_EXTENDED`) 조합으로 한다:

| 넘패드 키 | NumLock OFF일 때 VK | 일반 키와 구분하는 방법 |
| --- | --- | --- |
| 7 8 9 4 6 1 2 3 | `VK_HOME` `VK_UP` `VK_PRIOR` `VK_LEFT` `VK_RIGHT` `VK_END` `VK_DOWN` `VK_NEXT` | 확장 플래그 **없음** (전용 이동 키는 있음) |
| 5 | `VK_CLEAR` | 넘패드에만 있는 키 |
| 0, `.` | `VK_INSERT`, `VK_DELETE` | 확장 플래그 **없음** (전용 Insert·Delete는 있음) |
| Enter | `VK_RETURN` | 확장 플래그 **있음** (메인 Enter는 없음) |
| `/` `*` `-` `+` | `VK_DIVIDE` `VK_MULTIPLY` `VK_SUBTRACT` `VK_ADD` | 넘패드 전용 VK. **NumLock 영향을 받지 않으므로 NumLock 상태를 따로 조회해야 한다** |

NumLock 상태는 캐싱하지 말고 **키 입력마다 다시 조회한다.** 원격 데스크톱·가상 머신에서 상태가 불일치하기 때문이다. 훅 스레드에서 `GetKeyState(VK_NUMLOCK)`로 조회해도 정확함을 M1에서 실측했다(Shift 조합 포함). NumLock ON + Shift + 넘패드 7은 `VK_HOME`으로 들어오므로 VK만으로 판정하지 말 것.

- **키 업**: 다운을 막았으면 업도 막고, 통과시켰으면 업도 통과시킨다. "가로챈 키 집합"을 기록해 업을 판정한다(누른 채 NumLock이 바뀌는 경우 대비).
- **자동 반복**: 반복 다운은 무시한다. 버튼 1회 = 액션 1회.
- **훅 생존**: 생존 조회 API가 없으므로 주기적으로, 그리고 세션 잠금 해제·절전 복귀 때 재설치한다.

### 키 배치 (기본값, 시스템 키 네 개끼리는 역할을 바꿀 수 있음)

| 키 | 역할 |
| --- | --- |
| 7 8 9 4 5 6 1 2 3, 0, `+`, Enter | 버튼 (페이지당 12개) |
| `/` / `*` | 이전 / 다음 페이지 |
| `-` | OSD 띄우기/숨기기 |
| `.` | 첫 페이지로 |

### 자기 입력 재가로채기 방지

액션 실행기가 `SendInput`으로 보낸 입력을 훅이 다시 잡으면 무한 반복이 된다. 주입 입력 플래그(`LLKHF_INJECTED`)와 `dwExtraInfo` 서명으로 자기 입력을 식별해 무시한다.

## v1.0 범위

**필수**: NumLock 레이어 입력, 페이지, 액션(단축키 전송 / 프로그램 실행·포커스 / URL·폴더 열기 / 텍스트 입력(`KEYEVENTF_UNICODE`) / 미디어·볼륨 제어 / 마이크 음소거(Core Audio) / 효과음 재생, 지연 시간을 끼운 순차 묶음), 클릭 통과 OSD, GUI 설정 편집기(단축키 직접 녹화, 설치된 앱 목록에서 선택), 프리셋·아이콘 세트, 온보딩·트레이 모드 표시, 설치 파일·자동 업데이트·트레이 상주·일시정지·자동 실행, 한국어/영어 UI.

**v1.x 권장**: 시스템 키(페이지 이동·OSD 호출) 재지정, 설정 내보내기·가져오기, "관리자로 실행" 옵션.

**범위 밖 (v1.0에서 만들지 말 것)**: 앱별 자동 프로필, 길게 누르기·더블탭, 플러그인, 보조 키보드 구분, macOS·Linux 지원. 모두 v2 이후 후보다.

## 주의할 함정

- **편집기(이 앱의 WebView2 창)가 앞이면 키 다운이 저수준 키보드 훅을 거치지 않는다** (3-2 실측: 훅 호출 수가 늘지 않음, 별도 프로세스의 훅도 마찬가지, 훅을 1초마다 다시 설치해도 같음, 관리자 권한 문제 아님). 그래서 편집기 안의 단축키 녹화와 넘패드 덱 키는 화면이 받은 키(`KeyboardEvent.code` → `app/src/editor/keymap.ts`)를 command `record_key`·`press_deck_key`로 코어에 넘긴다. 코어 훅의 녹화 모드는 프로브(`--record`)에서만 쓴다. 진단은 `NPDK_DIAG=1 npm run tauri dev`(1초마다 훅 호출 수·디스패처 처리 수).

- **Shift + 넘패드**: NumLock ON + Shift + 넘패드 7이면 Windows가 가짜 Shift 업(`sc=0x22A`, 스캔 코드 0x200 비트, 확장 플래그 없음) → `VK_HOME`(확장 없음) → 넘패드 키 업 → 가짜 Shift 다운(`sc=0x2A`, 진짜와 구분 불가)을 보낸다(M1 실측). 덱 판정은 `GetKeyState(VK_NUMLOCK)`로 정확했다. 단축키 녹화에서는 가짜 Shift 업을 무시해야 한다.
- **UIPI**: 관리자 권한 창(작업 관리자 등)이 앞에 있으면 일반 권한 훅은 입력을 받지 못하고 넘패드가 평소처럼 동작한다. v1.0은 안내만 한다.
- **배포**: Microsoft Store(MSIX, Microsoft 서명) 우선, GitHub(NSIS + updater)·winget 보조. Store 빌드에서는 Tauri updater를 끈다.
- **OSD 활성화**: Tauri의 창 표시가 창을 활성화할 수 있으므로 `WS_EX_NOACTIVATE` / `SW_SHOWNOACTIVATE`를 직접 지정한다. OSD 창은 하나만 미리 만들어 숨겨 두고, 편집기 창은 닫을 때 파기한다.
- **키로거 오탐**: SmartScreen 경고와 백신 오탐이 일반 사용자 이탈의 가장 큰 원인이다. 키 입력을 저장하거나 전송하지 않는다는 것이 지켜야 할 제품 약속이다.
- **OSD는 포커스를 뺏지 않는다**: 항상 위 + 캡션 없음 + 클릭 통과(`WS_EX_TRANSPARENT`). 프로토타입의 `+E0x20` GUI 옵션이 이것에 해당한다.
- **노트북 내장 넘패드 / 넘패드 없는 키보드**: 키 코드가 달라 인식에 실패한다. v1.0은 넘패드 있는 키보드만 공식 지원하되 키 입력 확인 화면을 제공한다.
