use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const DEFAULT_COLORS: [SkydimoRgb; 5] = [
    SkydimoRgb { r: 255, g: 0, b: 0 },
    SkydimoRgb { r: 0, g: 255, b: 0 },
    SkydimoRgb { r: 0, g: 0, b: 255 },
    SkydimoRgb {
        r: 255,
        g: 255,
        b: 0,
    },
    SkydimoRgb {
        r: 0,
        g: 255,
        b: 255,
    },
];

type HostLogFn = unsafe extern "C" fn(*mut c_void, u32, *const c_char, usize);
type HostCallJsonFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    usize,
    *const c_char,
    usize,
    *mut u8,
    usize,
    *mut usize,
) -> i32;
type EffectAudioCaptureFn =
    unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoAudioFrameV1) -> i32;
type EffectRgbCaptureFn =
    unsafe extern "C" fn(*mut c_void, usize, usize, *mut SkydimoRgbFrameV1) -> i32;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoStr {
    pub ptr: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoFloatSliceV1 {
    pub ptr: *const f32,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoAudioFrameV1 {
    pub amplitude: f32,
    pub bins: SkydimoFloatSliceV1,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoRgbFrameV1 {
    pub width: usize,
    pub height: usize,
    pub pixels: *const SkydimoRgb,
    pub pixels_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoMatrixMapV1 {
    pub width: usize,
    pub height: usize,
    pub map: *const i64,
    pub map_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoLayoutTransformV1 {
    pub flip_horizontal: u8,
    pub flip_vertical: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputCapabilitiesV1 {
    pub editable: u8,
    pub min_total_leds: usize,
    pub max_total_leds: usize,
    pub allowed_total_leds: *const usize,
    pub allowed_total_leds_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputDefinitionV1 {
    pub id: SkydimoStr,
    pub name: SkydimoStr,
    pub output_type: u32,
    pub leds_count: usize,
    pub matrix: *const SkydimoMatrixMapV1,
    pub transform: SkydimoLayoutTransformV1,
    pub capabilities: SkydimoOutputCapabilitiesV1,
    pub default_effect: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputFrameV1 {
    pub output_id: SkydimoStr,
    pub colors: *const SkydimoRgb,
    pub colors_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoLedColorV1 {
    pub index: usize,
    pub color: SkydimoRgb,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoDeviceInfoV1 {
    pub manufacturer: SkydimoStr,
    pub model: SkydimoStr,
    pub serial_id: SkydimoStr,
    pub description: SkydimoStr,
    pub device_type: u32,
    pub image_url: SkydimoStr,
    pub controller_id: SkydimoStr,
    pub controller_name: SkydimoStr,
    pub device_path: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoHardwareCandidateV1 {
    pub candidate_type: u32,
    pub port_key: SkydimoStr,
    pub path: SkydimoStr,
    pub vendor_id: u32,
    pub product_id: u32,
    pub has_vendor_id: u8,
    pub has_product_id: u8,
    pub interface_number: i32,
    pub has_interface_number: u8,
    pub serial_number: SkydimoStr,
    pub manufacturer_string: SkydimoStr,
    pub product_string: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SkydimoHostApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub host_ctx: *mut c_void,
    pub log: Option<HostLogFn>,
    pub call_json: Option<HostCallJsonFn>,
    pub controller_set_device_info:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoDeviceInfoV1) -> i32>,
    pub controller_add_output:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputDefinitionV1) -> i32>,
    pub controller_output_led_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> usize>,
    pub controller_get_rgb_bytes:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut u8, usize) -> isize>,
    pub controller_write: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_read: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u32) -> isize>,
    pub controller_hid_send_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_hid_get_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u8) -> isize>,
    pub extension_lock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
            *mut usize,
            *mut usize,
        ) -> i32,
    >,
    pub extension_unlock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
        ) -> i32,
    >,
    pub extension_set_leds_rgb: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const SkydimoLedColorV1,
            usize,
        ) -> i32,
    >,
    pub effect_audio_capture: Option<EffectAudioCaptureFn>,
    pub effect_screen_capture: Option<EffectRgbCaptureFn>,
    pub effect_album_art: Option<EffectRgbCaptureFn>,
    pub get_plugin_id: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoStr) -> i32>,
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

struct SequenceEffect {
    speed: f64,
    fade_time: f64,
    colors: Vec<SkydimoRgb>,
    progress: f64,
    last_t: Option<f64>,
}

impl Default for SequenceEffect {
    fn default() -> Self {
        Self {
            speed: 10.0,
            fade_time: 1.0,
            colors: DEFAULT_COLORS.to_vec(),
            progress: 0.0,
            last_t: None,
        }
    }
}

impl SequenceEffect {
    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.speed = speed.round().clamp(1.0, 20.0);
        }
        if let Some(fade_time) = json_number(json, "fade_time") {
            self.fade_time = fade_time.round().clamp(1.0, 100.0);
        }
        if let Some(colors) = json_color_array(json, "colors") {
            if !colors.is_empty() {
                self.colors = colors;
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() || self.colors.is_empty() {
            return;
        }

        let delta = if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            let delta = match self.last_t {
                Some(last_t) if elapsed_seconds >= last_t => elapsed_seconds - last_t,
                _ => elapsed_seconds,
            };
            self.last_t = Some(elapsed_seconds);
            delta
        } else {
            0.0
        };

        let count = self.colors.len();
        let current_idx = (self.progress.ceil() as usize) % count;
        let frac = self.progress - self.progress.floor();

        let (color, fade_mult) = if frac >= 0.8 {
            let next_idx = (current_idx + 1) % count;
            let blend = (frac - 0.8) * 5.0;
            (
                lerp_rgb(self.colors[current_idx], self.colors[next_idx], blend),
                1.0 / self.fade_time,
            )
        } else {
            (self.colors[current_idx], 1.0)
        };

        pixels.fill(color);
        self.progress += fade_mult * 0.1 * self.speed * delta;
    }
}

unsafe extern "C" fn sequence_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(SequenceEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn sequence_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<SequenceEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn sequence_resize(
    _instance: *mut c_void,
    _width: u32,
    _height: u32,
    _led_count: u32,
) -> i32 {
    0
}

unsafe extern "C" fn sequence_update_params_json(
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
        let Ok(json) = std::str::from_utf8(bytes) else {
            return -2;
        };
        effect.update_params(json);
        0
    })
}

unsafe extern "C" fn sequence_tick(
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

unsafe extern "C" fn sequence_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the ABI version declared in manifest.json.
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
                    create: Some(sequence_create),
                    destroy: Some(sequence_destroy),
                    resize: Some(sequence_resize),
                    update_params_json: Some(sequence_update_params_json),
                    tick: Some(sequence_tick),
                    is_ready: Some(sequence_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut SequenceEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<SequenceEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn json_number(json: &str, key: &str) -> Option<f64> {
    let mut raw = json_value_after_key(json, key)?;
    if let Some(rest) = raw.strip_prefix('"') {
        raw = rest;
    }
    let end = raw
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(raw.len());
    raw[..end].trim().parse::<f64>().ok()
}

fn json_color_array(json: &str, key: &str) -> Option<Vec<SkydimoRgb>> {
    let raw = json_value_after_key(json, key)?;
    let open = raw.find('[')?;
    let mut rest = &raw[open + 1..];
    let mut colors = Vec::with_capacity(8);

    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            break;
        }
        let Some(after_quote) = rest.strip_prefix('"') else {
            let comma = rest.find(',')?;
            rest = &rest[comma + 1..];
            continue;
        };

        let Some((value, tail)) = next_json_string(after_quote) else {
            break;
        };
        if let Some(color) = hex_to_rgb(value) {
            colors.push(color);
        }
        rest = tail.trim_start();
        if let Some(tail) = rest.strip_prefix(',') {
            rest = tail;
        } else if rest.starts_with(']') {
            break;
        }
    }

    Some(colors)
}

fn next_json_string(raw: &str) -> Option<(&str, &str)> {
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some((&raw[..idx], &raw[idx + 1..])),
            _ => {}
        }
    }
    None
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon_pos = after_key.find(':')?;
    Some(after_key[colon_pos + 1..].trim_start())
}

fn hex_to_rgb(raw: &str) -> Option<SkydimoRgb> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let bytes = hex.as_bytes();
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

fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, t: f64) -> SkydimoRgb {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(left.r as f64 * inv + right.r as f64 * t),
        g: to_u8(left.g as f64 * inv + right.g as f64 * t),
        b: to_u8(left.b as f64 * inv + right.b as f64 * t),
    }
}

fn to_u8(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::{hex_to_rgb, SequenceEffect, SkydimoRgb};

    #[test]
    fn parses_full_and_short_hex_colors() {
        let full = hex_to_rgb("#00B3FF").unwrap();
        assert_eq!((full.r, full.g, full.b), (0, 179, 255));

        let short = hex_to_rgb("0af").unwrap();
        assert_eq!((short.r, short.g, short.b), (0, 170, 255));
    }

    #[test]
    fn updates_params_and_renders_solid_color() {
        let mut effect = SequenceEffect::default();
        effect.update_params(r##"{"speed":20,"fade_time":10,"colors":["#010203","#040506"]}"##);

        let mut pixels = [SkydimoRgb::default(); 4];
        effect.tick(0.0, &mut pixels);

        assert!(pixels.iter().all(|pixel| {
            pixel.r == 1 && pixel.g == 2 && pixel.b == 3
        }));
    }
}
