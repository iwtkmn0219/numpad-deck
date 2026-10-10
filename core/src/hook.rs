//! 저수준 키보드 훅 전용 스레드.
//!
//! 훅 콜백이 처리 제한 시간을 넘기면 Windows가 훅을 조용히 제거한다. 그래서:
//!
//! 1. 훅은 메시지 루프를 가진 전용 스레드에서 돈다.
//! 2. 콜백은 판정과 채널 적재만 하고 바로 반환한다. 액션 실행은 채널을 받는 쪽이 맡는다.
//! 3. 훅 제거를 알아낼 API가 없으므로 주기적으로, 그리고 세션 잠금 해제·절전 복귀 때
//!    다시 설치한다. 잠금 해제·절전 복귀 알림은 **별도 스레드**의 보이지 않는 창이 받아
//!    훅 스레드에 전달한다.
//!
//! 훅 스레드에는 창을 두지 않는다. 창이 있으면 이 앱의 창(편집기)이 맨 앞일 때 입력기
//! (IME/TSF)가 그 스레드와 통신하느라 훅 응답이 늦어지고, Windows가 키를 훅에 보내지
//! 않고 건너뛴다 (3-2 실측: 편집기가 앞이면 덱 버튼·단축키 녹화가 모두 안 됨).
//!
//! 이 스레드는 NumLock 상태가 바뀔 때도 알린다(트레이 모드 표시용). 일시정지 중에는
//! 새로 누른 키를 모두 통과시킨다.
//!
//! 이 모듈 안에서는 블로킹 작업·I/O·액션 실행을 하지 않는다.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_NUMLOCK};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    KillTimer, PeekMessageW, PostThreadMessageW, RegisterClassW, SetTimer, SetWindowsHookExW,
    UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED, MSG,
    PM_NOREMOVE, WH_KEYBOARD_LL, WINDOW_EX_STYLE, WM_APP, WM_KEYDOWN, WM_POWERBROADCAST, WM_QUIT,
    WM_SYSKEYDOWN, WM_TIMER, WM_USER, WM_WTSSESSION_CHANGE, WNDCLASSW, WS_OVERLAPPED,
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
    /// 훅을 다시 설치했다.
    Reinstalled { count: u32, reason: Reinstall },
    /// NumLock 상태. 시작할 때 한 번, 이후 바뀔 때마다 보낸다.
    NumLock(bool),
    /// 녹화 모드에서 받은 키 (주입·가짜 Shift 제외). [`crate::record::Recorder`]에 넣는다.
    Recorded { key: RawKey, down: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reinstall {
    Periodic,
    SessionUnlock,
    Resume,
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

/// 일시정지. 훅 스레드가 하나뿐이라 전역으로 둔다.
static PAUSED: AtomicBool = AtomicBool::new(false);

/// 녹화 마감 시각 ([`now_ms`] 기준). 0이면 녹화 중이 아니다.
static RECORD_UNTIL: AtomicU64 = AtomicU64::new(0);
/// 녹화 중 키 입력이 이만큼 없으면 녹화를 끝낸다. 녹화 중에는 모든 키를 막으므로, 앱이
/// 멈춰도 키보드가 먹통으로 남지 않게 훅이 스스로 끝낸다.
pub const RECORD_IDLE_MS: u64 = 5_000;

fn record_injected_for_test() -> bool {
    static FLAG: OnceLock<bool> = OnceLock::new();
    *FLAG.get_or_init(|| std::env::var_os("NPDK_RECORD_INJECTED").is_some())
}

fn now_ms() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

/// 훅 스레드 핸들. 버리면 훅을 풀고 스레드를 끝낸다.
pub struct HookHandle {
    thread_id: u32,
    join: Option<JoinHandle<()>>,
}

impl HookHandle {
    /// 일시정지 중에는 새로 누른 넘패드 키를 모두 통과시킨다. 이미 가로챈 키의 키 업은
    /// 계속 막아 짝을 맞춘다.
    pub fn set_paused(&self, paused: bool) {
        PAUSED.store(paused, Ordering::Relaxed);
    }

    pub fn paused(&self) -> bool {
        PAUSED.load(Ordering::Relaxed)
    }

    /// 단축키 녹화를 시작한다. 녹화 중에는 주입이 아닌 모든 키를 막고
    /// [`HookEvent::Recorded`]로 보낸다. [`RECORD_IDLE_MS`] 동안 키가 없으면 저절로 끝난다.
    pub fn start_recording(&self) {
        RECORD_UNTIL.store(now_ms() + RECORD_IDLE_MS, Ordering::Relaxed);
    }

    pub fn stop_recording(&self) {
        RECORD_UNTIL.store(0, Ordering::Relaxed);
    }

    /// 녹화 중인지 (시간이 지나 저절로 끝났으면 `false`)
    pub fn recording(&self) -> bool {
        let until = RECORD_UNTIL.load(Ordering::Relaxed);
        until != 0 && now_ms() < until
    }

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
    /// 마지막으로 알린 NumLock 상태
    numlock: bool,
}

/// 다른 프로그램이 NumLock을 바꾸는 경우를 대비한 확인 주기
const NUMLOCK_POLL_MS: u32 = 1_000;
/// 창 프로시저가 재설치를 요청할 때 스레드에 보내는 메시지.
const WM_REINSTALL: u32 = WM_APP + 1;
const REQUEST_UNLOCK: usize = 1;
const REQUEST_RESUME: usize = 2;
const WTS_SESSION_UNLOCK: usize = 0x8;
const PBT_APMRESUMESUSPEND: usize = 0x7;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;

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
            numlock: numlock_is_on(),
        })
    });
    let _ = events.send(HookEvent::NumLock(numlock_is_on()));

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
    // 창 없는 타이머는 넘긴 ID를 무시하고 새 ID를 돌려준다.
    let reinstall_timer = unsafe { SetTimer(None, 0, interval_ms, None) };
    let numlock_timer = unsafe { SetTimer(None, 0, NUMLOCK_POLL_MS, None) };
    let hook_thread = unsafe { GetCurrentThreadId() };
    let notify_thread = start_notify_thread(hook_thread);
    let _ = ready.send(Ok(hook_thread));

    let mut reinstalls = 0;
    loop {
        // 0이면 WM_QUIT, -1이면 오류
        let got = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if got.0 <= 0 {
            break;
        }
        let reason = match msg.message {
            WM_TIMER if msg.wParam.0 == reinstall_timer => Some(Reinstall::Periodic),
            WM_TIMER if msg.wParam.0 == numlock_timer => {
                check_numlock();
                None
            }
            WM_REINSTALL if msg.wParam.0 == REQUEST_UNLOCK => Some(Reinstall::SessionUnlock),
            WM_REINSTALL => Some(Reinstall::Resume),
            _ => None,
        };
        if let Some(reason) = reason {
            // 새 훅을 먼저 걸고 옛 훅을 푼다. 그 사이에 입력이 빠지지 않는다.
            if let Ok(fresh) = install() {
                unsafe {
                    let _ = UnhookWindowsHookEx(hook);
                }
                hook = fresh;
                reinstalls += 1;
                let _ = events.send(HookEvent::Reinstalled {
                    count: reinstalls,
                    reason,
                });
            }
            // 잠금·절전 동안 NumLock이 바뀌었을 수 있다.
            check_numlock();
        }
    }

    unsafe {
        let _ = KillTimer(None, reinstall_timer);
        let _ = KillTimer(None, numlock_timer);
        let _ = UnhookWindowsHookEx(hook);
    }
    if let Some((thread_id, join)) = notify_thread {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        let _ = join.join();
    }
    STATE.with(|s| s.borrow_mut().take());
}

/// 알림 창을 띄울 스레드. 알림을 받으면 훅 스레드에 [`WM_REINSTALL`]을 보낸다.
/// 실패해도 주기적 재설치는 돈다.
fn start_notify_thread(hook_thread: u32) -> Option<(u32, JoinHandle<()>)> {
    let (tx, rx) = mpsc::channel();
    let join = thread::Builder::new()
        .name("deck-hook-notify".into())
        .spawn(move || {
            HOOK_THREAD.with(|t| t.set(hook_thread));
            let mut msg = MSG::default();
            unsafe {
                let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
            }
            let window = create_notify_window();
            let _ = tx.send(unsafe { GetCurrentThreadId() });
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                    DispatchMessageW(&msg);
                }
                if let Some(hwnd) = window {
                    let _ = WTSUnRegisterSessionNotification(hwnd);
                    let _ = DestroyWindow(hwnd);
                }
            }
        })
        .ok()?;
    let thread_id = rx.recv().ok()?;
    Some((thread_id, join))
}

thread_local! {
    /// 알림 스레드에서: 알림을 전달할 훅 스레드
    static HOOK_THREAD: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// NumLock 상태가 바뀌었으면 알린다.
fn check_numlock() {
    STATE.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            notify_numlock(state);
        }
    });
}

fn notify_numlock(state: &mut State) {
    let now = numlock_is_on();
    if now != state.numlock {
        state.numlock = now;
        let _ = state.events.send(HookEvent::NumLock(now));
    }
}

/// 세션 잠금 해제·절전 복귀 알림을 받을 보이지 않는 창. 실패해도 주기적 재설치는 돈다.
fn create_notify_window() -> Option<HWND> {
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(notify_proc),
            hInstance: module.into(),
            lpszClassName: w!("NumpadDeckHookNotify"),
            ..Default::default()
        };
        // 이미 등록돼 있으면 실패하지만 그대로 쓸 수 있다.
        RegisterClassW(&class);
        // 메시지 전용 창은 전원 알림(브로드캐스트)을 받지 못하므로 숨은 최상위 창을 쓴다.
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("NumpadDeckHookNotify"),
            w!(""),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        )
        .ok()?;
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
        Some(hwnd)
    }
}

unsafe extern "system" fn notify_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let request = match (msg, wparam.0) {
        (WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK) => Some(REQUEST_UNLOCK),
        (WM_POWERBROADCAST, PBT_APMRESUMESUSPEND | PBT_APMRESUMEAUTOMATIC) => Some(REQUEST_RESUME),
        _ => None,
    };
    if let Some(request) = request {
        unsafe {
            let _ = PostThreadMessageW(
                HOOK_THREAD.with(|t| t.get()),
                WM_REINSTALL,
                WPARAM(request),
                LPARAM(0),
            );
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
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
    // 일시정지 중에는 새로 누른 키를 넘패드 키로 보지 않는다. 키 업은 그대로 판정해
    // 일시정지 전에 가로챈 키의 짝을 맞춘다.
    let paused = down && PAUSED.load(Ordering::Relaxed);
    let key = if ours || paused { None } else { identify(raw) };

    STATE.with(|cell| {
        let mut guard = cell.borrow_mut();
        let Some(state) = guard.as_mut() else {
            return false;
        };

        // 녹화 모드: 덱 판정을 건너뛰고 모든 키를 막아 녹화로 보낸다.
        // 시험용(NPDK_RECORD_INJECTED=1)이면 다른 프로그램이 주입한 키도 받는다.
        if !injected || (!ours && record_injected_for_test()) {
            let until = RECORD_UNTIL.load(Ordering::Relaxed);
            if until != 0 {
                let now = now_ms();
                if now < until {
                    if info.scanCode & SIMULATED_SCAN == 0 {
                        RECORD_UNTIL.store(now + RECORD_IDLE_MS, Ordering::Relaxed);
                        let _ = state.events.send(HookEvent::Recorded { key: raw, down });
                    }
                    return true;
                }
                RECORD_UNTIL.store(0, Ordering::Relaxed);
            }
        }

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
        // NumLock 키를 뗄 때는 토글이 이미 반영돼 있다.
        if raw.vk == key::vk::NUMLOCK && !down {
            notify_numlock(state);
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
