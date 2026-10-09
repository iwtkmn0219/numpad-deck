//! 마이크 음소거 (Core Audio).
//!
//! Windows에는 시스템 전역 마이크 음소거 단축키가 없다. 연결된 **모든** 녹음 장치를
//! 한꺼번에 음소거하고 켠다. 기본 장치만 바꾸면 오디오 인터페이스처럼 앱이 직접 고른
//! 장치가 빠진다 (실측: UR22C는 기본 장치가 아닌 녹음 장치 3개로 잡혔다).
//!
//! 여기서 바꾸는 것은 Windows 오디오 엔진의 음소거다. ASIO나 WASAPI 배타 모드로 장치를
//! 직접 여는 앱(DAW, 일부 방송 설정)은 이를 거치지 않으므로 음소거되지 않는다.

use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, ERole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, STGM_READ};

/// 하나라도 켜져 있으면 전부 음소거하고, 전부 음소거돼 있으면 전부 켠다. 새 상태
/// (음소거면 `true`)를 돌려준다. 호출 스레드에 COM이 초기화되어 있어야 한다.
pub fn toggle_mute() -> Result<bool, String> {
    unsafe {
        let enumerator = enumerator()?;
        let devices = enumerator
            .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("녹음 장치 목록을 읽을 수 없습니다: {e}"))?;
        let volumes: Vec<IAudioEndpointVolume> = (0..devices.GetCount().unwrap_or(0))
            .filter_map(|i| devices.Item(i).ok())
            .filter_map(|d| endpoint_volume(&d).ok())
            .collect();
        if volumes.is_empty() {
            return Err("연결된 마이크가 없습니다.".into());
        }
        let any_on = volumes
            .iter()
            .any(|v| v.GetMute().is_ok_and(|m| !m.as_bool()));
        let mute = any_on;
        let changed = volumes
            .iter()
            .filter(|v| v.SetMute(mute, std::ptr::null()).is_ok())
            .count();
        if changed == 0 {
            return Err("마이크 음소거를 바꿀 수 없습니다.".into());
        }
        Ok(mute)
    }
}

/// 진단용: 연결된 녹음 장치 하나.
#[derive(Debug, Clone)]
pub struct MicInfo {
    pub id: String,
    pub name: String,
    pub default_console: bool,
    pub default_communications: bool,
    /// 읽지 못하면 `None`
    pub muted: Option<bool>,
    /// 장치가 하드웨어 음소거를 지원한다고 알리는지
    pub hardware_mute: bool,
}

/// 진단용: 연결된(활성) 녹음 장치 목록.
pub fn list() -> Result<Vec<MicInfo>, String> {
    unsafe {
        let enumerator = enumerator()?;
        let default_id = |role: ERole| {
            enumerator
                .GetDefaultAudioEndpoint(eCapture, role)
                .ok()
                .and_then(|d| device_id(&d))
        };
        let console = default_id(eConsole);
        let comm = default_id(eCommunications);
        let devices = enumerator
            .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("녹음 장치 목록을 읽을 수 없습니다: {e}"))?;
        let count = devices.GetCount().unwrap_or(0);
        let mut list = Vec::new();
        for i in 0..count {
            let Ok(device) = devices.Item(i) else {
                continue;
            };
            let id = device_id(&device).unwrap_or_default();
            let name = device
                .OpenPropertyStore(STGM_READ)
                .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
                .map(|v| v.to_string())
                .unwrap_or_else(|_| "(이름 없음)".into());
            let volume = endpoint_volume(&device).ok();
            list.push(MicInfo {
                default_console: console.as_deref() == Some(id.as_str()),
                default_communications: comm.as_deref() == Some(id.as_str()),
                muted: volume
                    .as_ref()
                    .and_then(|v| v.GetMute().ok())
                    .map(|b| b.as_bool()),
                hardware_mute: volume
                    .as_ref()
                    .and_then(|v| v.QueryHardwareSupport().ok())
                    .is_some_and(|mask| mask & 0x2 != 0), // ENDPOINT_HARDWARE_SUPPORT_MUTE
                id,
                name,
            });
        }
        Ok(list)
    }
}

/// 진단용: 특정 장치의 음소거를 정한다.
pub fn set_mute(id: &str, muted: bool) -> Result<(), String> {
    unsafe {
        let enumerator = enumerator()?;
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = enumerator
            .GetDevice(PCWSTR(wide.as_ptr()))
            .map_err(|e| format!("장치를 찾을 수 없습니다: {e}"))?;
        endpoint_volume(&device)?
            .SetMute(muted, std::ptr::null())
            .map_err(|e| format!("음소거를 바꿀 수 없습니다: {e}"))
    }
}

unsafe fn enumerator() -> Result<IMMDeviceEnumerator, String> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|e| format!("오디오 장치 목록을 열 수 없습니다: {e}"))
}

unsafe fn device_id(device: &IMMDevice) -> Option<String> {
    unsafe {
        let raw = device.GetId().ok()?;
        let id = raw.to_string().ok();
        CoTaskMemFree(Some(raw.0 as *const _));
        id
    }
}

unsafe fn endpoint_volume(device: &IMMDevice) -> Result<IAudioEndpointVolume, String> {
    unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|e| format!("마이크 볼륨 제어를 열 수 없습니다: {e}"))
}
