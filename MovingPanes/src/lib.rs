use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
// The OpenRGB MovingPanes reference uses 3.14f here; keep that rounded value for parity.
#[allow(clippy::approx_constant)]
const PI4: f32 = 3.14 * 0.25;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
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

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    divisions: usize,
    reverse: bool,
    color1: SkydimoRgb,
    color2: SkydimoRgb,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            divisions: 4,
            reverse: false,
            color1: SkydimoRgb { r: 255, g: 0, b: 0 },
            color2: SkydimoRgb { r: 0, g: 0, b: 255 },
        }
    }
}

struct MovingPanesEffect {
    config: Config,
    width: usize,
    height: usize,
    time_acc: f32,
    last_elapsed: Option<f32>,
}

impl MovingPanesEffect {
    fn new() -> Self {
        Self {
            config: Config::default(),
            width: 0,
            height: 1,
            time_acc: 0.0,
            last_elapsed: None,
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = round_param(speed).clamp(1.0, 100.0);
        }
        if let Some(divisions) = json_number(json, "divisions") {
            self.config.divisions = round_param(divisions).clamp(2.0, 50.0) as usize;
        }
        if let Some(reverse) = json_bool(json, "reverse") {
            self.config.reverse = reverse;
        }
        if let Some(colors) = json_color_pair(json, "colors") {
            if let (Some(first), Some(second)) = (colors[0], colors[1]) {
                self.config.color1 = first;
                self.config.color2 = second;
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let t_signed = if self.config.reverse {
            self.time_acc
        } else {
            -self.time_acc
        };

        let width = if self.width == 0 { pixels.len() } else { self.width }.max(1);
        let height = self.height.max(1);
        if height <= 1 {
            self.render_linear(pixels, width, t_signed);
        } else {
            self.render_matrix(pixels, width, height, t_signed);
        }

        let dt = self.delta_seconds(elapsed_seconds);
        self.time_acc += 0.1 * self.config.speed * dt;
    }

    fn render_linear(&self, pixels: &mut [SkydimoRgb], width: usize, t_signed: f32) {
        let step = 4.0 / width as f32;
        let (sin_step, cos_step) = step.sin_cos();
        let mut active_zone = usize::MAX;
        let mut active_zone_id = 0usize;
        let mut sin_phase = 0.0f32;
        let mut cos_phase = 1.0f32;

        for (x, pixel) in pixels.iter_mut().enumerate() {
            let zone = zone_for_x(x, width, self.config.divisions);
            if zone != active_zone {
                active_zone = zone;
                active_zone_id = zone & 1;
                let direction_time = if active_zone_id == 0 {
                    -t_signed
                } else {
                    t_signed
                };
                (sin_phase, cos_phase) = (x as f32 * step + direction_time + PI4).sin_cos();
            }

            let blend = 0.5 * (1.0 + sin_phase);
            *pixel = if active_zone_id == 0 {
                lerp_floor(self.config.color1, self.config.color2, blend)
            } else {
                lerp_floor(self.config.color2, self.config.color1, blend)
            };

            let next_sin = sin_phase * cos_step + cos_phase * sin_step;
            let next_cos = cos_phase * cos_step - sin_phase * sin_step;
            sin_phase = next_sin;
            cos_phase = next_cos;
        }
    }

    fn render_matrix(&self, pixels: &mut [SkydimoRgb], width: usize, height: usize, t_signed: f32) {
        let total = pixels.len().min(width.saturating_mul(height));
        let row_step = 4.0 / height as f32;
        let mut index = 0usize;

        for row in 0..height {
            let row_base = row as f32 * row_step + PI4;
            let even_blend = 0.5 * (1.0 + (row_base - t_signed).sin());
            let odd_blend = 0.5 * (1.0 + (row_base + t_signed).sin());
            let even_color = lerp_floor(self.config.color1, self.config.color2, even_blend);
            let odd_color = lerp_floor(self.config.color2, self.config.color1, odd_blend);

            for col in 0..width {
                if index >= total {
                    if total < pixels.len() {
                        pixels[total..].fill(SkydimoRgb::default());
                    }
                    return;
                }

                let zone_id = zone_for_x(col, width, self.config.divisions) & 1;
                pixels[index] = if zone_id == 0 { even_color } else { odd_color };
                index += 1;
            }
        }

        if index < pixels.len() {
            pixels[index..].fill(SkydimoRgb::default());
        }
    }

    fn delta_seconds(&mut self, elapsed_seconds: f64) -> f32 {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return 0.0;
        }

        let elapsed = elapsed_seconds.min(f32::MAX as f64) as f32;
        let delta = match self.last_elapsed {
            Some(last) if elapsed >= last => elapsed - last,
            _ => 0.0,
        };
        self.last_elapsed = Some(elapsed);
        delta
    }
}

unsafe extern "C" fn moving_panes_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(MovingPanesEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn moving_panes_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<MovingPanesEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn moving_panes_resize(
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

unsafe extern "C" fn moving_panes_update_params_json(
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

unsafe extern "C" fn moving_panes_tick(
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

unsafe extern "C" fn moving_panes_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the ABI declared in manifest.json.
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
                    create: Some(moving_panes_create),
                    destroy: Some(moving_panes_destroy),
                    resize: Some(moving_panes_resize),
                    update_params_json: Some(moving_panes_update_params_json),
                    tick: Some(moving_panes_tick),
                    is_ready: Some(moving_panes_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut MovingPanesEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<MovingPanesEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline(always)]
fn zone_for_x(x: usize, width: usize, divisions: usize) -> usize {
    x.saturating_mul(divisions) / width.max(1)
}

#[inline(always)]
fn lerp_floor(left: SkydimoRgb, right: SkydimoRgb, t: f32) -> SkydimoRgb {
    let t = t.clamp(0.0, 1.0);
    SkydimoRgb {
        r: lerp_channel_floor(left.r, right.r, t),
        g: lerp_channel_floor(left.g, right.g, t),
        b: lerp_channel_floor(left.b, right.b, t),
    }
}

#[inline(always)]
fn lerp_channel_floor(left: u8, right: u8, t: f32) -> u8 {
    let value = left as f32 + t * (right as f32 - left as f32);
    value.clamp(0.0, 255.0) as u8
}

#[inline(always)]
fn round_param(value: f32) -> f32 {
    (value + 0.5).floor()
}

fn json_number(json: &str, key: &str) -> Option<f32> {
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
    raw[..end].trim().parse::<f32>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_after_key(json, key)?;
    match raw {
        raw if raw.starts_with("true") || raw.starts_with("\"true\"") || raw.starts_with('1') => {
            Some(true)
        }
        raw if raw.starts_with("false") || raw.starts_with("\"false\"") || raw.starts_with('0') => {
            Some(false)
        }
        _ => None,
    }
}

fn json_color_pair(json: &str, key: &str) -> Option<[Option<SkydimoRgb>; 2]> {
    let raw = json_value_after_key(json, key)?;
    let mut raw = raw.strip_prefix('[')?;
    let mut colors = [None, None];

    for slot in &mut colors {
        raw = raw.trim_start();
        if raw.starts_with(']') {
            break;
        }
        if let Some(rest) = raw.strip_prefix(',') {
            raw = rest.trim_start();
        }
        let rest = raw.strip_prefix('"')?;
        let end = json_string_end(rest)?;
        *slot = parse_hex_color(&rest[..end]);
        raw = &rest[end + 1..];
    }

    colors.iter().any(Option::is_some).then_some(colors)
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon_pos = after_key.find(':')?;
    Some(after_key[colon_pos + 1..].trim_start())
}

fn json_string_end(raw: &str) -> Option<usize> {
    let mut escaped = false;
    for (idx, ch) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(idx),
            _ => {}
        }
    }
    None
}

fn parse_hex_color(raw: &str) -> Option<SkydimoRgb> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let bytes = hex.as_bytes();

    if bytes.len() == 3 {
        return Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        });
    }

    if bytes.len() != 6 {
        return None;
    }

    Some(SkydimoRgb {
        r: parse_hex_byte(bytes[0], bytes[1])?,
        g: parse_hex_byte(bytes[2], bytes[3])?,
        b: parse_hex_byte(bytes[4], bytes[5])?,
    })
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

#[cfg(test)]
mod tests {
    use super::{json_color_pair, json_number, parse_hex_color, MovingPanesEffect, SkydimoRgb};

    #[test]
    fn parses_params_without_json_dependency() {
        assert_eq!(json_number(r#"{"speed":75,"divisions":"9"}"#, "speed"), Some(75.0));
        assert_eq!(json_number(r#"{"speed":75,"divisions":"9"}"#, "divisions"), Some(9.0));

        let colors = json_color_pair(r##"{"colors":["#0af","#102030"]}"##, "colors").unwrap();
        assert_eq!(colors[0], Some(SkydimoRgb { r: 0, g: 170, b: 255 }));
        assert_eq!(colors[1], Some(SkydimoRgb { r: 16, g: 32, b: 48 }));
    }

    #[test]
    fn parses_short_and_full_hex_colors() {
        assert_eq!(
            parse_hex_color("#Aa10ff"),
            Some(SkydimoRgb {
                r: 170,
                g: 16,
                b: 255,
            })
        );
        assert_eq!(parse_hex_color("#xyz"), None);
    }

    #[test]
    fn renders_directly_into_host_buffer() {
        let mut effect = MovingPanesEffect::new();
        effect.resize(8, 1, 8);

        let mut pixels = [SkydimoRgb::default(); 8];
        effect.tick(0.0, &mut pixels);

        assert!(pixels.iter().any(|pixel| pixel.r != 0 || pixel.g != 0 || pixel.b != 0));
        assert_eq!(effect.time_acc, 0.0);

        effect.tick(1.0, &mut pixels);
        assert_eq!(effect.time_acc, 5.0);
    }
}
