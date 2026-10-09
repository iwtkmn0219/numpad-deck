# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 현재 상태

**2단계 진행 중: 2-1 설정, 2-2 덱 상태, 2-3 액션 실행기 완료(수동 검증 통과), 2-4 트레이 앱 완료(수동 검증 통과). 다음은 2-5 OSD.** 1단계는 M1 통과(2026-10-09). Cargo 워크스페이스에 `core/` 크레이트(`numpad-deck-core`, lib 이름 `deck_core`)와 `app/src-tauri`(Tauri 2 앱 `numpad-deck`)가 있다. 앱은 아직 창 없이 트레이만 있다(React 프런트엔드는 템플릿 그대로, 2-5 OSD부터 사용). 기획 근거는 `Numpad Deck 프로젝트 제안서.md`, M1 수동 검증 항목과 결과는 `docs/m1-checklist.md`, 2단계 계획(설정 스키마·액션 명세·OSD 동작 결정)은 `docs/stage2-plan.md`, 액션 수동 검증은 `docs/m2-checklist.md`(시험용 설정 `docs/m2-test-config.json`).

개념 검증용 AutoHotkey v2 프로토타입이 `~/Downloads/numpad-deck.ahk`에 있다(저장소 밖). 키 배치·페이지 전환·클릭 통과 OSD의 참고 구현이다.

`core/` 구성:

- `key.rs`: VK + 확장 플래그 → `NumpadKey`. NumLock ON 형태(`VK_NUMPAD7`)와 OFF 형태(`VK_HOME`)를 같은 키로 식별한다(키 다운/업 짝짓기용). Windows 의존 없음.
- `gate.rs`: 다운/업 판정(`Pass`/`Swallow`/`Fire`), 자동 반복 억제, 키 업 놓침 복구. Windows 의존 없음.
- `layout.rs`: v1.0 고정 키 배치.
- `config.rs`: 설정 JSON 스키마(`Config`/`Page`/`Button`/`Action`), 검증, 원자적 저장, 폴더 감시로 즉시 반영. 기본 설정은 `default_config.json`(`include_str!`). 경로는 `%APPDATA%\NumpadDeck\config.json`.
- `deck.rs`: 현재 페이지와 디스패처. 넘패드 키 → `Dispatch`(버튼/페이지/OSD 토글). 페이지 키는 끝에서 반대쪽으로 돈다. 설정이 바뀌어 페이지가 줄면 마지막 페이지로 옮긴다. Windows 의존 없음.
- `keys.rs`: 단축키 키 이름(`"Ctrl"`, `"Win"`, `"F5"` …) ↔ VK·확장 플래그. 설정 검증과 (3단계) 단축키 녹화가 쓴다. Windows 의존 없음.
- `hook.rs`: 훅 전용 스레드, 주기적·세션 잠금 해제·절전 복귀 때 재설치(숨은 창으로 알림 수신), NumLock 상태 변화 알림, 일시정지, `mpsc` 채널로 `HookEvent` 전달.
- `inject.rs`: `dwExtraInfo` 서명(`SIGNATURE`)을 붙인 `SendInput`. 단축키·유니코드 텍스트 주입 전에 사용자가 누르고 있는 보조 키를 뗀다(다시 누르지는 않는다: 끼임 방지). Alt·Win을 뗄 때는 마스크 키 `0xE8`을 끼운다.
- `action/`: 액션 실행기(`Executor`). 전용 스레드 + 큐, 묶음은 별도 스레드, 실패는 `ExecEvent`로 알림. `shell.rs`(ShellExecuteEx, 오류 창 없음, `%환경변수%` 펼치기), `window.rs`(실행 파일로 창 찾기·포커스·같은 앱 다음 창), `mic.rs`(Core Audio로 연결된 **모든** 녹음 장치 음소거 토글 — 기본 장치만 바꾸면 오디오 인터페이스가 빠진다. ASIO 앱에는 적용 안 됨), `audio.rs`(rodio, 재생할 때만 장치 열고 3초 유휴 뒤 닫음 — 절전 방해 방지).
- `bin/deck-probe.rs`: M1·M2 검증용 콘솔 프로그램.

`app/src-tauri/src/` 구성:

- `lib.rs`: Tauri 진입점. 단일 인스턴스(훅이 둘이면 버튼이 두 번 실행됨), 창 없이 상주(`ExitRequested` 막음), 트레이 메뉴 이벤트.
- `runtime.rs`: 코어 연결. 훅 이벤트를 받는 디스패처 스레드 → `Deck` → `Executor`. 설정 감시, 일시정지(`HookHandle::set_paused`), 상태를 트레이에 반영.
- `tray.rs`: 트레이 아이콘(32×32 RGBA를 코드로 그림: 숫자 모드=테두리, 덱 모드=파란 칸, 일시정지=회색·가운데 줄 빔)과 메뉴.

## 작업 방식

- 사용자에게 물어볼 것이 있으면 텍스트로 나열하지 말고 **질의응답 도구(AskUserQuestion)로 한 번에 하나씩** 묻는다. 답을 받은 뒤 다음 질문으로 넘어간다.

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
cargo run -p numpad-deck                # 트레이 앱 실행 (창 없음, 프런트엔드 빌드 불필요)
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

### 키 배치 (v1.0 고정)

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

- **Shift + 넘패드**: NumLock ON + Shift + 넘패드 7이면 Windows가 가짜 Shift 업(`sc=0x22A`, 스캔 코드 0x200 비트, 확장 플래그 없음) → `VK_HOME`(확장 없음) → 넘패드 키 업 → 가짜 Shift 다운(`sc=0x2A`, 진짜와 구분 불가)을 보낸다(M1 실측). 덱 판정은 `GetKeyState(VK_NUMLOCK)`로 정확했다. 단축키 녹화에서는 가짜 Shift 업을 무시해야 한다.
- **UIPI**: 관리자 권한 창(작업 관리자 등)이 앞에 있으면 일반 권한 훅은 입력을 받지 못하고 넘패드가 평소처럼 동작한다. v1.0은 안내만 한다.
- **배포**: Microsoft Store(MSIX, Microsoft 서명) 우선, GitHub(NSIS + updater)·winget 보조. Store 빌드에서는 Tauri updater를 끈다.
- **OSD 활성화**: Tauri의 창 표시가 창을 활성화할 수 있으므로 `WS_EX_NOACTIVATE` / `SW_SHOWNOACTIVATE`를 직접 지정한다. OSD 창은 하나만 미리 만들어 숨겨 두고, 편집기 창은 닫을 때 파기한다.
- **키로거 오탐**: SmartScreen 경고와 백신 오탐이 일반 사용자 이탈의 가장 큰 원인이다. 키 입력을 저장하거나 전송하지 않는다는 것이 지켜야 할 제품 약속이다.
- **OSD는 포커스를 뺏지 않는다**: 항상 위 + 캡션 없음 + 클릭 통과(`WS_EX_TRANSPARENT`). 프로토타입의 `+E0x20` GUI 옵션이 이것에 해당한다.
- **노트북 내장 넘패드 / 넘패드 없는 키보드**: 키 코드가 달라 인식에 실패한다. v1.0은 넘패드 있는 키보드만 공식 지원하되 키 입력 확인 화면을 제공한다.
