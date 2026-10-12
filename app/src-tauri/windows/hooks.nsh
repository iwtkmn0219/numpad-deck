; NSIS 설치 파일 훅 (tauri.conf.json bundle.windows.nsis.installerHooks)

; 제거할 때 "Windows 시작 시 실행" 등록도 지운다. 값 이름은 tauri-plugin-autostart가 쓰는
; 앱 이름(productName)이다. 없으면 아무 일도 하지 않는다.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Numpad Deck"
!macroend
