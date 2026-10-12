//! 마이크 꺼짐 배지: 마이크가 꺼져 있는 동안 주 모니터 위쪽 가운데에 계속 보인다.
//!
//! OSD처럼 포커스를 뺏지 않고 클릭이 통과한다. 화면 공유·녹화에는 잡히지 않는다
//! (`WDA_EXCLUDEFROMCAPTURE`). WebView를 쓰지 않고 Win32 레이어드 창에 GDI+로 직접 그린다
//! (WebView 창이면 렌더러 프로세스가 하나 늘어 상주 메모리가 약 20MB 늘었다).
//!
//! 창은 자기 스레드(`deck-mic-badge`)의 메시지 루프에 산다. 마이크 버튼이 없는 사용자에게는
//! 스레드도 창도 만들지 않도록 처음 보여야 할 때 띄운다. 바깥에서는 `PostMessageW`로 명령만
//! 보내고 기다리지 않는다 (메인 스레드에서 불러도 막히지 않는다).
//!
//! 주 모니터 배율로 그 해상도에 맞춰 직접 그린다. 보일 때마다, 자리를 다시 잡을 때마다,
//! 언어가 바뀔 때마다 다시 그린다.

use std::sync::{Arc, Mutex};

use deck_core::i18n::Lang;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW,
    MonitorFromPoint, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, MONITORINFO, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::Graphics::GdiPlus as gp;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, IsWindowVisible, PostMessageW,
    RegisterClassW, SetWindowDisplayAffinity, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, HWND_TOPMOST, MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE,
    SW_SHOWNOACTIVATE, ULW_ALPHA, WDA_EXCLUDEFROMCAPTURE, WM_APP, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

/// 보이기 (그리고 자리 잡기)
const MSG_SHOW: u32 = WM_APP + 1;
const MSG_HIDE: u32 = WM_APP + 2;
/// 보이는 중이면 다시 그리고 자리 잡기 (모니터·배율·작업 영역·언어가 바뀐 뒤)
const MSG_REDRAW: u32 = WM_APP + 3;

/// 작업 영역 위에서 띄우는 거리 (물리 픽셀) + 알약 위 여백 (논리 픽셀, 예전 WebView 창과 같은 자리)
const MARGIN: i32 = 12;
const TOP_INSET: f32 = 7.0;

// 크기는 예전 WebView 배지(Tailwind)와 같다 (논리 픽셀): 알약 높이 42(테두리 1 + 위아래 6 +
// 원 28), 왼쪽 9(테두리 1 + 8), 원 28, 간격 8, 글자 14px, 오른쪽 17(16 + 테두리 1).
const PILL_H: f32 = 42.0;
const PAD_LEFT: f32 = 9.0;
const CIRCLE: f32 = 28.0;
const GAP: f32 = 8.0;
const PAD_RIGHT: f32 = 17.0;
const FONT_PX: f32 = 14.0;
const ICON_PX: f32 = 16.0;
/// 글자 윤곽에 반투명 흰색 선(1 논리 픽셀)을 한 번 더 그어 예전 배지만큼 진하게 한다
/// (채우기만 하면 가늘고, 불투명 선은 너무 굵다)
const TEXT_STROKE: u32 = 0x50FFFFFF;

const CHARCOAL: u32 = 0xFF1F1E1C;
/// 흰색 10%
const BORDER: u32 = 0x1AFFFFFF;
/// Tailwind red-600
const RED: u32 = 0xFFE7000B;
const WHITE: u32 = 0xFFFFFFFF;
/// 사선 그은 마이크 (Segoe Fluent Icons / Segoe MDL2 Assets "MicOff2")
const MIC_OFF_GLYPH: &str = "\u{F781}";
const PARGB: i32 = 0x000E200B; // PixelFormat32bppPARGB

#[derive(Default)]
struct Inner {
    /// 만든 창 (스레드 사이로 옮기려고 정수). 아직 없으면 `None`.
    hwnd: Option<isize>,
    /// 창 스레드를 띄웠다 (창이 생기기 전 포함)
    started: bool,
    /// 마지막으로 요청받은 보임 여부
    wanted: bool,
    /// 창에 보낸 보임 여부
    applied: Option<bool>,
    /// 마지막으로 받은 순번. 이보다 작거나 같은 요청은 오래된 것이라 무시한다.
    seq: u64,
}

pub struct MicBadge {
    inner: Arc<Mutex<Inner>>,
}

impl MicBadge {
    pub fn new() -> MicBadge {
        MicBadge {
            inner: Arc::default(),
        }
    }

    /// 보이거나 숨긴다. 어느 스레드에서 불러도 막히지 않는다. `seq`가 더 큰 요청만 받는다.
    pub fn set_visible(&self, seq: u64, visible: bool) {
        let mut inner = self.inner.lock().unwrap();
        if seq <= inner.seq {
            return;
        }
        inner.seq = seq;
        inner.wanted = visible;
        if let Some(hwnd) = inner.hwnd {
            sync(&mut inner, hwnd);
        } else if visible && !inner.started {
            inner.started = true;
            let shared = Arc::clone(&self.inner);
            let spawned = std::thread::Builder::new()
                .name("deck-mic-badge".into())
                .spawn(move || run(shared));
            if let Err(e) = spawned {
                eprintln!("마이크 배지 스레드를 만들 수 없습니다: {e}");
                inner.started = false;
            }
        }
    }

    /// 보이는 중이면 다시 그리고 자리를 잡는다 (모니터·배율·작업 영역이 바뀐 뒤).
    pub fn reposition(&self) {
        self.post(MSG_REDRAW);
    }

    /// 언어가 바뀌었다: 보이는 중이면 새 문구로 다시 그린다.
    pub fn set_lang(&self, _lang: Lang) {
        self.post(MSG_REDRAW);
    }

    fn post(&self, msg: u32) {
        if let Some(hwnd) = self.inner.lock().unwrap().hwnd {
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 요청받은 상태와 창에 보낸 상태가 다르면 보낸다.
fn sync(inner: &mut Inner, hwnd: isize) {
    if inner.applied == Some(inner.wanted) {
        return;
    }
    inner.applied = Some(inner.wanted);
    let msg = if inner.wanted { MSG_SHOW } else { MSG_HIDE };
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), LPARAM(0));
    }
}

/// 배지 스레드: GDI+를 켜고 창을 만든 뒤 메시지 루프를 돈다.
fn run(shared: Arc<Mutex<Inner>>) {
    unsafe {
        let mut token = 0usize;
        let input = gp::GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        if gp::GdiplusStartup(&mut token, &input, std::ptr::null_mut()) != gp::Ok {
            eprintln!("마이크 배지: GDI+를 시작할 수 없습니다");
            shared.lock().unwrap().started = false;
            return;
        }
        let hwnd = match create_window() {
            Ok(h) => h,
            Err(e) => {
                eprintln!("마이크 배지 창을 만들 수 없습니다: {e}");
                shared.lock().unwrap().started = false;
                return;
            }
        };
        {
            let mut inner = shared.lock().unwrap();
            inner.hwnd = Some(hwnd.0 as isize);
            sync(&mut inner, hwnd.0 as isize);
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

unsafe fn create_window() -> windows::core::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("NumpadDeckMicBadge"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("NumpadDeckMicBadge"),
            w!("Numpad Deck Mic"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        // 화면 공유·녹화에는 빼고 내 모니터에만 보인다 (회의 상대가 배지를 보지 않게).
        // Windows 10 2004 이전에는 실패하고, 그때는 공유 화면에도 보인다.
        let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
        Ok(hwnd)
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            MSG_SHOW => {
                draw_and_place(hwnd);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                // 다른 항상 위 창(전체 화면 앱 등)보다 위로
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                LRESULT(0)
            }
            MSG_HIDE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            MSG_REDRAW => {
                if IsWindowVisible(hwnd).as_bool() {
                    draw_and_place(hwnd);
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// 주 모니터 배율로 배지를 그려 작업 영역 위쪽 가운데에 올린다.
unsafe fn draw_and_place(hwnd: HWND) {
    unsafe {
        let monitor = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let (mut dpi, mut dpi_y) = (96u32, 96u32);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y);
        let scale = dpi as f32 / 96.0;
        let Some(image) = Image::render(deck_core::i18n::current(), scale) else {
            return;
        };
        let work = info.rcWork;
        let pos = POINT {
            x: work.left + (work.right - work.left - image.width) / 2,
            y: work.top + MARGIN + (TOP_INSET * scale).round() as i32,
        };
        image.upload(hwnd, pos);
    }
}

/// 미리 곱한 알파 32bpp DIB에 그린 배지.
struct Image {
    dc: windows::Win32::Graphics::Gdi::HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    old: windows::Win32::Graphics::Gdi::HGDIOBJ,
    width: i32,
    height: i32,
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

impl Image {
    unsafe fn render(lang: Lang, scale: f32) -> Option<Image> {
        unsafe {
            let text = match lang {
                Lang::Ko => "마이크 꺼짐",
                Lang::En => "Mic muted",
            };
            let font_family = if lang == Lang::Ko {
                w!("Malgun Gothic")
            } else {
                w!("Segoe UI Semibold")
            };
            // 한국어 글꼴(맑은 고딕)은 semibold가 없어 굵게 (브라우저도 같다)
            let font_style = if lang == Lang::Ko {
                gp::FontStyleBold.0
            } else {
                gp::FontStyleRegular.0
            };
            let font = Font::new(font_family, FONT_PX * scale, font_style)
                .or_else(|| Font::new(w!("Segoe UI"), FONT_PX * scale, gp::FontStyleBold.0))?;
            let icon = Font::new(w!("Segoe Fluent Icons"), ICON_PX * scale, 0)
                .or_else(|| Font::new(w!("Segoe MDL2 Assets"), ICON_PX * scale, 0))?;
            let format = StringFormat::typographic()?;
            let wide: Vec<u16> = text.encode_utf16().collect();

            let text_w = format.measure(&font, &wide)?;
            let height = (PILL_H * scale).round() as i32;
            let width = ((PAD_LEFT + CIRCLE + GAP + PAD_RIGHT) * scale + text_w).ceil() as i32;

            // 위에서 아래로 쌓인(top-down) 32bpp DIB
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let dc = CreateCompatibleDC(None);
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                .ok()
                .filter(|_| !bits.is_null());
            let Some(bitmap) = bitmap else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, bitmap.into());
            let image = Image {
                dc,
                bitmap,
                old,
                width,
                height,
            };

            let mut gp_bitmap = std::ptr::null_mut();
            if gp::GdipCreateBitmapFromScan0(
                width,
                height,
                width * 4,
                PARGB,
                Some(bits as *const u8),
                &mut gp_bitmap,
            ) != gp::Ok
            {
                return None;
            }
            let mut g = std::ptr::null_mut();
            if gp::GdipGetImageGraphicsContext(gp_bitmap as *mut gp::GpImage, &mut g) == gp::Ok {
                draw(g, &image, scale, &font, &icon, &format, &wide);
                gp::GdipDeleteGraphics(g);
            }
            gp::GdipDisposeImage(gp_bitmap as *mut gp::GpImage);
            Some(image)
        }
    }

    unsafe fn upload(&self, hwnd: HWND, pos: POINT) {
        unsafe {
            let size = SIZE {
                cx: self.width,
                cy: self.height,
            };
            let src = POINT::default();
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                hwnd,
                None,
                Some(&pos),
                Some(&size),
                Some(self.dc),
                Some(&src),
                windows::Win32::Foundation::COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }
}

unsafe fn draw(
    g: *mut gp::GpGraphics,
    image: &Image,
    scale: f32,
    font: &Font,
    icon: &Font,
    format: &StringFormat,
    text: &[u16],
) {
    unsafe {
        gp::GdipGraphicsClear(g, 0);
        gp::GdipSetSmoothingMode(g, gp::SmoothingModeAntiAlias);
        gp::GdipSetPixelOffsetMode(g, gp::PixelOffsetModeHalf);
        // 투명 바탕 비트맵에는 ClearType이 맞지 않는다 (색 번짐)
        gp::GdipSetTextRenderingHint(g, gp::TextRenderingHintAntiAlias);

        let (w, h) = (image.width as f32, image.height as f32);
        // 알약: 차콜 바탕 + 흰색 10% 테두리 1px (테두리는 안쪽에)
        let border = scale.max(1.0);
        let inset = border / 2.0;
        if let Some(path) = Path::pill(inset, inset, w - border, h - border) {
            if let Some(brush) = Brush::solid(CHARCOAL) {
                gp::GdipFillPath(g, brush.0, path.0);
            }
            let mut pen = std::ptr::null_mut();
            if gp::GdipCreatePen1(BORDER, border, gp::UnitPixel, &mut pen) == gp::Ok {
                gp::GdipDrawPath(g, pen, path.0);
                gp::GdipDeletePen(pen);
            }
        }
        // 빨간 원 + 아이콘
        let d = CIRCLE * scale;
        let (cx, cy) = (PAD_LEFT * scale, (h - d) / 2.0);
        if let Some(red) = Brush::solid(RED) {
            gp::GdipFillEllipse(g, red.0, cx, cy, d, d);
        }
        // 반 픽셀 오프셋은 도형만: 글자에 쓰면 획이 두 픽셀에 걸쳐 흐리고 가늘어진다
        gp::GdipSetPixelOffsetMode(g, gp::PixelOffsetModeNone);
        let Some(white) = Brush::solid(WHITE) else {
            return;
        };
        let glyph: Vec<u16> = MIC_OFF_GLYPH.encode_utf16().collect();
        let icon_rect = gp::RectF {
            X: cx,
            Y: cy,
            Width: d,
            Height: d,
        };
        let centered = format.centered();
        gp::GdipDrawString(
            g,
            PCWSTR(glyph.as_ptr()),
            glyph.len() as i32,
            icon.0,
            &icon_rect,
            centered.as_ref().map_or(format.0, |f| f.0),
            white.0,
        );
        // 문구: 원 오른쪽, 세로 가운데
        let text_x = (PAD_LEFT + CIRCLE + GAP) * scale;
        let text_rect = gp::RectF {
            X: text_x,
            Y: 0.0,
            Width: w - text_x,
            Height: h,
        };
        let line = format.line_centered();
        let line = line.as_ref().map_or(format.0, |f| f.0);
        // 글자를 윤곽선 경로로 바꿔 채우고 얇게 한 번 더 그린다. GDI+ 글자는 예전 배지
        // (브라우저, DirectWrite)보다 눈에 띄게 가늘다.
        let (mut family, mut style, mut size) = (std::ptr::null_mut(), 0i32, 0f32);
        let outlined = gp::GdipGetFamily(font.0, &mut family) == gp::Ok
            && gp::GdipGetFontStyle(font.0, &mut style) == gp::Ok
            && gp::GdipGetFontSize(font.0, &mut size) == gp::Ok
            && Path::new().is_some_and(|path| {
                let added = gp::GdipAddPathString(
                    path.0,
                    PCWSTR(text.as_ptr()),
                    text.len() as i32,
                    family,
                    style,
                    size,
                    &text_rect,
                    line,
                ) == gp::Ok;
                if added {
                    gp::GdipFillPath(g, white.0, path.0);
                    let mut pen = std::ptr::null_mut();
                    if gp::GdipCreatePen1(TEXT_STROKE, scale, gp::UnitPixel, &mut pen) == gp::Ok {
                        gp::GdipSetPenLineJoin(pen, gp::LineJoinRound);
                        gp::GdipDrawPath(g, pen, path.0);
                        gp::GdipDeletePen(pen);
                    }
                }
                added
            });
        if !family.is_null() {
            gp::GdipDeleteFontFamily(family);
        }
        if !outlined {
            gp::GdipDrawString(
                g,
                PCWSTR(text.as_ptr()),
                text.len() as i32,
                font.0,
                &text_rect,
                line,
                white.0,
            );
        }
    }
}

// GDI+ 객체를 쓰고 나면 지우는 얇은 감싸개들

struct Font(*mut gp::GpFont);

impl Font {
    unsafe fn new(family: PCWSTR, px: f32, style: i32) -> Option<Font> {
        unsafe {
            let mut fam = std::ptr::null_mut();
            if gp::GdipCreateFontFamilyFromName(family, std::ptr::null_mut(), &mut fam) != gp::Ok {
                return None;
            }
            let mut font = std::ptr::null_mut();
            let ok = gp::GdipCreateFont(fam, px, style, gp::UnitPixel, &mut font) == gp::Ok;
            gp::GdipDeleteFontFamily(fam);
            ok.then_some(Font(font))
        }
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            gp::GdipDeleteFont(self.0);
        }
    }
}

struct StringFormat(*mut gp::GpStringFormat);

impl StringFormat {
    /// 글자 앞뒤 여백을 붙이지 않는 서식 (크기를 정확히 재고 맞추려고)
    unsafe fn typographic() -> Option<StringFormat> {
        unsafe {
            let mut generic = std::ptr::null_mut();
            if gp::GdipStringFormatGetGenericTypographic(&mut generic) != gp::Ok {
                return None;
            }
            let mut f = std::ptr::null_mut();
            (gp::GdipCloneStringFormat(generic, &mut f) == gp::Ok).then_some(StringFormat(f))
        }
    }

    unsafe fn line_centered(&self) -> Option<StringFormat> {
        unsafe {
            let f = self.clone_format()?;
            gp::GdipSetStringFormatLineAlign(f.0, gp::StringAlignmentCenter);
            Some(f)
        }
    }

    unsafe fn centered(&self) -> Option<StringFormat> {
        unsafe {
            let f = self.line_centered()?;
            gp::GdipSetStringFormatAlign(f.0, gp::StringAlignmentCenter);
            Some(f)
        }
    }

    unsafe fn clone_format(&self) -> Option<StringFormat> {
        unsafe {
            let mut f = std::ptr::null_mut();
            (gp::GdipCloneStringFormat(self.0, &mut f) == gp::Ok).then_some(StringFormat(f))
        }
    }

    /// 한 줄로 그릴 때의 너비 (픽셀)
    unsafe fn measure(&self, font: &Font, text: &[u16]) -> Option<f32> {
        unsafe {
            // 재기만 하는 1×1 비트맵
            let mut bmp = std::ptr::null_mut();
            if gp::GdipCreateBitmapFromScan0(1, 1, 4, PARGB, None, &mut bmp) != gp::Ok {
                return None;
            }
            let mut g = std::ptr::null_mut();
            let mut width = None;
            if gp::GdipGetImageGraphicsContext(bmp as *mut gp::GpImage, &mut g) == gp::Ok {
                gp::GdipSetTextRenderingHint(g, gp::TextRenderingHintAntiAlias);
                let layout = gp::RectF {
                    X: 0.0,
                    Y: 0.0,
                    Width: 10_000.0,
                    Height: 10_000.0,
                };
                let mut bounds = gp::RectF::default();
                if gp::GdipMeasureString(
                    g,
                    PCWSTR(text.as_ptr()),
                    text.len() as i32,
                    font.0,
                    &layout,
                    self.0,
                    &mut bounds,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                ) == gp::Ok
                {
                    width = Some(bounds.Width);
                }
                gp::GdipDeleteGraphics(g);
            }
            gp::GdipDisposeImage(bmp as *mut gp::GpImage);
            width
        }
    }
}

impl Drop for StringFormat {
    fn drop(&mut self) {
        unsafe {
            gp::GdipDeleteStringFormat(self.0);
        }
    }
}

struct Brush(*mut gp::GpBrush);

impl Brush {
    unsafe fn solid(argb: u32) -> Option<Brush> {
        unsafe {
            let mut b = std::ptr::null_mut();
            (gp::GdipCreateSolidFill(argb, &mut b) == gp::Ok)
                .then_some(Brush(b as *mut gp::GpBrush))
        }
    }
}

impl Drop for Brush {
    fn drop(&mut self) {
        unsafe {
            gp::GdipDeleteBrush(self.0);
        }
    }
}

struct Path(*mut gp::GpPath);

impl Path {
    unsafe fn new() -> Option<Path> {
        unsafe {
            let mut p = std::ptr::null_mut();
            (gp::GdipCreatePath(gp::FillModeAlternate, &mut p) == gp::Ok).then_some(Path(p))
        }
    }

    /// 양 끝이 반원인 알약 모양
    unsafe fn pill(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
        unsafe {
            let mut p = std::ptr::null_mut();
            if gp::GdipCreatePath(gp::FillModeAlternate, &mut p) != gp::Ok {
                return None;
            }
            gp::GdipAddPathArc(p, x, y, h, h, 90.0, 180.0);
            gp::GdipAddPathArc(p, x + w - h, y, h, h, 270.0, 180.0);
            gp::GdipClosePathFigure(p);
            Some(Path(p))
        }
    }
}

impl Drop for Path {
    fn drop(&mut self) {
        unsafe {
            gp::GdipDeletePath(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 그린 배지의 BGRA 픽셀 (미리 곱한 알파)
    fn render(lang: Lang, scale: f32) -> (i32, i32, Vec<u8>) {
        unsafe {
            let mut token = 0usize;
            let input = gp::GdiplusStartupInput {
                GdiplusVersion: 1,
                ..Default::default()
            };
            assert_eq!(
                gp::GdiplusStartup(&mut token, &input, std::ptr::null_mut()),
                gp::Ok
            );
            let image = Image::render(lang, scale).expect("render");
            let mut obj = windows::Win32::Graphics::Gdi::DIBSECTION::default();
            windows::Win32::Graphics::Gdi::GetObjectW(
                image.bitmap.into(),
                std::mem::size_of_val(&obj) as i32,
                Some(&mut obj as *mut _ as *mut _),
            );
            let len = (image.width * image.height * 4) as usize;
            let bytes = std::slice::from_raw_parts(obj.dsBm.bmBits as *const u8, len).to_vec();
            (image.width, image.height, bytes)
        }
    }

    fn pixel(img: &(i32, i32, Vec<u8>), x: i32, y: i32) -> [u8; 4] {
        let i = ((y * img.0 + x) * 4) as usize;
        img.2[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn badge_matches_webview_layout() {
        for (lang, scale) in [
            (Lang::Ko, 1.0),
            (Lang::En, 1.0),
            (Lang::Ko, 1.5),
            (Lang::En, 1.25),
        ] {
            let img = render(lang, scale);
            // 높이 42 (논리 픽셀), 너비는 문구에 맞춤
            assert_eq!(img.1, (42.0 * scale).round() as i32, "{lang:?} {scale}");
            assert!(img.0 > img.1 * 2, "{lang:?} {scale}: {}", img.0);
            // 모서리는 투명, 가운데 줄 원 안쪽은 빨강(BGRA), 오른쪽 여백은 차콜
            assert_eq!(pixel(&img, 0, 0)[3], 0);
            let cy = img.1 / 2;
            assert_eq!(
                pixel(&img, (12.0 * scale) as i32, cy),
                [0x0B, 0x00, 0xE7, 0xFF]
            );
            assert_eq!(
                pixel(&img, img.0 - (10.0 * scale) as i32, cy),
                [0x1C, 0x1E, 0x1F, 0xFF]
            );
            // NPDK_BADGE_DUMP=폴더 면 눈으로 비교할 BMP를 남긴다
            if let Ok(dir) = std::env::var("NPDK_BADGE_DUMP") {
                dump_bmp(&format!("{dir}/badge-{lang:?}-{scale}.bmp"), &img);
            }
        }
    }

    fn dump_bmp(path: &str, img: &(i32, i32, Vec<u8>)) {
        let (w, h, px) = img;
        let mut out = Vec::new();
        let size = 54 + px.len() as u32;
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&(-h).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&[0; 24]);
        out.extend_from_slice(px);
        std::fs::write(path, out).unwrap();
    }
}
