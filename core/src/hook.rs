//! 저수준 키보드 훅 전용 스레드.
//!
//! 훅 콜백이 처리 제한 시간을 넘기면 Windows가 훅을 조용히 제거한다. 그래서:
//!
//! 1. 훅은 메시지 루프를 가진 전용 스레드에서 돈다.
//! 2. 콜백은 판정과 채널 적재만 하고 바로 반환한다. 액션 실행은 채널을 받는 쪽이 맡는다.
//! 3. 훅 제거를 알아낼 API가 없으므로 주기적으로 다시 설치한다.
//!    세션 잠금 해제·절전 복귀 때 재설치는 창이 필요하므로 2단계(Tauri)에서 붙인다.
//!
//! 이 모듈 안에서는 블로킹 작업·I/O·액션 실행을 하지 않는다.

use std::cell::RefCell;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_NUMLOCK};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, KillTimer, PeekMessageW, PostThreadMessageW, SetTimer,
    SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED,
    LLKHF_INJECTED, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN, WM_TIMER,
    WM_USER,
};

use crate::gate::{Decision, KeyGate};
use crate::inject;
use crate::key::{self, identify, numlock_hint, NumpadKey, RawKey};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone, Copy)]
pub enum HookEvent {
    /// 덱 모드에서 넘패드 키가 눌렸다.
    Fire(NumpadKey),
    /// 진단용 기록. [`HookConfig::trace`]가 켜져 있을 때만 보낸다.
    Trace(Trace),
    /// 주기적 재설치가 끝났다.
    Reinstalled { count: u32 },
}

/// 훅이 본 키 이벤트 하나. 넘패드 키, Shift, NumLock, 자기 입력만 기록한다.
#[derive(Debug, Clone, Copy)]
pub struct Trace {
    pub vk: u16,
    pub scan: u32,
    pub extended: bool,
    pub injected: bool,
    /// 이 앱이 주입한 입력
    pub ours: bool,
    pub down: bool,
    /// `KBDLLHOOKSTRUCT::time`
    pub time_ms: u32,
    pub key: Option<NumpadKey>,
    /// 판정에 쓴 NumLock 상태 (`GetKeyState`). 넘패드 키가 아니면 조회하지 않는다.
    pub numlock_on: Option<bool>,
    /// VK로 짐작한 NumLock 상태. `numlock_on`과 비교하는 진단용이다. 주입 입력은
    /// OS가 번역한 VK가 아니므로 짐작하지 않는다.
    pub numlock_hint: Option<bool>,
    pub decision: Decision,
    /// 콜백 안에서 쓴 시간
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub struct HookConfig {
    pub trace: bool,
    pub reinstall_every: Duration,
}

impl Default for HookConfig {
    fn default() -> Self {
        Self {
            trace: false,
            reinstall_every: Duration::from_secs(60),
        }
    }
}

/// 훅 스레드 핸들. 버리면 훅을 풀고 스레드를 끝낸다.
pub struct HookHandle {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl HookHandle {
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        if let Some(join) = self.join.take() {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            let _ = join.join();
        }
    }
}

impl Drop for HookHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 훅 스레드를 시작한다. 훅 설치에 성공해야 반환한다.
pub fn start(config: HookConfig, events: Sender<HookEvent>) -> Result<HookHandle, Error> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let join = thread::Builder::new()
        .name("deck-hook".into())
        .spawn(move || run(config, events, ready_tx))?;
    let thread_id = ready_rx.recv()??;
    Ok(HookHandle {
        thread_id,
        join: Some(join),
    })
}

struct State {
    gate: KeyGate,
    events: Sender<HookEvent>,
    trace: bool,
    /// 사용자가 실제로 Shift를 누르고 있는지. Windows가 끼워 넣는 가짜 Shift 업은
    /// 무시한다. 진단용 `numlock_hint`에만 쓴다.
    shift_held: bool,
}

/// Windows가 NumLock ON + Shift + 넘패드 때 끼워 넣는 가짜 Shift 업은 스캔 코드에
/// 이 비트가 붙어 들어온다 (실측: `0x22A`). 확장 플래그는 붙지 않는다.
/// 뒤따르는 가짜 Shift 다운은 일반 `0x2A`로 들어와 구분되지 않는다.
const SIMULATED_SCAN: u32 = 0x200;

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn run(config: HookConfig, events: Sender<HookEvent>, ready: Sender<Result<u32, Error>>) {
    let mut msg = MSG::default();
    // 메시지 큐를 먼저 만들어야 PostThreadMessageW(WM_QUIT)가 이 스레드에 닿는다.
    unsafe {
        let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
    }

    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            gate: KeyGate::new(),
            events: events.clone(),
            trace: config.trace,
            shift_held: false,
        })
    });

    let mut hook = match install() {
        Ok(hook) => hook,
        Err(e) => {
            let _ = ready.send(Err(e.into()));
            return;
        }
    };
    let interval_ms = config
        .reinstall_every
        .as_millis()
        .clamp(1_000, u32::MAX as u128) as u32;
    let timer = unsafe { SetTimer(None, 0, interval_ms, None) };
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut reinstalls = 0;
    loop {
        // 0이면 WM_QUIT, -1이면 오류
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        if msg.message == WM_TIMER {
            // 새 훅을 먼저 걸고 옛 훅을 푼다. 그 사이에 입력이 빠지지 않는다.
            if let Ok(fresh) = install() {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
                hook = fresh;
                reinstalls += 1;
                let _ = events.send(HookEvent::Reinstalled { count: reinstalls });
            }
        }
    }

    unsafe {
        let _ = KillTimer(None, timer);
        let _ = UnhookWindowsHookEx(hook);
    }
    STATE.with(|s| s.borrow_mut().take());
}

fn install() -> windows::core::Result<HHOOK> {
    unsafe {
        let module = GetModuleHandleW(None)?;
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0)
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if handle(info, down) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 판정하고 채널에 적재한다. 막아야 하면 `true`.
fn handle(info: &KBDLLHOOKSTRUCT, down: bool) -> bool {
    let started = Instant::now();
    let injected = info.flags.0 & LLKHF_INJECTED.0 != 0;
    let ours = injected && inject::is_ours(info.dwExtraInfo);
    let raw = RawKey {
        vk: info.vkCode as u16,
        extended: info.flags.0 & LLKHF_EXTENDED.0 != 0,
    };
    let key = if ours { None } else { identify(raw) };

    STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return false;
        };

        // 캐싱하지 않고 키 입력마다 조회한다 (원격 데스크톱·가상 머신 대비).
        let numlock_on = key.map(|_| numlock_is_on());
        let decision = if down {
            state
                .gate
                .on_down(key, numlock_on.unwrap_or(false), info.time)
        } else {
            state.gate.on_up(key)
        };

        if let Decision::Fire(k) = decision {
            let _ = state.events.send(HookEvent::Fire(k));
        }

        if is_shift_vk(raw.vk) && !injected && info.scanCode & SIMULATED_SCAN == 0 {
            state.shift_held = down;
        }
        // Shift를 누른 채면 NumLock ON이어도 이동 키 형태로 들어오므로 VK로 짐작할 수 없다.
        let hint = if injected || state.shift_held {
            None
        } else {
            numlock_hint(raw)
        };

        if state.trace && (key.is_some() || ours || is_traced_vk(raw.vk)) {
            let _ = state.events.send(HookEvent::Trace(Trace {
                vk: raw.vk,
                scan: info.scanCode,
                extended: raw.extended,
                injected,
                ours,
                down,
                time_ms: info.time,
                key,
                numlock_on,
                numlock_hint: hint,
                decision,
                elapsed: started.elapsed(),
            }));
        }

        decision.swallows()
    })
}

/// 넘패드 키는 아니지만 판정에 영향을 주므로 기록하는 키 (Shift, NumLock).
fn is_traced_vk(vk: u16) -> bool {
    is_shift_vk(vk) || vk == key::vk::NUMLOCK
}

fn is_shift_vk(vk: u16) -> bool {
    matches!(vk, key::vk::SHIFT | key::vk::LSHIFT | key::vk::RSHIFT)
}

fn numlock_is_on() -> bool {
    unsafe { GetKeyState(VK_NUMLOCK.0 as i32) & 1 != 0 }
}
