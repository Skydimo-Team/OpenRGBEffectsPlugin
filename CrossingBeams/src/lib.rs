mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f32 = 50.0;
const DEFAULT_AXIS_SPEED: f32 = 1.0;
const DEFAULT_GLOW: f32 = 1.0;
const DEFAULT_THICKNESS: f32 = 0.0;

#[derive(Clone, Copy, Default)]
struct Hsv {
    h: f32,
    s: f32,
    v: f32,
}

#[derive(Clone, Copy, Default)]
struct HsvBasis {
    r: f32,
    g: f32,
    b: f32,
}

struct CrossingBeamsEffect {
    speed: f32,
    h_speed: f32,
    v_speed: f32,
    glow: f32,
    thickness: f32,
    random_colors: bool,
    progress: f32,
    user_colors: [SkydimoRgb; 2],
    hsv1: Hsv,
    hsv2: Hsv,
    width: usize,
    height: usize,
}

impl Default for CrossingBeamsEffect {
    fn default() -> Self {
        let mut effect = Self {
            speed: DEFAULT_SPEED,
            h_speed: DEFAULT_AXIS_SPEED,
            v_speed: DEFAULT_AXIS_SPEED,
            glow: DEFAULT_GLOW,
            thickness: DEFAULT_THICKNESS,
            random_colors: false,
            progress: 0.0,
            user_colors: [rgb(255, 0, 0), rgb(0, 0, 255)],
            hsv1: Hsv::default(),
            hsv2: Hsv::default(),
            width: 0,
            height: 1,
        };
        effect.apply_user_colors();
        effect
    }
}

impl CrossingBeamsEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(h_speed) = json_number(json, "h_speed") {
            self.h_speed = h_speed.clamp(1.0, 100.0);
        }
        if let Some(v_speed) = json_number(json, "v_speed") {
            self.v_speed = v_speed.clamp(1.0, 100.0);
        }
        if let Some(glow) = json_number(json, "glow") {
            self.glow = glow.clamp(1.0, 100.0);
        }
        if let Some(thickness) = json_number(json, "thickness") {
            self.thickness = thickness.clamp(0.0, 100.0);
        }

        let colors = json_color_array2(json, "colors");
        if let Some(colors) = colors {
            self.user_colors = colors;
        }

        let mut turned_random_off = false;
        if let Some(enabled) = json_bool(json, "random_colors") {
            if enabled != self.random_colors {
                self.random_colors = enabled;
                if enabled {
                    self.hsv1 = Hsv {
                        h: 0.0,
                        s: 1.0,
                        v: 1.0,
                    };
                    self.hsv2 = Hsv {
                        h: 180.0,
                        s: 1.0,
                        v: 1.0,
                    };
                } else {
                    turned_random_off = true;
                }
            }
        }

        if !self.random_colors && (colors.is_some() || turned_random_off) {
            self.apply_user_colors();
        }
    }

    fn tick(&mut self, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            self.advance();
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width.max(1)
        };
        let height = self.height.max(1);
        let width_f = width as f32;
        let height_f = height as f32;
        let inv_width = 1.0 / width_f;
        let inv_height = 1.0 / height_f;

        let sine_x = (0.01 * self.h_speed * self.progress).sin();
        let sine_y = (0.01 * self.v_speed * self.progress).sin();
        let x_progress = 0.5 * (1.0 + sine_x) * width_f;
        let y_progress = 0.5 * (1.0 + sine_y) * height_f;

        let basis1 = hsv_basis(self.hsv1.h, self.hsv1.s);
        let basis2 = hsv_basis(self.hsv2.h, self.hsv2.s);
        let glow_exp = 0.01 * self.glow;
        let mut index = 0usize;

        for y in 0..height {
            let dy = (y_progress - y as f32).abs();
            let y_pct = beam_falloff(dy, inv_height, self.thickness, glow_exp);
            let v2 = self.hsv2.v * (1.0 - y_pct);
            let beam2 = hsv_basis_to_rgb(basis2, v2);

            for x in 0..width {
                if index >= pixels.len() {
                    self.advance();
                    return;
                }

                let dx = (x_progress - x as f32).abs();
                let x_pct = beam_falloff(dx, inv_width, self.thickness, glow_exp);
                let v1 = self.hsv1.v * (1.0 - x_pct);
                let beam1 = hsv_basis_to_rgb(basis1, v1);

                pixels[index] = screen_blend_rgb(beam1, beam2);
                index += 1;
            }
        }

        if index < pixels.len() {
            pixels[index..].fill(SkydimoRgb::default());
        }

        self.advance();
    }

    fn advance(&mut self) {
        self.progress += self.speed / 600.0;
        if self.random_colors {
            self.hsv1.h = (self.hsv1.h + 1.0).rem_euclid(360.0);
            self.hsv2.h = (self.hsv2.h + 1.0).rem_euclid(360.0);
        }
    }

    fn apply_user_colors(&mut self) {
        self.hsv1 = rgb_to_hsv(self.user_colors[0]);
        self.hsv2 = rgb_to_hsv(self.user_colors[1]);
    }
}

unsafe extern "C" fn crossing_beams_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(CrossingBeamsEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn crossing_beams_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<CrossingBeamsEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn crossing_beams_resize(
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

unsafe extern "C" fn crossing_beams_update_params_json(
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

unsafe extern "C" fn crossing_beams_tick(
    instance: *mut c_void,
    _elapsed_seconds: f64,
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
            effect.advance();
            return 0;
        }

        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(pixels);
        0
    })
}

unsafe extern "C" fn crossing_beams_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be writable for one host-compatible `SkydimoPluginApiV1`.
/// The host must request the ABI version declared in this plugin manifest.
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
                    create: Some(crossing_beams_create),
                    destroy: Some(crossing_beams_destroy),
                    resize: Some(crossing_beams_resize),
                    update_params_json: Some(crossing_beams_update_params_json),
                    tick: Some(crossing_beams_tick),
                    is_ready: Some(crossing_beams_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut CrossingBeamsEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<CrossingBeamsEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline(always)]
const fn rgb(r: u8, g: u8, b: u8) -> SkydimoRgb {
    SkydimoRgb { r, g, b }
}

#[inline(always)]
fn beam_falloff(distance: f32, inv_axis_len: f32, thickness: f32, glow_exp: f32) -> f32 {
    let normalized = distance * inv_axis_len;
    if distance > thickness {
        normalized.powf(glow_exp).min(1.0)
    } else {
        normalized.min(1.0)
    }
}

#[inline(always)]
fn screen_blend_channel(a: u8, b: u8) -> u8 {
    let product = (255u32 - a as u32) * (255u32 - b as u32);
    (255u32 - product.div_ceil(255)) as u8
}

#[inline(always)]
fn screen_blend_rgb(a: SkydimoRgb, b: SkydimoRgb) -> SkydimoRgb {
    rgb(
        screen_blend_channel(a.r, b.r),
        screen_blend_channel(a.g, b.g),
        screen_blend_channel(a.b, b.b),
    )
}

fn rgb_to_hsv(rgb: SkydimoRgb) -> Hsv {
    let rf = rgb.r as f32 / 255.0;
    let gf = rgb.g as f32 / 255.0;
    let bf = rgb.b as f32 / 255.0;
    let maxc = rf.max(gf).max(bf);
    let minc = rf.min(gf).min(bf);
    let delta = maxc - minc;
    let saturation = if maxc > 0.0 { delta / maxc } else { 0.0 };

    let hue = if delta <= 0.0 {
        0.0
    } else if maxc == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if maxc == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };

    Hsv {
        h: hue.rem_euclid(360.0),
        s: saturation,
        v: maxc,
    }
}

fn hsv_basis(h: f32, s: f32) -> HsvBasis {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let c = s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = 1.0 - c;

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

    HsvBasis {
        r: r + m,
        g: g + m,
        b: b + m,
    }
}

#[inline(always)]
fn hsv_basis_to_rgb(basis: HsvBasis, value: f32) -> SkydimoRgb {
    let scale = value.clamp(0.0, 1.0) * 255.0;
    rgb(
        to_u8(basis.r * scale),
        to_u8(basis.g * scale),
        to_u8(basis.b * scale),
    )
}

#[inline(always)]
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
    if end == 0 {
        return None;
    }
    raw[..end].trim().parse::<f32>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_after_key(json, key)?;
    if raw.starts_with("true") {
        Some(true)
    } else if raw.starts_with("false") {
        Some(false)
    } else {
        json_number(json, key).map(|value| value != 0.0)
    }
}

fn json_color_array2(json: &str, key: &str) -> Option<[SkydimoRgb; 2]> {
    let mut raw = json_value_after_key(json, key)?.trim_start();
    raw = raw.strip_prefix('[')?;

    let mut colors = [rgb(255, 0, 0), rgb(255, 0, 0)];
    let mut count = 0usize;
    loop {
        raw = raw.trim_start();
        if raw.starts_with(']') {
            break;
        }
        if let Some(rest) = raw.strip_prefix(',') {
            raw = rest.trim_start();
        }
        let Some(rest) = raw.strip_prefix('"') else {
            break;
        };
        let Some((value, remaining)) = read_json_string(rest) else {
            break;
        };
        if count < colors.len() {
            colors[count] = hex_to_rgb(value);
        }
        count += 1;
        raw = remaining;
    }

    (count >= 2).then_some(colors)
}

fn read_json_string(raw: &str) -> Option<(&str, &str)> {
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

fn hex_to_rgb(raw: &str) -> SkydimoRgb {
    let hex = raw.trim().strip_prefix('#').unwrap_or(raw.trim());
    if hex.len() != 6 {
        return rgb(255, 0, 0);
    }
    let bytes = hex.as_bytes();
    let Some(r) = parse_hex_byte(bytes[0], bytes[1]) else {
        return rgb(255, 0, 0);
    };
    let Some(g) = parse_hex_byte(bytes[2], bytes[3]) else {
        return rgb(255, 0, 0);
    };
    let Some(b) = parse_hex_byte(bytes[4], bytes[5]) else {
        return rgb(255, 0, 0);
    };
    rgb(r, g, b)
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
        hex_to_rgb, json_bool, json_color_array2, json_number, screen_blend_channel,
        skydimo_plugin_get_api, CrossingBeamsEffect,
    };
    use crate::abi::{SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION};

    #[test]
    fn parses_crossing_beams_params() {
        let json = r##"{"speed":72,"h_speed":3,"random_colors":false,"colors":["#FF0000","#00AAFF"]}"##;
        assert_eq!(json_number(json, "speed"), Some(72.0));
        assert_eq!(json_number(json, "h_speed"), Some(3.0));
        assert_eq!(json_bool(json, "random_colors"), Some(false));

        let colors = json_color_array2(json, "colors").expect("colors should parse");
        assert_eq!((colors[0].r, colors[0].g, colors[0].b), (255, 0, 0));
        assert_eq!((colors[1].r, colors[1].g, colors[1].b), (0, 170, 255));
    }

    #[test]
    fn color_helpers_match_lua_contract() {
        assert_eq!(screen_blend_channel(128, 128), 191);

        let invalid = hex_to_rgb("#0af");
        assert_eq!((invalid.r, invalid.g, invalid.b), (255, 0, 0));
    }

    #[test]
    fn renders_default_crossing_center() {
        let mut effect = CrossingBeamsEffect::default();
        effect.resize(4, 4, 16);
        let mut pixels = [SkydimoRgb::default(); 16];
        effect.tick(&mut pixels);

        assert_eq!((pixels[10].r, pixels[10].g, pixels[10].b), (255, 0, 255));
        assert!(effect.progress > 0.0);
    }

    #[test]
    fn exports_effect_api_for_current_abi() {
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
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }
}
