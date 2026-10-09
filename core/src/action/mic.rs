//! 마이크 음소거 (Core Audio).
//!
//! Windows에는 시스템 전역 마이크 음소거 단축키가 없다. 기본 녹음 장치는 용도별로 두
//! 가지(일반 `eConsole`, 통신 `eCommunications`)이고 회의 앱은 대개 통신용을 쓰므로,
//! 둘이 다른 장치면 둘 다 같은 상태로 맞춘다.

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

/// 음소거를 뒤집고 새 상태(음소거면 `true`)를 돌려준다. 호출 스레드에 COM이 초기화되어
/// 있어야 한다.
pub fn toggle_mute() -> Result<bool, String> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("오디오 장치 목록을 열 수 없습니다: {e}"))?;
        let console = enumerator
            .GetDefaultAudioEndpoint(eCapture, eConsole)
            .map_err(|_| "연결된 마이크가 없습니다.".to_string())?;
        let volume = endpoint_volume(&console)?;
        let muted = !volume
            .GetMute()
            .map_err(|e| format!("마이크 상태를 읽을 수 없습니다: {e}"))?
            .as_bool();
        volume
            .SetMute(muted, std::ptr::null())
            .map_err(|e| format!("마이크 음소거를 바꿀 수 없습니다: {e}"))?;

        if let Ok(comm) = enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) {
            let same = matches!((console.GetId(), comm.GetId()), (Ok(a), Ok(b)) if a.to_string().ok() == b.to_string().ok());
            if !same {
                if let Ok(v) = endpoint_volume(&comm) {
                    let _ = v.SetMute(muted, std::ptr::null());
                }
            }
        }
        Ok(muted)
    }
}

unsafe fn endpoint_volume(device: &IMMDevice) -> Result<IAudioEndpointVolume, String> {
    unsafe { device.Activate(CLSCTX_ALL, None) }
        .map_err(|e| format!("마이크 볼륨 제어를 열 수 없습니다: {e}"))
}
