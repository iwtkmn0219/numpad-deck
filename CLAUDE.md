# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 현재 상태

**2단계 완료, M2 통과(2026-10-10). 3단계 진행 중(계획 `docs/stage3-plan.md`, 검증 `docs/m3-checklist.md` — 체크리스트는 로컬 전용): 3-1 편집기 뼈대 완료(수동 검증 통과), 3-2 액션 편집 완료(수동 검증 통과), 3-3 아이콘 완료(수동 검증 통과), 3-4 프리셋 완료(수동 검증 통과, OBS는 미확인), 3-5 온보딩 완료(수동 검증 통과), 3-6 한국어·영어 완료(수동 검증 통과, 영어 Windows 첫 실행은 미확인). M3 테스터 시험 준비(안내문 `docs/m3-tester-guide.md`).** 1단계는 M1 통과(2026-10-09). Cargo 워크스페이스에 `core/` 크레이트(`numpad-deck-core`, lib 이름 `deck_core`)와 `app/src-tauri`(Tauri 2 앱 `numpad-deck`)가 있다. 앱은 트레이와 OSD 창 하나로 이뤄진다. 프런트엔드는 Vite 다중 진입점: `osd.html`(OSD), `index.html`(설정 편집기 자리, 3단계). 기획 근거는 `Numpad Deck 프로젝트 제안서.md`, M1 수동 검증 항목과 결과는 `docs/m1-checklist.md`, 2단계 계획(설정 스키마·액션 명세·OSD 동작 결정)은 `docs/stage2-plan.md`, 액션 수동 검증은 `docs/m2-checklist.md`(시험용 설정 `docs/m2-test-config.json`).

`core/` 구성:

- `key.rs`: VK + 확장 플래그 → `NumpadKey`. NumLock ON 형태(`VK_NUMPAD7`)와 OFF 형태(`VK_HOME`)를 같은 키로 식별한다(키 다운/업 짝짓기용). Windows 의존 없음.
- `gate.rs`: 다운/업 판정(`Pass`/`Swallow`/`Fire`), 자동 반복 억제, 키 업 놓침 복구. Windows 의존 없음.
- `layout.rs`: v1.0 고정 키 배치.
- `config.rs`: 설정 JSON 스키마(`Config`/`Page`/`Button`/`Action` — 문장 액션 `Text`의 선택 항목 `method`(`paste`|`type`, 없으면 `type`, 편집기에서 새로 만드는 버튼만 `paste`; `deny_unknown_fields`라 0.7.x 이하 앱은 `method`가 있는 설정·공유 파일을 거부 — 받아들인 호환 손실), 선택 항목 `language`·`layout`·`systemKeys`·`micIndicator`(마이크 상시 표시, 없으면 켬, 마이크 버튼이 있어야 동작 — `Config::mic_indicator_active`, 묶음 안 마이크도 셈 — `Config::uses_mic_mute`) — `layout`은 화면에 그리는 넘패드 모양 `KeyPlace`만, `systemKeys`는 시스템 키 네 개 안에서 역할 맞바꾸기, 역할은 `Config::role`로 조회), 검증, 원자적 저장, 폴더 감시로 즉시 반영. 기본 설정은 `presets::default_config`(음악·영상 프리셋 한 페이지). 경로는 `%APPDATA%\NumpadDeck\config.json`.
- `deck.rs`: 현재 페이지와 디스패처. 넘패드 키 → `Dispatch`(버튼/페이지/OSD 토글). 페이지 키는 끝에서 반대쪽으로 돈다. 설정이 바뀌어 페이지가 줄면 마지막 페이지로 옮긴다. Windows 의존 없음.
- `record.rs`: 단축키 녹화. 녹화 모드에서 훅이 보낸 키 이벤트 → 이름 배열(보조 키 순서 Ctrl·Shift·Alt·Win, 좌우 구분 없음, Esc 취소, 넘패드·이름 없는 키 거부). Windows 의존 없음.
- `presets.rs` + `presets/*.json`: 프리셋 6개(음악·영상, 창 관리·업무, Zoom, Teams, Meet, OBS — OBS는 키보드에 없는 F13–F22). 이름·버튼 이름은 `{ko, en}`, `Lang`으로 골라 `Page`를 만든다. 첫 실행 기본 설정·편집기·온보딩이 같이 쓴다.
- `i18n.rs`: 언어(`Lang`)와 코어 메시지(설정 오류·액션 실패) 언어. 전역으로 두고 `tr(한국어, 영어)`로 고른다. 설정의 `language`, 없으면 Windows 표시 언어(`GetUserDefaultUILanguage`, 한국어가 아니면 영어).
- `text_template.rs`: 문장 자리표시자 펼치기 `expand`(누르는 순간 값). `{날짜}` `{날짜:긴}` `{날짜:짧은}` `{시간}` `{요일}` `{클립보드}`와 영어 이름(`{date}` … 소문자만), 출력 언어는 앱 언어. 이름이 정확히 맞을 때만 바꾸고 표에 없는 `{...}`·짝 없는 괄호는 그대로, 겹친 중괄호는 `{{`+표의 이름+`}}`일 때만 `{이름}`으로 줄이고 그 밖의 `{{`·`}}`는 그대로 — 표의 이름이 없는 문장은 입력과 정확히 같다(기존 문장 불변, 전수 시험으로 고정). 클립보드는 자리표시자가 있을 때만 한 번 읽고 글자가 없으면 오류(아무것도 보내지 않음). 시각·클립보드는 밖에서 넣는다(`Moment`, 클로저). 편집기 미리 보기 `app/src/editor/textTemplate.ts`는 같은 규칙의 TS 구현 — 한쪽을 바꾸면 다른 쪽도. Windows 의존 없음.
- `clipboard.rs`: 붙여넣기용 클립보드. 창 없이 연다(`OpenClipboard(None)`, 10ms × 10회 재시도 — 실행 스레드에 소유 창을 두면 메시지를 돌리지 않아 다른 앱의 복사가 멈출 수 있다). 보관 `snapshot`(HGLOBAL 형식만, GDI 핸들·`CF_PRIVATE*`·`DataObject`/`Ole Private Data`·OLE로만 읽히는 `FileContents`와 짝 `FileGroupDescriptor(W)`는 뺌 — 탐색기 파일은 `CF_HDROP`로 되살아남, OLE 클립보드에서 알리기만 하고 오류 없이 NULL인 등록 형식(메모장의 `EnterpriseDataProtectionId` 등)은 건너뜀 — 단 HTML·RTF·PNG·Csv·Biff* 같은 서식 있는 내용은 건너뛰지 않음, 32MiB 넘거나 그 밖의 읽기 실패면 오류. 진단 `deck-probe --clipboard`), 넣기 `put_text_private`(기록 제외 형식 `ExcludeClipboardContentFromMonitorProcessing`·`CanIncludeInClipboardHistory`=0·`CanUploadToCloudClipboard`=0을 **글자보다 먼저** 넣고 실패하면 다시 비움 — Win+V 기록·클라우드에 문장이 남지 않게, 오류는 비웠는지 `emptied`를 알림), 되돌리기 `restore`(순번이 같을 때만, 기록 제외 형식 포함), `RESTORE_DELAY` 400ms. 실제 클립보드를 쓰는 자동 시험은 두지 않는다(순수 판단 함수만 시험).
- `keys.rs`: 단축키 키 이름(`"Ctrl"`, `"Win"`, `"F5"` …) ↔ VK·확장 플래그. 설정 검증과 (3단계) 단축키 녹화가 쓴다. Windows 의존 없음.
- `hook.rs`: 훅 전용 스레드(**창을 두지 않는다**: 창이 있으면 이 앱 창이 앞일 때 IME/TSF가 끼어 훅이 키를 못 받는다 — 3-2 실측), 주기적·세션 잠금 해제·절전 복귀 때 재설치(알림은 별도 스레드의 숨은 창이 받아 전달), NumLock 상태 변화 알림, 일시정지, **녹화 모드**(주입 아닌 모든 키를 막고 `Recorded`로 보냄, 5초 무입력이면 훅이 스스로 끝냄 — 키보드 먹통 방지), `mpsc` 채널로 `HookEvent` 전달.
- `inject.rs`: `dwExtraInfo` 서명(`SIGNATURE`)을 붙인 `SendInput`(붙여넣기 `paste_shortcut` = Ctrl+V 포함). 단축키·유니코드 텍스트 주입 전에 사용자가 누르고 있는 보조 키를 뗀다(다시 누르지는 않는다: 끼임 방지). Alt·Win을 뗄 때는 마스크 키 `0xE8`을 끼운다.
- `action/`: 액션 실행기(`Executor`). 전용 스레드 + 큐, 묶음은 별도 스레드, 실패는 `ExecEvent`로 알림. `shell.rs`(ShellExecuteEx, 오류 창 없음, `%환경변수%` 펼치기), `window.rs`(실행 파일로 창 찾기·포커스·같은 앱 다음 창), `mic.rs`(Core Audio로 연결된 **모든** 녹음 장치 음소거 토글 — 기본 장치만 바꾸면 오디오 인터페이스가 빠진다. ASIO 앱에는 적용 안 됨. `all_muted`로 상태 조회: 모두 음소거여야 꺼짐, 장치가 없으면 None), `audio.rs`(rodio, 재생할 때만 장치 열고 3초 유휴 뒤 닫음 — 절전 방해 방지), `apps.rs`(`shell:AppsFolder`로 설치된 앱 목록: 일반 앱은 exe 경로, 스토어 앱은 `shell:AppsFolder\ID`), `text.rs`(문장 버튼: 현지 시각으로 자리표시자를 펼친 뒤 `paste`면 보관 → 넣기 → Ctrl+V → 400ms → 되돌리기, 보관·넣기에 실패하면 직접 입력으로 대신 보냄(비웠으면 먼저 되돌림), `type`이면 `inject::text`. 묶음 스레드와 실행 스레드가 겹쳐 클립보드를 잃지 않게 `TEXT_LOCK`으로 한 번에 하나만. 대신 보낸 이유는 `eprintln!` — 개발 실행에서만 보임).
- `share.rs`: 페이지 공유·설정 백업 파일 `.npdeck`(ZIP: `manifest.json` + `sounds/<sha256 앞 16 hex>.<wav|mp3|ogg|flac>`, `FORMAT=1`, `kind` pages|backup). 내보내기(`export_check`·`export`: 실행·폴더·효과음 경로의 사용자 폴더 앞부분을 `%LOCALAPPDATA%`→`%APPDATA%`→`%USERPROFILE%`로 바꿈 `portable_path`, 효과음은 내용 해시 이름으로 한 번만 담음, 소리 5MiB·합계 20MiB), 읽기(`read`: **받은 파일은 믿지 않는다** — 파일 30MiB·항목 512개·manifest 1MiB·소리 5MiB/합계 20MiB를 압축을 푼 실제 바이트로 셈, 허용 이름 외 항목·경로 조작은 읽지 않고 무시, 소리는 해시가 이름과 맞아야 받음, 더 새 `format`은 거부, 기존 `Config::validate` 통과), 미리 보기(`preview`: 버튼마다 주의(`launch` `open` `hotkey` `text`와 이를 담은 묶음)·이 PC에 없음 — URI 스킴(`ms-settings:` 등)과 경로 없는 실행 이름은 판정 안 함, 파일 확인은 `Probe`로 주입), 설치(`install`: 추가는 이름 겹치면 "이름 (2)", 바꾸기는 백업 파일만, 소리는 `%APPDATA%\NumpadDeck\sounds\`에 임시 파일 후 이름 바꾸기), 자동 백업(`backup_config`: 설정과 앱 버전·`UserDirs`를 받아 `backups\config-YYYYMMDD-HHMMSS[-n].npdeck` 현지 시각으로 지금 설정 전체를 내보냄 — "백업 파일 가져오기"로 되돌림, 담을 수 없는 효과음은 오류 없이 빼고 버튼은 둠 `export_with`; 설정 파일이 깨졌을 때만 디스크 원본을 `.json`으로도 복사(`BackupMode`: 정상=`Export`, 깨졌고 시작부터 기본 설정이면 `Raw`=.json만, 깨졌지만 앱이 마지막 정상 설정으로 돌면 `Both`; `Runtime::backup_source`가 한 번의 잠금으로 정함); `.json`·`.npdeck` 한 묶음으로 최신 10개, 같은 초 번호는 두 확장자 모두 확인, 방금 만든 것은 지우지 않음).
- `bin/deck-probe.rs`: M1·M2 검증용 콘솔 프로그램.

`app/src-tauri/src/` 구성:

- `lib.rs`: Tauri 진입점. 단일 인스턴스(훅이 둘이면 버튼이 두 번 실행됨, 두 번째 실행은 편집기를 연다), 창 없이 상주(`ExitRequested` 막음), 트레이 메뉴 이벤트, command `get_config`·`save_config`·`test_action`·`list_apps`(spawn_blocking + COM)·`start_recording`·`cancel_recording`·`record_key`·`press_deck_key`·`list_presets`·`get_mode`·`get_lang`·`onboarding_pending`·`finish_onboarding`, 첫 실행(설정 파일 없음)이면 편집기를 열어 온보딩, `tauri-plugin-dialog`(파일·폴더 고르기).
- 앱 아이콘(`icons/`): 원본 `icons/icon.svg`(차콜 바탕 3×3 키캡, 오른쪽 위 파랑). `app/scripts/make-icons.py`(Pillow)가 모든 파일을 만든다 — 16~44px는 줄이지 않고 픽셀 격자에 맞춰 따로 그린다. `tauri icon`으로 다시 만들지 말 것(작은 크기가 번진다).
- `editor.rs`: 편집기 창. 열 때 만들고 닫으면 파기. OSD와 같은 `BROWSER_ARGS`.
- `runtime.rs`: 코어 연결. 훅 이벤트를 받는 디스패처 스레드 → `Deck` → `Executor`. 설정 감시(밖에서 바뀌면 편집기에 `config://changed`), 편집기 저장(`save_config`: 코어 검증·저장 후 즉시 반영), 일시정지(`HookHandle::set_paused`), 상태를 트레이에 반영. `deck-mic-watch` 스레드가 시작할 때 한 번, 그 뒤로는 상시 표시가 켜져 있을 때만 1초마다 마이크 상태를 읽어 바뀌면 트레이·OSD·배지에 반영(버튼이 바꾼 직후 읽은 오래된 값은 `mic_gen`으로 버림, COM은 MTA로 초기화, COM 조회는 이 스레드에서만).
- `tray.rs`: 트레이 아이콘(3×3 칸을 코드로 그림: 덱 모드=채움, 숫자 모드=테두리, 일시정지=가운데 줄 빔. 작업 표시줄 배율의 크기(16·20·24·32, 표에 없는 크기는 아래 크기를 가운데)로 직접 그려 번지지 않게 하고, 색은 작업 표시줄 밝기(`SystemUsesLightTheme`)에 맞춤 — `theme.rs`가 보이지 않는 최상위 창으로 `WM_SETTINGCHANGE`·배율 변경을 받아 다시 그린다)과 메뉴. 왼쪽 클릭 = 편집기, 오른쪽 클릭 = 메뉴. 마이크 꺼짐(상시 표시 켜짐)이면 오른쪽 아래 빨간 점과 메뉴 상태 줄 "마이크 꺼짐". 메뉴는 `TrackPopupMenuEx`로 직접 띄워 작업 영역 안에 둔다(전체 화면에서 작업 표시줄에 가려지던 문제).
- `osd.rs`: OSD 창. 시작할 때 숨긴 채 만들고 `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW` + `set_ignore_cursor_events`(클릭 통과). 마우스가 있는 모니터 작업 영역 우측 하단에 `SWP_NOACTIVATE`로 띄운다. 창 조작은 모두 비동기(`SWP_ASYNCWINDOWPOS`, `ShowWindowAsync`). 내용은 `osd://update` 이벤트로 보낸다. 숨김 예약은 세대 번호로 무효화, `-` 키는 고정 토글. WebView2는 `--disable-gpu --disable-gpu-compositing`으로 띄운다(상주 메모리 약 116MB, GPU를 켜면 약 223MB). 편집기 창도 같은 인자를 써야 한다.
- `share.rs`(앱): 공유 파일 command `export_check`·`export_share`·`open_share`·`install_share`·`take_share_request`(모두 async + `spawn_blocking`). `ShareState`가 미리 보기 중인 묶음 하나를 `preview_id`와 함께 들고 있고(열 때마다 새 id, 다른 id로 설치하면 거부), 바꾸기·설정 로드 오류가 있을 때는 저장 전에 `backup_config`, 저장은 `Runtime::save_config`. 더블클릭 열기: 시작 인자(`args_os`, `share_arg`)나 단일 인스턴스 콜백의 `.npdeck` 경로를 열 파일 요청에 넣고 편집기를 열어 `share://open`(편집기는 시작할 때도 `take_share_request`, 온보딩 중이면 끝난 뒤). 확장자 등록은 `tauri.conf.json` `bundle.fileAssociations`(NSIS가 HKCU에 쓰고 제거 때 지움, Store(MSIX)는 출시 작업에서 따로).
- `mic_badge.rs`: 마이크 꺼짐 배지. WebView가 아닌 Win32 레이어드 창(`WS_EX_LAYERED | WS_EX_TRANSPARENT`, 클릭 통과·포커스 안 뺏음)에 GDI+로 직접 그린다(미리 곱한 알파 DIB → `UpdateLayeredWindow`) — WebView 배지는 렌더러 프로세스가 하나 늘어 상주 메모리가 약 20MB(작업 관리자 기준) 늘었다. 창은 자기 스레드(`deck-mic-badge`)의 메시지 루프에 살고, 처음 보여야 할 때 스레드와 창을 만든다(마이크 버튼이 없으면 둘 다 없음). 바깥(`set_visible(seq, visible)`·`reposition`·`set_lang`)은 `PostMessageW`로 명령만 보내고 기다리지 않는다(메인 스레드에서 불러도 교착 없음). 표시 요청의 순번(seq, 상태 잠금 안에서 매김)으로 늦게 온 오래된 값은 무시, 같은 값이면 건너뜀. 주 모니터 작업 영역 위쪽 가운데에 주 모니터 배율(`GetDpiForMonitor`)로 그 해상도에 맞춰 그린다(화면·배율·작업 영역이 바뀌면 `theme::watch`가 `reposition`, 언어가 바뀌면 `set_lang`으로 다시 그림). 모양은 예전 WebView 배지와 같게: 차콜 알약 + 흰색 10% 테두리, 빨간 원(`#E7000B`) 안 아이콘(Segoe Fluent Icons/MDL2 `U+F781` 사선 마이크), 문구는 코어 언어(`i18n::current`)를 따름(맑은 고딕 굵게 / Segoe UI Semibold, 글자 윤곽에 반투명 선을 더해 브라우저만큼 진하게). 화면 공유·녹화에서 빠짐(`WDA_EXCLUDEFROMCAPTURE`, Win10 2004+). 테스트는 `NPDK_BADGE_DUMP=폴더`면 그린 배지를 BMP로 남긴다.
- **교착 주의**: `runtime.rs`는 상태 잠금 안에서 계산만 하고 트레이·OSD 갱신은 잠금을 푼 뒤 한다(`Effects` → `apply`). 트레이 갱신은 메인 스레드를 기다리는데 메뉴 처리(메인 스레드)도 같은 잠금을 잡기 때문이다.

`app/src/`: `osd/`(OSD React 화면, `types.ts`의 `OsdPayload`는 `osd.rs`의 `Payload`와 모양이 같아야 함), `editor/`(`share/`는 내보내기 흐름 `ExportFlow.tsx`(문장 입력·담지 못하는 소리 확인 → 저장 창)와 가져오기 창 `ImportDialog.tsx`(키캡 미리 보기·버튼별 설명·주의·없음 태그, 페이지로 추가 / 백업이면 설정 바꾸기 — 바꾼 뒤에는 되돌리기 기록을 비움 `useEditorConfig.adopt`·`forgetHistory`), 편집기: `useEditorConfig`가 자동 저장 0.3초·되돌리기 50개·같은 입력란 연속 입력 묶기, `ButtonPanel`은 동작이 완성되기 전까지 화면에만 두고 저장하지 않음, `actions/`는 동작 카드·종류별 입력 화면·`meta.ts`의 완성 검사 — 코어 검증보다 너그러우면 안 됨, `Onboarding.tsx`는 첫 실행 5단계 안내(언어·NumLock·넘패드 1·배치 맞추기·프리셋)와 키 입력 확인 화면 — NumLock·넘패드는 화면이 받은 키로 판단), `shared/layout.ts`(OSD·편집기 공용 넘패드 배치: 표준 배치 + 설정의 `layout`으로 그리는 자리만 바꿈, 끌어서 바꾸는 화면은 `editor/LayoutEditor.tsx`), `shared/icons.tsx`(Lucide 아이콘을 이름으로 필요할 때만 불러옴 `lucide-react/dynamic`, 고르지 않은 버튼은 동작에 맞는 기본 아이콘 `defaultIcon` — OSD는 버튼 전체를 받아 같은 규칙으로 그린다), `i18n/`(UI 문자열 `ko.ts`·`en.ts` — 모양이 같아야 함, 언어는 코어가 정하고 `lang://changed`로 알림, 창 진입점에서 `initI18n()`), `bindings/`(**ts-rs 생성물, 손으로 고치지 말 것**: `core/src/config.rs` 타입을 바꾸면 `cargo test -p numpad-deck-core`로 다시 만든다), `components/ui/`(shadcn/ui, Nova 프리셋, 글꼴은 Segoe UI·맑은 고딕), `index.css`(Tailwind v4).

## 작업 방식

- **브랜치 (2026-10-12부터)**: 기능·수정마다 `develop`에서 `feature/…` 브랜치를 만들어 작업하고, 사용자 확인이 끝나면 `develop`에 병합한다(병합한 기능 브랜치는 지운다). `main`에는 사용자가 **버전을 올리자고 할 때만** `develop`을 병합하고 그 커밋에 `v0.x.0` 태그를 단다. 원격은 `origin` = github.com/iwtkmn0219/numpad-deck (공개 저장소). `develop`에 병합할 때 `develop`을, 버전을 올릴 때 `main`과 태그를 올린다. 기능 브랜치는 로컬에만. 수동 시험 체크리스트(`docs/*-checklist.md`, 개인 시험 기록)는 `.gitignore`로 **로컬에만** 둔다.

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
cd app && TAURI_SIGNING_PRIVATE_KEY=~/.tauri/numpad-deck.key TAURI_SIGNING_PRIVATE_KEY_PASSWORD= npm run tauri build -- --bundles nsis
                                        # 설치 파일 + 업데이트 서명(.sig) (target/release/bundle/nsis). 키가 없으면 빌드 실패
cd app && node scripts/latest-json.mjs "노트"   # 자동 업데이트용 latest.json
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

**v1.x 권장**: 시스템 키(페이지 이동·OSD 호출) 재지정, 설정 내보내기·가져오기(완료: 페이지 공유·백업 파일 `.npdeck`), "관리자로 실행" 옵션.

**범위 밖 (v1.0에서 만들지 말 것)**: 앱별 자동 프로필, 길게 누르기·더블탭, 플러그인, 보조 키보드 구분, macOS·Linux 지원. 모두 v2 이후 후보다.

## 주의할 함정

- **편집기(이 앱의 WebView2 창)가 앞이면 키 다운이 저수준 키보드 훅을 거치지 않는다** (3-2 실측: 훅 호출 수가 늘지 않음, 별도 프로세스의 훅도 마찬가지, 훅을 1초마다 다시 설치해도 같음, 관리자 권한 문제 아님). 그래서 편집기 안의 단축키 녹화와 넘패드 덱 키는 화면이 받은 키(`KeyboardEvent.code` → `app/src/editor/keymap.ts`)를 command `record_key`·`press_deck_key`로 코어에 넘긴다. 코어 훅의 녹화 모드는 프로브(`--record`)에서만 쓴다. 진단은 `NPDK_DIAG=1 npm run tauri dev`(1초마다 훅 호출 수·디스패처 처리 수).

- **Shift + 넘패드**: NumLock ON + Shift + 넘패드 7이면 Windows가 가짜 Shift 업(`sc=0x22A`, 스캔 코드 0x200 비트, 확장 플래그 없음) → `VK_HOME`(확장 없음) → 넘패드 키 업 → 가짜 Shift 다운(`sc=0x2A`, 진짜와 구분 불가)을 보낸다(M1 실측). 덱 판정은 `GetKeyState(VK_NUMLOCK)`로 정확했다. 단축키 녹화에서는 가짜 Shift 업을 무시해야 한다.
- **UIPI**: 관리자 권한 창(작업 관리자 등)이 앞에 있으면 일반 권한 훅은 입력을 받지 못하고 넘패드가 평소처럼 동작한다. v1.0은 안내만 한다.
- **자동 업데이트 / 릴리스 절차**: 앱(`update.rs`)은 `…/releases/latest/download/latest.json`을 확인한다. 릴리스는 ① 버전 올리고 `main` 병합·태그 ② 위 명령으로 서명 빌드 ③ `latest-json.mjs` ④ `gh release create vX.Y.Z <setup.exe> <setup.exe.sig> <latest.json>` — **사전 릴리스로 올리면 안 된다**(latest에서 빠져 업데이트가 안 보임). 업데이트 서명 비밀 키는 `~/.tauri/numpad-deck.key`(저장소 밖, 비밀번호 없음). **잃어버리면 기존 설치에 업데이트를 보낼 수 없다** — 따로 백업. 공개 키는 `tauri.conf.json`.
- **배포**: Microsoft Store(MSIX, Microsoft 서명) 우선, GitHub(NSIS + updater)·winget 보조. Store 빌드에서는 Tauri updater를 끈다.
- **OSD 활성화**: Tauri의 창 표시가 창을 활성화할 수 있으므로 `WS_EX_NOACTIVATE` / `SW_SHOWNOACTIVATE`를 직접 지정한다. OSD 창은 하나만 미리 만들어 숨겨 두고, 편집기 창은 닫을 때 파기한다.
- **키로거 오탐**: SmartScreen 경고와 백신 오탐이 일반 사용자 이탈의 가장 큰 원인이다. 키 입력을 저장하거나 전송하지 않는다는 것이 지켜야 할 제품 약속이다.
- **OSD는 포커스를 뺏지 않는다**: 항상 위 + 캡션 없음 + 클릭 통과(`WS_EX_TRANSPARENT`). 프로토타입의 `+E0x20` GUI 옵션이 이것에 해당한다.
- **노트북 내장 넘패드 / 넘패드 없는 키보드**: 키 코드가 달라 인식에 실패한다. v1.0은 넘패드 있는 키보드만 공식 지원하되 키 입력 확인 화면을 제공한다.
