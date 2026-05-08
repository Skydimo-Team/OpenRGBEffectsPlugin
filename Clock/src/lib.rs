use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
pub struct SkydimoHostApiV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoHardwareCandidateV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoDeviceInfoV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputDefinitionV1 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct SkydimoOutputFrameV1 {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoEffectApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub resize: Option<unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32>,
    pub update_params_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub tick: Option<unsafe extern "C" fn(*mut c_void, f64, *mut SkydimoRgb, usize) -> i32>,
    pub is_ready: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoControllerApiV1 {
    pub size: u32,
    pub create: Option<
        unsafe extern "C" fn(
            *const SkydimoHostApiV1,
            *const SkydimoHardwareCandidateV1,
            *mut *mut c_void,
        ) -> i32,
    >,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub validate: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub init: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub get_device_info: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoDeviceInfoV1) -> i32>,
    pub get_output_count: Option<unsafe extern "C" fn(*mut c_void) -> usize>,
    pub get_output:
        Option<unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoOutputDefinitionV1) -> i32>,
    pub update: Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputFrameV1, usize) -> i32>,
    pub set_output_leds_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, usize) -> i32>,
    pub update_output:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputDefinitionV1) -> i32>,
    pub disconnect: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoExtensionApiV1 {
    pub size: u32,
    pub create: Option<unsafe extern "C" fn(*const SkydimoHostApiV1, *mut *mut c_void) -> i32>,
    pub destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    pub start: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub stop: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_scan_devices: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    pub on_event_json:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *const c_char, usize) -> i32>,
    pub on_page_message_json: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> i32>,
    pub on_device_frame: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const SkydimoOutputFrameV1,
            usize,
        ) -> i32,
    >,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoPluginApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub kind_mask: u32,
    pub effect: SkydimoEffectApiV1,
    pub controller: SkydimoControllerApiV1,
    pub extension: SkydimoExtensionApiV1,
    pub shutdown_plugin: Option<unsafe extern "C" fn()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClockMode {
    TwelveHour,
    TwentyFourHour,
}

impl ClockMode {
    #[inline]
    fn hour_divisor(self) -> u32 {
        match self {
            Self::TwelveHour => 12,
            Self::TwentyFourHour => 24,
        }
    }
}

#[derive(Clone, Copy)]
struct ClockConfig {
    mode: ClockMode,
    hour_color: SkydimoRgb,
    minute_color: SkydimoRgb,
    second_color: SkydimoRgb,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            mode: ClockMode::TwelveHour,
            hour_color: SkydimoRgb { r: 255, g: 0, b: 0 },
            minute_color: SkydimoRgb { r: 0, g: 255, b: 0 },
            second_color: SkydimoRgb { r: 0, g: 0, b: 255 },
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ClockTime {
    hour: u32,
    minute: u32,
    second: u32,
    subsecond: f64,
}

struct ClockEffect {
    config: ClockConfig,
    width: usize,
    height: usize,
}

impl ClockEffect {
    fn new() -> Self {
        Self {
            config: ClockConfig::default(),
            width: 0,
            height: 1,
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, bytes: &[u8]) {
        if let Some(mode) = json_number(bytes, b"clockMode") {
            match round_select_value(mode) {
                Some(0) => self.config.mode = ClockMode::TwelveHour,
                Some(1) => self.config.mode = ClockMode::TwentyFourHour,
                _ => {}
            }
        }

        if let Some(color) = json_string(bytes, b"hourColor").and_then(parse_hex_color) {
            self.config.hour_color = color;
        }
        if let Some(color) = json_string(bytes, b"minuteColor").and_then(parse_hex_color) {
            self.config.minute_color = color;
        }
        if let Some(color) = json_string(bytes, b"secondColor").and_then(parse_hex_color) {
            self.config.second_color = color;
        }
    }

    fn tick(&self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        self.tick_at(local_clock_time(elapsed_seconds), pixels);
    }

    fn tick_at(&self, time: ClockTime, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(1);
        let height = self.height.max(1);
        let total = pixels.len().min(width.saturating_mul(height));
        if total == 0 {
            return;
        }

        let row_len = width.min(total);
        let (hour_pos, minute_pos, second_pos) = self.hand_positions(width, time);
        let row = &mut pixels[..row_len];
        row.fill(SkydimoRgb::default());
        splat_hand(row, hour_pos, self.config.hour_color);
        splat_hand(row, minute_pos, self.config.minute_color);
        splat_hand(row, second_pos, self.config.second_color);

        let mut filled = row_len;
        while filled < total {
            let copy_len = row_len.min(total - filled);
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr(),
                    pixels.as_mut_ptr().add(filled),
                    copy_len,
                );
            }
            filled += copy_len;
        }

        if total < pixels.len() {
            pixels[total..].fill(SkydimoRgb::default());
        }
    }

    fn hand_positions(&self, width: usize, time: ClockTime) -> (f64, f64, f64) {
        let span = width.saturating_sub(1) as f64;
        let mode = self.config.mode.hour_divisor();
        let hour = time.hour % mode;
        let minute = time.minute.min(59);
        let second = time.second.min(59);
        let subsecond = time.subsecond.clamp(0.0, 0.999_999);

        let s = second as f64 + subsecond;
        let m = minute as f64 + second as f64 / 60.0;
        let h = hour as f64 + minute as f64 / 60.0;

        (
            span * h / mode as f64,
            span * m / 60.0,
            span * s / 60.0,
        )
    }
}

unsafe extern "C" fn clock_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(ClockEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn clock_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<ClockEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn clock_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        effect.resize(width, height, led_count);
        0
    })
}

unsafe extern "C" fn clock_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if ptr.is_null() || len == 0 {
            return 0;
        }

        let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
        effect.update_params(bytes);
        0
    })
}

unsafe extern "C" fn clock_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if buffer.is_null() && len > 0 {
            return -2;
        }
        if len == 0 {
            return 0;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn clock_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer. The host must pass the ABI
/// version declared in the plugin manifest.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    catch_ffi(|| {
        if out_api.is_null() || requested_abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
            return -1;
        }

        unsafe {
            *out_api = SkydimoPluginApiV1 {
                size: std::mem::size_of::<SkydimoPluginApiV1>() as u32,
                abi_version: SKYDIMO_NATIVE_C_ABI_VERSION,
                kind_mask: SKYDIMO_PLUGIN_KIND_EFFECT,
                effect: SkydimoEffectApiV1 {
                    size: std::mem::size_of::<SkydimoEffectApiV1>() as u32,
                    create: Some(clock_create),
                    destroy: Some(clock_destroy),
                    resize: Some(clock_resize),
                    update_params_json: Some(clock_update_params_json),
                    tick: Some(clock_tick),
                    is_ready: Some(clock_is_ready),
                },
                controller: SkydimoControllerApiV1 {
                    size: std::mem::size_of::<SkydimoControllerApiV1>() as u32,
                    ..SkydimoControllerApiV1::default()
                },
                extension: SkydimoExtensionApiV1 {
                    size: std::mem::size_of::<SkydimoExtensionApiV1>() as u32,
                    ..SkydimoExtensionApiV1::default()
                },
                shutdown_plugin: None,
            };
        }
        0
    })
}

fn effect_mut(instance: *mut c_void) -> Option<&'static mut ClockEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<ClockEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn splat_hand(row: &mut [SkydimoRgb], position: f64, color: SkydimoRgb) {
    if row.is_empty() || !position.is_finite() {
        return;
    }

    let max_index = row.len() - 1;
    let start = if position <= 1.0 {
        0
    } else {
        (position - 1.0).ceil().min(max_index as f64) as usize
    };
    let end = (position + 1.0).floor().clamp(0.0, max_index as f64) as usize;
    if start > end {
        return;
    }

    for (x, pixel) in row.iter_mut().enumerate().take(end + 1).skip(start) {
        let brightness = 1.0 - (x as f64 - position).abs();
        if brightness <= 0.0 {
            continue;
        }

        let candidate = SkydimoRgb {
            r: scale_channel(color.r, brightness),
            g: scale_channel(color.g, brightness),
            b: scale_channel(color.b, brightness),
        };
        pixel.r = pixel.r.max(candidate.r);
        pixel.g = pixel.g.max(candidate.g);
        pixel.b = pixel.b.max(candidate.b);
    }
}

#[inline]
fn scale_channel(value: u8, brightness: f64) -> u8 {
    (value as f64 * brightness + 0.5)
        .floor()
        .clamp(0.0, 255.0) as u8
}

#[cfg(windows)]
fn local_clock_time(_fallback_elapsed: f64) -> ClockTime {
    let mut raw = WindowsSystemTime::default();
    unsafe {
        GetLocalTime(&mut raw);
    }

    ClockTime {
        hour: raw.hour as u32,
        minute: raw.minute as u32,
        second: raw.second as u32,
        subsecond: raw.milliseconds as f64 / 1000.0,
    }
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct WindowsSystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocalTime(system_time: *mut WindowsSystemTime);
}

#[cfg(unix)]
fn local_clock_time(_fallback_elapsed: f64) -> ClockTime {
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return fallback_clock_time(_fallback_elapsed);
    };
    let seconds = now.as_secs() as TimeT;
    let mut local = Tm::default();
    let converted = unsafe { localtime_r(&seconds, &mut local) };
    if converted.is_null() {
        return fallback_clock_time(_fallback_elapsed);
    }

    ClockTime {
        hour: local.tm_hour.max(0) as u32,
        minute: local.tm_min.max(0) as u32,
        second: local.tm_sec.max(0) as u32,
        subsecond: now.subsec_nanos() as f64 / 1_000_000_000.0,
    }
}

#[cfg(unix)]
#[cfg(target_pointer_width = "64")]
type TimeT = i64;

#[cfg(unix)]
#[cfg(target_pointer_width = "32")]
type TimeT = i32;

#[cfg(unix)]
#[repr(C)]
struct Tm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: std::os::raw::c_long,
    tm_zone: *const c_char,
}

#[cfg(unix)]
impl Default for Tm {
    fn default() -> Self {
        Self {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 0,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 0,
            tm_yday: 0,
            tm_isdst: 0,
            tm_gmtoff: 0,
            tm_zone: std::ptr::null(),
        }
    }
}

#[cfg(unix)]
unsafe extern "C" {
    fn localtime_r(timep: *const TimeT, result: *mut Tm) -> *mut Tm;
}

#[cfg(not(windows))]
fn fallback_clock_time(fallback_elapsed: f64) -> ClockTime {
    let seconds_of_day = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() % 86_400)
        .unwrap_or(0);

    ClockTime {
        hour: (seconds_of_day / 3600) as u32,
        minute: ((seconds_of_day / 60) % 60) as u32,
        second: (seconds_of_day % 60) as u32,
        subsecond: fallback_elapsed.fract().abs(),
    }
}

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f64> {
    let start = json_value_start(bytes, key)?;
    let mut end = start;
    while end < bytes.len()
        && matches!(
            bytes[end],
            b'0'..=b'9' | b'+' | b'-' | b'.' | b'e' | b'E'
        )
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..end]).ok()?.parse().ok()
}

fn json_string<'a>(bytes: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut pos = json_value_start(bytes, key)?;
    if bytes.get(pos).copied()? != b'"' {
        return None;
    }
    pos += 1;
    let start = pos;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some(&bytes[start..pos]);
        }
        pos += 1;
    }
    None
}

fn json_value_start(bytes: &[u8], key: &[u8]) -> Option<usize> {
    let key_pos = find_json_key(bytes, key)?;
    let mut pos = key_pos + key.len() + 2;
    pos = skip_ascii_ws(bytes, pos);
    if bytes.get(pos).copied()? != b':' {
        return None;
    }
    Some(skip_ascii_ws(bytes, pos + 1))
}

fn find_json_key(bytes: &[u8], key: &[u8]) -> Option<usize> {
    if key.is_empty() || bytes.len() < key.len() + 2 {
        return None;
    }

    let last = bytes.len() - key.len() - 1;
    let mut pos = 0usize;
    while pos < last {
        if bytes[pos] == b'"'
            && bytes[pos + 1..].starts_with(key)
            && bytes.get(pos + key.len() + 1).copied() == Some(b'"')
        {
            return Some(pos);
        }
        pos += 1;
    }
    None
}

fn skip_ascii_ws(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn round_select_value(value: f64) -> Option<i32> {
    if !value.is_finite() {
        return None;
    }
    let rounded = (value + 0.5).floor();
    if rounded < i32::MIN as f64 || rounded > i32::MAX as f64 {
        None
    } else {
        Some(rounded as i32)
    }
}

fn parse_hex_color(raw: &[u8]) -> Option<SkydimoRgb> {
    let mut bytes = trim_ascii(raw);
    if let Some(stripped) = bytes.strip_prefix(b"#") {
        bytes = stripped;
    }

    match bytes.len() {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(bytes[0], bytes[1])?,
            g: parse_hex_byte(bytes[2], bytes[3])?,
            b: parse_hex_byte(bytes[4], bytes[5])?,
        }),
        _ => None,
    }
}

fn parse_hex_byte(hi: u8, lo: u8) -> Option<u8> {
    Some((parse_hex_nibble(hi)? << 4) | parse_hex_nibble(lo)?)
}

fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while let Some((first, rest)) = bytes.split_first() {
        if !first.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    while let Some((last, rest)) = bytes.split_last() {
        if !last.is_ascii_whitespace() {
            break;
        }
        bytes = rest;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::{
        parse_hex_color, skydimo_plugin_get_api, ClockEffect, ClockMode, ClockTime,
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_clock_params_without_json_allocations() {
        let mut effect = ClockEffect::new();
        effect.update_params(
            br##"{
                "clockMode": 1,
                "hourColor": "#0af",
                "minuteColor": "#123456",
                "secondColor": "#FEDCBA"
            }"##,
        );

        assert_eq!(effect.config.mode, ClockMode::TwentyFourHour);
        assert_eq!(effect.config.hour_color, SkydimoRgb { r: 0, g: 170, b: 255 });
        assert_eq!(
            effect.config.minute_color,
            SkydimoRgb {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }
        );
        assert_eq!(
            effect.config.second_color,
            SkydimoRgb {
                r: 0xfe,
                g: 0xdc,
                b: 0xba,
            }
        );
    }

    #[test]
    fn renders_midnight_as_lightened_overlap_on_each_row() {
        let mut effect = ClockEffect::new();
        effect.resize(5, 2, 10);
        let mut pixels = [SkydimoRgb { r: 9, g: 9, b: 9 }; 10];

        effect.tick_at(
            ClockTime {
                hour: 0,
                minute: 0,
                second: 0,
                subsecond: 0.0,
            },
            &mut pixels,
        );

        let expected_row = [
            SkydimoRgb {
                r: 255,
                g: 255,
                b: 255,
            },
            SkydimoRgb::default(),
            SkydimoRgb::default(),
            SkydimoRgb::default(),
            SkydimoRgb::default(),
        ];
        assert_eq!(&pixels[..5], &expected_row);
        assert_eq!(&pixels[5..], &expected_row);
    }

    #[test]
    fn places_twenty_four_hour_hand_against_full_day() {
        let mut effect = ClockEffect::new();
        effect.update_params(br#"{"clockMode":1}"#);
        effect.resize(25, 1, 25);
        let mut pixels = [SkydimoRgb::default(); 25];

        effect.tick_at(
            ClockTime {
                hour: 12,
                minute: 0,
                second: 0,
                subsecond: 0.0,
            },
            &mut pixels,
        );

        assert_eq!(pixels[12].r, 255);
    }

    #[test]
    fn parses_short_and_full_hex_colors() {
        assert_eq!(
            parse_hex_color(b"#0aF"),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255,
            })
        );
        assert_eq!(
            parse_hex_color(b"  #102030  "),
            Some(SkydimoRgb {
                r: 0x10,
                g: 0x20,
                b: 0x30,
            })
        );
    }

    #[test]
    fn exports_effect_api_for_current_abi() {
        let mut api = SkydimoPluginApiV1::default();
        let status = unsafe {
            skydimo_plugin_get_api(SKYDIMO_NATIVE_C_ABI_VERSION, std::ptr::null(), &mut api)
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(api.kind_mask & SKYDIMO_PLUGIN_KIND_EFFECT, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }
}
