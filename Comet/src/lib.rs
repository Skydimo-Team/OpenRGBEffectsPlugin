mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const COLOR_MODE_RAINBOW: u8 = 0;
const COLOR_MODE_CUSTOM: u8 = 1;
const DEFAULT_COLOR: SkydimoRgb = SkydimoRgb {
    r: 0,
    g: 170,
    b: 255,
};
const PROGRESS_PER_SPEED_PER_SECOND: f32 = 0.005;
const MAX_DELTA_SECONDS: f64 = 0.5;

#[derive(Clone, Copy, Debug)]
struct Hsv {
    h: f32,
    s: f32,
    v: f32,
}

#[derive(Clone, Copy)]
struct CometConfig {
    speed: f32,
    comet_size: f32,
    color_mode: u8,
    user_color: SkydimoRgb,
    user_hsv: Hsv,
}

impl Default for CometConfig {
    fn default() -> Self {
        Self {
            speed: 50.0,
            comet_size: 50.0,
            color_mode: COLOR_MODE_RAINBOW,
            user_color: DEFAULT_COLOR,
            user_hsv: rgb_to_hsv(DEFAULT_COLOR),
        }
    }
}

struct CometEffect {
    config: CometConfig,
    time_acc: f32,
    progress: f32,
    width: usize,
    height: usize,
    last_elapsed_seconds: Option<f64>,
    row_cache: Vec<SkydimoRgb>,
}

impl Default for CometEffect {
    fn default() -> Self {
        Self {
            config: CometConfig::default(),
            time_acc: 0.0,
            progress: 0.0,
            width: 0,
            height: 1,
            last_elapsed_seconds: None,
            row_cache: Vec::new(),
        }
    }
}

impl CometEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
        self.row_cache.clear();
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(comet_size) = json_number(json, "comet_size") {
            self.config.comet_size = comet_size.clamp(1.0, 100.0);
        }
        if let Some(color_mode) = json_number(json, "color_mode") {
            let rounded = (color_mode + 0.5).floor() as i32;
            if rounded == COLOR_MODE_RAINBOW as i32 || rounded == COLOR_MODE_CUSTOM as i32 {
                self.config.color_mode = rounded as u8;
            }
        }
        if let Some(color) = json_color(json, "color") {
            self.config.user_color = color;
            self.config.user_hsv = rgb_to_hsv(color);
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            self.advance(elapsed_seconds);
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width.max(1)
        };

        if pixels.len() <= width {
            render_row(
                pixels,
                width,
                self.progress,
                self.time_acc,
                self.config,
            );
        } else {
            self.row_cache.resize(width, SkydimoRgb::default());
            render_row(
                &mut self.row_cache,
                width,
                self.progress,
                self.time_acc,
                self.config,
            );
            repeat_row(&self.row_cache, pixels);
        }

        self.advance(elapsed_seconds);
    }

    fn advance(&mut self, elapsed_seconds: f64) {
        if !elapsed_seconds.is_finite() {
            return;
        }

        let delta = self
            .last_elapsed_seconds
            .map(|last| elapsed_seconds - last)
            .unwrap_or(0.0);
        self.last_elapsed_seconds = Some(elapsed_seconds);

        if !(0.0..=MAX_DELTA_SECONDS).contains(&delta) {
            return;
        }

        self.time_acc += delta as f32 * self.config.speed * PROGRESS_PER_SPEED_PER_SECOND;
        if self.time_acc > 360.0 {
            self.time_acc = self.time_acc.rem_euclid(360.0);
        }
        self.progress = self.time_acc.rem_euclid(1.0);
    }
}

fn render_row(
    pixels: &mut [SkydimoRgb],
    width: usize,
    progress: f32,
    time_acc: f32,
    config: CometConfig,
) {
    let render_count = width.min(pixels.len());
    let width_f = width.max(1) as f32;
    let tail_len = (0.01 * config.comet_size * width_f).max(0.0001);
    let tail_len_for_hue = tail_len.max(1.0);
    let position = progress * 2.0 * width_f;
    let rainbow_base_hue = 1000.0 * time_acc;

    for (x, pixel) in pixels.iter_mut().take(render_count).enumerate() {
        let x = x as f32;
        *pixel = if x > position {
            SkydimoRgb::default()
        } else {
            comet_color(x, position, tail_len, tail_len_for_hue, rainbow_base_hue, config)
        };
    }

    if render_count < pixels.len() {
        pixels[render_count..].fill(SkydimoRgb::default());
    }
}

#[inline]
fn comet_color(
    x: f32,
    position: f32,
    tail_len: f32,
    tail_len_for_hue: f32,
    rainbow_base_hue: f32,
    config: CometConfig,
) -> SkydimoRgb {
    let distance = position - x;
    let value = if distance > tail_len {
        0.0
    } else if distance == 0.0 {
        1.0
    } else {
        1.0 - distance / tail_len
    };

    if value <= 0.0 {
        return SkydimoRgb::default();
    }

    let saturation_factor = value.powf(0.2);
    let brightness_factor = value * value * value;

    if config.color_mode == COLOR_MODE_CUSTOM {
        hsv_to_rgb(Hsv {
            h: config.user_hsv.h,
            s: saturation_factor * config.user_hsv.s,
            v: brightness_factor * config.user_hsv.v,
        })
    } else {
        hsv_to_rgb(Hsv {
            h: rainbow_base_hue + (distance / tail_len_for_hue) * 360.0,
            s: saturation_factor,
            v: brightness_factor,
        })
    }
}

fn repeat_row(row: &[SkydimoRgb], pixels: &mut [SkydimoRgb]) {
    let width = row.len();
    if width == 0 {
        pixels.fill(SkydimoRgb::default());
        return;
    }

    let mut offset = 0usize;
    while offset < pixels.len() {
        let copy_len = width.min(pixels.len() - offset);
        unsafe {
            std::ptr::copy_nonoverlapping(row.as_ptr(), pixels.as_mut_ptr().add(offset), copy_len);
        }
        offset += copy_len;
    }
}

unsafe extern "C" fn comet_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(CometEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn comet_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<CometEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn comet_resize(
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

unsafe extern "C" fn comet_update_params_json(
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

unsafe extern "C" fn comet_tick(
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
            effect.advance(elapsed_seconds);
            return 0;
        }
        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels);
        0
    })
}

unsafe extern "C" fn comet_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer for one `SkydimoPluginApiV1`.
/// The host must request the ABI version declared by this plugin manifest.
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
                    create: Some(comet_create),
                    destroy: Some(comet_destroy),
                    resize: Some(comet_resize),
                    update_params_json: Some(comet_update_params_json),
                    tick: Some(comet_tick),
                    is_ready: Some(comet_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut CometEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<CometEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn rgb_to_hsv(rgb: SkydimoRgb) -> Hsv {
    let r = rgb.r as f32 / 255.0;
    let g = rgb.g as f32 / 255.0;
    let b = rgb.b as f32 / 255.0;
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let delta = maxc - minc;

    let saturation = if maxc > 0.0 { delta / maxc } else { 0.0 };
    let hue = if delta <= 0.0 {
        0.0
    } else if maxc == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if maxc == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    Hsv {
        h: hue.rem_euclid(360.0),
        s: saturation,
        v: maxc,
    }
}

fn hsv_to_rgb(hsv: Hsv) -> SkydimoRgb {
    let h = hsv.h.rem_euclid(360.0);
    let s = hsv.s.clamp(0.0, 1.0);
    let v = hsv.v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    SkydimoRgb {
        r: to_u8((r + m) * 255.0),
        g: to_u8((g + m) * 255.0),
        b: to_u8((b + m) * 255.0),
    }
}

#[inline]
fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
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

fn json_color(json: &str, key: &str) -> Option<SkydimoRgb> {
    let raw = json_value_after_key(json, key)?;
    let raw = raw.strip_prefix('"')?;
    let end = json_string_end(raw)?;
    parse_hex_color(&raw[..end])
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

#[cfg(test)]
mod tests {
    use super::{
        json_color, json_number, parse_hex_color, skydimo_plugin_get_api, CometEffect,
        SkydimoPluginApiV1, SkydimoRgb, COLOR_MODE_CUSTOM, DEFAULT_COLOR,
        SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_params_without_allocating_runtime_dependencies() {
        let json = r##"{"speed":75,"comet_size":25,"color_mode":1,"color":"#0af"}"##;
        assert_eq!(json_number(json, "speed"), Some(75.0));
        assert_eq!(json_number(json, "comet_size"), Some(25.0));
        assert_eq!(json_color(json, "color"), Some(DEFAULT_COLOR));
    }

    #[test]
    fn parses_short_and_long_hex_colors() {
        assert_eq!(
            parse_hex_color("#f06"),
            Some(SkydimoRgb {
                r: 255,
                g: 0,
                b: 102,
            })
        );
        assert_eq!(
            parse_hex_color("336699"),
            Some(SkydimoRgb {
                r: 51,
                g: 102,
                b: 153,
            })
        );
        assert_eq!(parse_hex_color("#xyz"), None);
    }

    #[test]
    fn renders_custom_color_at_comet_head() {
        let mut effect = CometEffect::default();
        effect.update_params(r##"{"color_mode":1,"color":"#00AAFF"}"##);
        effect.resize(5, 1, 5);

        let mut pixels = [SkydimoRgb::default(); 5];
        effect.tick(0.0, &mut pixels);

        assert_eq!(effect.config.color_mode, COLOR_MODE_CUSTOM);
        assert_eq!(pixels[0], DEFAULT_COLOR);
        assert!(pixels[1..].iter().all(|pixel| *pixel == SkydimoRgb::default()));
    }

    #[test]
    fn repeats_cached_row_across_matrix() {
        let mut effect = CometEffect::default();
        effect.update_params(r##"{"color_mode":1,"color":"#00AAFF"}"##);
        effect.resize(4, 2, 8);

        let mut pixels = [SkydimoRgb::default(); 8];
        effect.tick(0.0, &mut pixels);

        assert_eq!(pixels[..4], pixels[4..]);
    }

    #[test]
    fn advances_using_elapsed_delta() {
        let mut effect = CometEffect::default();
        let mut pixels = [SkydimoRgb::default(); 1];

        effect.tick(0.0, &mut pixels);
        assert_eq!(effect.progress, 0.0);

        effect.tick(0.5, &mut pixels);
        assert!((effect.progress - 0.125).abs() < 0.0001);
    }

    #[test]
    fn exposes_native_effect_abi_v3() {
        let mut api = SkydimoPluginApiV1::default();

        let status = unsafe {
            skydimo_plugin_get_api(
                SKYDIMO_NATIVE_C_ABI_VERSION,
                std::ptr::null(),
                &mut api,
            )
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(api.kind_mask & SKYDIMO_PLUGIN_KIND_EFFECT, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }
}
