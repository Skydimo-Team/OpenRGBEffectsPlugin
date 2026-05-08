mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const MODE_CLOCKWISE: u8 = 0;
const MODE_COUNTER_CLOCKWISE: u8 = 1;
const MODE_PENDULUM: u8 = 2;
const MODE_WIPERS: u8 = 3;
const MODE_SWING_H: u8 = 4;
const MODE_SWING_V: u8 = 5;

const TAU: f32 = std::f32::consts::PI * 2.0;
const PROGRESS_RADIANS_PER_SECOND: f32 = 0.05;
const RANDOM_HUE_DEGREES_PER_SECOND: f32 = 30.0;

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

struct RotatingBeamEffect {
    speed: f32,
    glow: f32,
    thickness: f32,
    mode: u8,
    random_colors: bool,
    progress: f32,
    user_colors: [SkydimoRgb; 2],
    hsv1: Hsv,
    hsv2: Hsv,
    width: usize,
    height: usize,
    last_elapsed_seconds: Option<f64>,
}

impl Default for RotatingBeamEffect {
    fn default() -> Self {
        let mut effect = Self {
            speed: 50.0,
            glow: 10.0,
            thickness: 0.0,
            mode: MODE_CLOCKWISE,
            random_colors: false,
            progress: 0.0,
            user_colors: [
                SkydimoRgb { r: 255, g: 0, b: 0 },
                SkydimoRgb { r: 0, g: 0, b: 255 },
            ],
            hsv1: Hsv::default(),
            hsv2: Hsv::default(),
            width: 0,
            height: 1,
            last_elapsed_seconds: None,
        };
        effect.apply_user_colors();
        effect
    }
}

impl RotatingBeamEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback = (led_count as usize).max(1);
        self.width = if width == 0 { fallback } else { width as usize };
        self.height = height.max(1) as usize;
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(glow) = json_number(json, "glow") {
            self.glow = glow.clamp(1.0, 50.0);
        }
        if let Some(thickness) = json_number(json, "thickness") {
            self.thickness = thickness.clamp(0.0, 99.0);
        }
        if let Some(mode) = json_number(json, "mode") {
            let mode = (mode + 0.5).floor().clamp(0.0, MODE_SWING_V as f32) as u8;
            self.mode = mode;
        }

        let mut colors_updated = false;
        if let Some(colors) = json_color_array2(json, "colors") {
            for (index, color) in colors.into_iter().enumerate() {
                if let Some(color) = color {
                    self.user_colors[index] = color;
                    colors_updated = true;
                }
            }
        }

        let mut turned_off_random = false;
        if let Some(enabled) = json_bool(json, "random_colors") {
            if enabled != self.random_colors {
                self.set_random_colors_enabled(enabled);
                turned_off_random = !self.random_colors;
            }
        }

        if !self.random_colors && (colors_updated || turned_off_random) {
            self.apply_user_colors();
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
        let height = self.height.max(1);
        let (p1x, p1y, p2x, p2y) = self.resolve_line_points();
        let bg = hsv_to_rgb(self.hsv2);
        let beam_basis = hsv_basis(self.hsv1.h, self.hsv1.s);

        if height <= 1 {
            let render_count = width.min(pixels.len());
            let dim = width.saturating_sub(1) as f32;
            let sample_y = width as f32 * 0.25;
            for (x, pixel) in pixels.iter_mut().take(render_count).enumerate() {
                *pixel = render_sample(RenderSample {
                    x: x as f32,
                    y: sample_y,
                    line: BeamLine {
                        p1x,
                        p1y,
                        p2x,
                        p2y,
                        width: dim,
                        height: dim,
                    },
                    avg_dim: dim,
                    bg,
                    beam_basis,
                    beam_max_v: self.hsv1.v,
                    glow: self.glow,
                    thickness: self.thickness,
                });
            }
            if render_count < pixels.len() {
                pixels[render_count..].fill(SkydimoRgb::default());
            }
        } else {
            let canvas_w = width.saturating_sub(1) as f32;
            let canvas_h = height.saturating_sub(1) as f32;
            let avg_dim = 0.5 * (canvas_w + canvas_h);
            let mut index = 0usize;
            for y in 0..height {
                for x in 0..width {
                    if index >= pixels.len() {
                        self.advance(elapsed_seconds);
                        return;
                    }
                    pixels[index] = render_sample(RenderSample {
                        x: x as f32,
                        y: y as f32,
                        line: BeamLine {
                            p1x,
                            p1y,
                            p2x,
                            p2y,
                            width: canvas_w,
                            height: canvas_h,
                        },
                        avg_dim,
                        bg,
                        beam_basis,
                        beam_max_v: self.hsv1.v,
                        glow: self.glow,
                        thickness: self.thickness,
                    });
                    index += 1;
                }
            }
            if index < pixels.len() {
                pixels[index..].fill(SkydimoRgb::default());
            }
        }

        self.advance(elapsed_seconds);
    }

    fn resolve_line_points(&self) -> (f32, f32, f32, f32) {
        match self.mode {
            MODE_CLOCKWISE => {
                let x = 0.5 * (1.0 + self.progress.cos());
                let y = 0.5 * (1.0 + self.progress.sin());
                (x, y, 1.0 - x, 1.0 - y)
            }
            MODE_COUNTER_CLOCKWISE => {
                let x = 0.5 * (1.0 + (-self.progress).cos());
                let y = 0.5 * (1.0 + (-self.progress).sin());
                (x, y, 1.0 - x, 1.0 - y)
            }
            MODE_PENDULUM => {
                let x = 0.5 * (1.0 + self.progress.cos());
                (0.5, 0.0, x, 1.0)
            }
            MODE_WIPERS => {
                let x = 0.5 * (1.0 + self.progress.cos());
                (x, 0.0, 0.5, 1.0)
            }
            MODE_SWING_H => {
                let x = 0.5 * (1.0 + self.progress.cos());
                (0.0, x, 1.0, 1.0 - x)
            }
            MODE_SWING_V => {
                let x = 0.5 * (1.0 + self.progress.cos());
                (x, 0.0, 1.0 - x, 1.0)
            }
            _ => (0.0, 0.0, 1.0, 1.0),
        }
    }

    fn advance(&mut self, elapsed_seconds: f64) {
        let delta = self
            .last_elapsed_seconds
            .map(|last| (elapsed_seconds - last).max(0.0) as f32)
            .unwrap_or(0.0);
        self.last_elapsed_seconds = Some(elapsed_seconds);
        if delta == 0.0 {
            return;
        }

        self.progress =
            (self.progress + delta * self.speed * PROGRESS_RADIANS_PER_SECOND).rem_euclid(TAU);

        if self.random_colors {
            let hue_delta = delta * RANDOM_HUE_DEGREES_PER_SECOND;
            self.hsv1.h = (self.hsv1.h + hue_delta).rem_euclid(360.0);
            self.hsv2.h = (self.hsv2.h + hue_delta).rem_euclid(360.0);
        }
    }

    fn set_random_colors_enabled(&mut self, enabled: bool) {
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
            self.apply_user_colors();
        }
    }

    fn apply_user_colors(&mut self) {
        self.hsv1 = rgb_to_hsv(self.user_colors[0]);
        self.hsv2 = rgb_to_hsv(self.user_colors[1]);
    }
}

#[derive(Clone, Copy)]
struct RenderSample {
    x: f32,
    y: f32,
    line: BeamLine,
    avg_dim: f32,
    bg: SkydimoRgb,
    beam_basis: HsvBasis,
    beam_max_v: f32,
    glow: f32,
    thickness: f32,
}

#[derive(Clone, Copy)]
struct BeamLine {
    p1x: f32,
    p1y: f32,
    p2x: f32,
    p2y: f32,
    width: f32,
    height: f32,
}

fn render_sample(sample: RenderSample) -> SkydimoRgb {
    let distance = line_distance(sample.x, sample.y, sample.line);
    let distance_norm = if sample.avg_dim > 0.0 {
        distance / sample.avg_dim
    } else {
        0.0
    };

    let exponent = if distance < sample.thickness {
        1.0
    } else {
        0.01 * sample.glow
    };
    let falloff = if distance_norm <= 0.0 {
        0.0
    } else {
        distance_norm.powf(exponent)
    };
    let beam_v = (sample.beam_max_v - sample.beam_max_v * falloff).clamp(0.0, 1.0);
    let beam = SkydimoRgb {
        r: to_u8(sample.beam_basis.r * beam_v * 255.0),
        g: to_u8(sample.beam_basis.g * beam_v * 255.0),
        b: to_u8(sample.beam_basis.b * beam_v * 255.0),
    };
    let mix = (1.0 - distance_norm).clamp(0.0, 1.0);

    SkydimoRgb {
        r: lerp_channel(sample.bg.r, beam.r, mix),
        g: lerp_channel(sample.bg.g, beam.g, mix),
        b: lerp_channel(sample.bg.b, beam.b, mix),
    }
}

fn line_distance(x0: f32, y0: f32, line: BeamLine) -> f32 {
    let x1 = line.p1x * line.width;
    let x2 = line.p2x * line.width;
    let y1 = line.p1y * line.height;
    let y2 = line.p2y * line.height;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let denom = dx.mul_add(dx, dy * dy).sqrt();
    if denom <= 1e-9 {
        return 0.0;
    }
    (dx * (y1 - y0) - (x1 - x0) * dy).abs() / denom
}

unsafe extern "C" fn rotating_beam_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(RotatingBeamEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn rotating_beam_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<RotatingBeamEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn rotating_beam_resize(
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

unsafe extern "C" fn rotating_beam_update_params_json(
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

unsafe extern "C" fn rotating_beam_tick(
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

unsafe extern "C" fn rotating_beam_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(rotating_beam_create),
                    destroy: Some(rotating_beam_destroy),
                    resize: Some(rotating_beam_resize),
                    update_params_json: Some(rotating_beam_update_params_json),
                    tick: Some(rotating_beam_tick),
                    is_ready: Some(rotating_beam_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut RotatingBeamEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<RotatingBeamEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
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

fn hsv_to_rgb(hsv: Hsv) -> SkydimoRgb {
    let basis = hsv_basis(hsv.h, hsv.s);
    SkydimoRgb {
        r: to_u8(basis.r * hsv.v * 255.0),
        g: to_u8(basis.g * hsv.v * 255.0),
        b: to_u8(basis.b * hsv.v * 255.0),
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
    if raw.starts_with("true") {
        Some(true)
    } else if raw.starts_with("false") {
        Some(false)
    } else {
        json_number(json, key).map(|value| value != 0.0)
    }
}

fn json_color_array2(json: &str, key: &str) -> Option<[Option<SkydimoRgb>; 2]> {
    let mut raw = json_value_after_key(json, key)?.trim_start();
    raw = raw.strip_prefix('[')?;
    let mut colors = [None, None];
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
            colors[count] = Some(hex_to_rgb(value));
        }
        count += 1;
        raw = remaining;
    }

    (count > 0).then_some(colors)
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
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() == 3 {
        let bytes = hex.as_bytes();
        let Some(r) = parse_hex_nibble(bytes[0]) else {
            return fallback_red();
        };
        let Some(g) = parse_hex_nibble(bytes[1]) else {
            return fallback_red();
        };
        let Some(b) = parse_hex_nibble(bytes[2]) else {
            return fallback_red();
        };
        return SkydimoRgb {
            r: r * 17,
            g: g * 17,
            b: b * 17,
        };
    }
    if hex.len() != 6 {
        return fallback_red();
    }
    let bytes = hex.as_bytes();
    let Some(r) = parse_hex_byte(bytes[0], bytes[1]) else {
        return fallback_red();
    };
    let Some(g) = parse_hex_byte(bytes[2], bytes[3]) else {
        return fallback_red();
    };
    let Some(b) = parse_hex_byte(bytes[4], bytes[5]) else {
        return fallback_red();
    };
    SkydimoRgb { r, g, b }
}

fn fallback_red() -> SkydimoRgb {
    SkydimoRgb {
        r: 255,
        g: 0,
        b: 0,
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

fn lerp_channel(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t + 0.5)
        .floor()
        .clamp(0.0, 255.0) as u8
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::{
        hex_to_rgb, json_bool, json_color_array2, json_number, line_distance, rgb_to_hsv, BeamLine,
        SkydimoRgb,
    };

    #[test]
    fn parses_number_bool_and_color_params() {
        let json = r##"{"speed":72,"random_colors":false,"colors":["#FF0000","#0af"]}"##;
        assert_eq!(json_number(json, "speed"), Some(72.0));
        assert_eq!(json_bool(json, "random_colors"), Some(false));

        let colors = json_color_array2(json, "colors").expect("colors should parse");
        let first = colors[0].expect("first color");
        let second = colors[1].expect("second color");
        assert_eq!((first.r, first.g, first.b), (255, 0, 0));
        assert_eq!((second.r, second.g, second.b), (0, 170, 255));
    }

    #[test]
    fn hex_parser_matches_lua_fallbacks() {
        let short = hex_to_rgb("#f0a");
        assert_eq!((short.r, short.g, short.b), (255, 0, 170));

        let invalid = hex_to_rgb("bad-input");
        assert_eq!((invalid.r, invalid.g, invalid.b), (255, 0, 0));
    }

    #[test]
    fn rgb_to_hsv_preserves_primary_colors() {
        let hsv = rgb_to_hsv(SkydimoRgb { r: 0, g: 0, b: 255 });
        assert!((hsv.h - 240.0).abs() < 0.01);
        assert!((hsv.s - 1.0).abs() < 0.01);
        assert!((hsv.v - 1.0).abs() < 0.01);
    }

    #[test]
    fn line_distance_is_zero_on_the_line() {
        let distance = line_distance(
            5.0,
            5.0,
            BeamLine {
                p1x: 0.0,
                p1y: 0.0,
                p2x: 1.0,
                p2y: 1.0,
                width: 10.0,
                height: 10.0,
            },
        );
        assert!(distance.abs() < 0.001);
    }
}
