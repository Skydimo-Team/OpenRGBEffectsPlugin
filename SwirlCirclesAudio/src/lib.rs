mod abi;

use std::ffi::{c_char, c_void};

use abi::{
    EffectAudioCaptureFn, SkydimoAudioFrameV1, SkydimoControllerApiV1, SkydimoEffectApiV1,
    SkydimoExtensionApiV1, SkydimoHostApiV1, SkydimoPluginApiV1, SkydimoRgb,
    SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_COLOR_1: SkydimoRgb = SkydimoRgb { r: 255, g: 0, b: 0 };
const DEFAULT_COLOR_2: SkydimoRgb = SkydimoRgb { r: 0, g: 255, b: 0 };
const DEFAULT_COLORS: [SkydimoRgb; 2] = [DEFAULT_COLOR_1, DEFAULT_COLOR_2];
const AUDIO_BIN_SUM_LIMIT: usize = 256;

#[derive(Clone, Copy)]
struct Hsv {
    h: f32,
    s: f32,
    v: f32,
}

#[derive(Clone, Copy)]
struct Coord {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    glow: f32,
    radius: f32,
    avg_size: usize,
    color_mode: u8,
    colors: [SkydimoRgb; 2],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            glow: 50.0,
            radius: 0.0,
            avg_size: 8,
            color_mode: 0,
            colors: DEFAULT_COLORS,
        }
    }
}

#[derive(Clone, Copy)]
struct NativeHost {
    host_ctx: *mut c_void,
    audio_capture: Option<EffectAudioCaptureFn>,
}

#[derive(Clone, Copy)]
struct RenderContext {
    width: f32,
    height: f32,
    x1: f32,
    y1: f32,
    glow_mult: f32,
}

struct SwirlCirclesAudio {
    config: Config,
    progress: f32,
    current_level: f32,
    hsv1: Hsv,
    hsv2: Hsv,
    width: usize,
    height: usize,
    coords: Vec<Coord>,
    host: NativeHost,
}

impl SwirlCirclesAudio {
    fn new(host: NativeHost) -> Self {
        let mut effect = Self {
            config: Config::default(),
            progress: 0.0,
            current_level: 0.0,
            hsv1: Hsv {
                h: 0.0,
                s: 1.0,
                v: 1.0,
            },
            hsv2: Hsv {
                h: 180.0,
                s: 1.0,
                v: 1.0,
            },
            width: 0,
            height: 1,
            coords: Vec::new(),
            host,
        };
        effect.reset_random_colors();
        effect
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = (width as usize).max(1);
        self.height = (height as usize).max(1);
        let count = if led_count == 0 {
            self.width.saturating_mul(self.height).max(1)
        } else {
            led_count as usize
        };
        self.rebuild_coords(count);
    }

    fn update_params_json(&mut self, json: &str) {
        if json.trim().is_empty() {
            return;
        }

        if let Some(speed) = parse_number(json, "speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(glow) = parse_number(json, "glow") {
            self.config.glow = glow.clamp(1.0, 100.0);
        }
        if let Some(radius) = parse_number(json, "radius") {
            self.config.radius = radius.clamp(0.0, 100.0);
        }
        if let Some(avg_size) = parse_number(json, "avgSize") {
            self.config.avg_size = clamp_usize(avg_size.round() as isize, 1, 256);
        }

        let mut mode_changed = false;
        if let Some(color_mode) = parse_number(json, "colorMode") {
            let next_mode = if color_mode.round() as i32 == 1 { 1 } else { 0 };
            mode_changed = next_mode != self.config.color_mode;
            self.config.color_mode = next_mode;
        }

        let mut colors_changed = false;
        if let Some(colors) = parse_colors_array(json, "colors") {
            self.config.colors = normalize_color_pair_from_array(&colors);
            colors_changed = true;
        } else {
            let color1 = parse_string(json, "color1");
            let color2 = parse_string(json, "color2");
            if color1.is_some() || color2.is_some() {
                self.config.colors = [
                    color1
                        .as_deref()
                        .map(parse_hex_color)
                        .unwrap_or(self.config.colors[0]),
                    color2
                        .as_deref()
                        .map(parse_hex_color)
                        .unwrap_or(self.config.colors[1]),
                ];
                colors_changed = true;
            }
        }

        if mode_changed {
            if self.config.color_mode == 0 {
                self.reset_random_colors();
            } else {
                self.reset_custom_colors();
            }
        } else if colors_changed && self.config.color_mode == 1 {
            self.reset_custom_colors();
        }
    }

    fn tick(&mut self, buffer: &mut [SkydimoRgb]) {
        if buffer.is_empty() {
            return;
        }

        if self.coords.len() != buffer.len() {
            self.rebuild_coords(buffer.len());
        }

        let Some(audio_capture) = self.host.audio_capture else {
            fill_black(buffer);
            return;
        };

        let mut frame = SkydimoAudioFrameV1::default();
        let avg_size = self.config.avg_size.clamp(1, 256);
        let status = unsafe { audio_capture(self.host.host_ctx, avg_size, &mut frame) };
        if status <= 0 || frame.bins.ptr.is_null() || frame.bins.len == 0 {
            self.current_level = 0.0;
            fill_black(buffer);
            return;
        }

        let bins = unsafe { std::slice::from_raw_parts(frame.bins.ptr, frame.bins.len) };
        let level = bins
            .iter()
            .take(AUDIO_BIN_SUM_LIMIT)
            .copied()
            .filter(|value| value.is_finite())
            .sum::<f32>()
            .max(0.0);
        self.current_level = level;

        let width = self.width.max(1) as f32;
        let height = self.height.max(1) as f32;
        let half_width = width * 0.5;
        let half_height = height * 0.5;
        let x1 = half_width + half_width * self.progress.cos();
        let y1 = half_height + half_height * self.progress.sin();
        let glow_mult = 0.001 * self.config.glow;
        let render = RenderContext {
            width,
            height,
            x1,
            y1,
            glow_mult,
        };

        for (index, pixel) in buffer.iter_mut().enumerate() {
            let coord = self.coords.get(index).copied().unwrap_or(Coord {
                x: index as f32,
                y: 0.0,
            });
            *pixel = self.color_at(coord, render);
        }

        self.progress += 0.1 * self.config.speed / 60.0;
        if self.config.color_mode == 0 {
            self.hsv1.h = (self.hsv1.h + 1.0).rem_euclid(360.0);
            self.hsv2.h = (self.hsv2.h + 1.0).rem_euclid(360.0);
        }
    }

    fn color_at(&self, coord: Coord, render: RenderContext) -> SkydimoRgb {
        let radius = self.config.radius;
        let level = self.current_level;
        let distance_scale = (render.height + render.width).max(1.0);

        let dx1 = render.x1 - coord.x;
        let dy1 = render.y1 - coord.y;
        let distance1 = (dx1.mul_add(dx1, dy1 * dy1)).sqrt();
        let dist1_pct = distance_pct(
            distance1,
            radius,
            level,
            render.glow_mult,
            distance_scale,
        );
        let v1 = (self.hsv1.v * (1.0 - dist1_pct)).clamp(0.0, 1.0);
        let rgb1 = hsv_to_rgb(self.hsv1.h, self.hsv1.s, v1);

        let x2 = render.width - render.x1;
        let y2 = render.height - render.y1;
        let dx2 = x2 - coord.x;
        let dy2 = y2 - coord.y;
        let distance2 = (dx2.mul_add(dx2, dy2 * dy2)).sqrt();
        let dist2_pct = distance_pct(
            distance2,
            radius,
            level,
            render.glow_mult,
            distance_scale,
        );
        let v2 = (self.hsv2.v * (1.0 - dist2_pct)).clamp(0.0, 1.0);
        let rgb2 = hsv_to_rgb(self.hsv2.h, self.hsv2.s, v2);

        SkydimoRgb {
            r: screen_blend(rgb1.r, rgb2.r),
            g: screen_blend(rgb1.g, rgb2.g),
            b: screen_blend(rgb1.b, rgb2.b),
        }
    }

    fn rebuild_coords(&mut self, led_count: usize) {
        self.coords.clear();
        self.coords.reserve(led_count);

        if self.height == 1 || self.width == 1 {
            self.coords.extend((0..led_count).map(|index| Coord {
                x: index as f32,
                y: 0.0,
            }));
            return;
        }

        let mut remaining = led_count;
        for y in 0..self.height {
            if remaining == 0 {
                break;
            }
            for x in 0..self.width {
                if remaining == 0 {
                    break;
                }
                self.coords.push(Coord {
                    x: x as f32,
                    y: y as f32,
                });
                remaining -= 1;
            }
        }

        let existing = self.coords.len();
        self.coords.extend((existing..led_count).map(|index| Coord {
            x: index as f32,
            y: 0.0,
        }));
    }

    fn reset_random_colors(&mut self) {
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
    }

    fn reset_custom_colors(&mut self) {
        self.hsv1 = rgb_to_hsv(self.config.colors[0]);
        self.hsv2 = rgb_to_hsv(self.config.colors[1]);
    }
}

unsafe extern "C" fn swirl_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    if out_instance.is_null() {
        return -1;
    }

    let native_host = if host.is_null() {
        NativeHost {
            host_ctx: std::ptr::null_mut(),
            audio_capture: None,
        }
    } else {
        NativeHost {
            host_ctx: (*host).host_ctx,
            audio_capture: (*host).effect_audio_capture,
        }
    };

    let effect = Box::new(SwirlCirclesAudio::new(native_host));
    *out_instance = Box::into_raw(effect).cast::<c_void>();
    0
}

unsafe extern "C" fn swirl_destroy(instance: *mut c_void) {
    if !instance.is_null() {
        drop(Box::from_raw(instance.cast::<SwirlCirclesAudio>()));
    }
}

unsafe extern "C" fn swirl_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    effect.resize(width, height, led_count);
    0
}

unsafe extern "C" fn swirl_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    if ptr.is_null() || len == 0 {
        return 0;
    }

    let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), len);
    let Ok(json) = std::str::from_utf8(bytes) else {
        return -2;
    };
    effect.update_params_json(json);
    0
}

unsafe extern "C" fn swirl_tick(
    instance: *mut c_void,
    _elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    if buffer.is_null() && len > 0 {
        return -2;
    }
    if len == 0 {
        return 0;
    }

    let pixels = std::slice::from_raw_parts_mut(buffer, len);
    effect.tick(pixels);
    0
}

unsafe extern "C" fn swirl_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer provided by the Skydimo host.
/// The host must pass the ABI version it expects in `requested_abi_version`.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    if out_api.is_null() || requested_abi_version != SKYDIMO_NATIVE_C_ABI_VERSION {
        return -1;
    }

    *out_api = SkydimoPluginApiV1 {
        size: std::mem::size_of::<SkydimoPluginApiV1>() as u32,
        abi_version: SKYDIMO_NATIVE_C_ABI_VERSION,
        kind_mask: SKYDIMO_PLUGIN_KIND_EFFECT,
        effect: SkydimoEffectApiV1 {
            size: std::mem::size_of::<SkydimoEffectApiV1>() as u32,
            create: Some(swirl_create),
            destroy: Some(swirl_destroy),
            resize: Some(swirl_resize),
            update_params_json: Some(swirl_update_params_json),
            tick: Some(swirl_tick),
            is_ready: Some(swirl_is_ready),
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
    0
}

unsafe fn effect_mut(instance: *mut c_void) -> Option<&'static mut SwirlCirclesAudio> {
    if instance.is_null() {
        None
    } else {
        Some(&mut *instance.cast::<SwirlCirclesAudio>())
    }
}

#[inline]
fn distance_pct(distance: f32, radius: f32, level: f32, glow_mult: f32, distance_scale: f32) -> f32 {
    if distance < radius {
        1.0 / (0.000_001 + level)
    } else {
        (distance / distance_scale).powf(glow_mult * level)
    }
}

#[inline]
fn fill_black(buffer: &mut [SkydimoRgb]) {
    buffer.fill(SkydimoRgb { r: 0, g: 0, b: 0 });
}

#[inline]
fn screen_blend(a: u8, b: u8) -> u8 {
    let inv = (255_u32 - u32::from(a)) * (255_u32 - u32::from(b));
    (255_u32 - inv.div_ceil(255)).min(255) as u8
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> SkydimoRgb {
    let h = h.rem_euclid(360.0);
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
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

fn rgb_to_hsv(rgb: SkydimoRgb) -> Hsv {
    let r = f32::from(rgb.r) / 255.0;
    let g = f32::from(rgb.g) / 255.0;
    let b = f32::from(rgb.b) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let s = if max == 0.0 { 0.0 } else { delta / max };
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };

    Hsv { h, s, v: max }
}

#[inline]
fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn normalize_color_pair_from_array(colors: &[String]) -> [SkydimoRgb; 2] {
    [
        colors
            .first()
            .map(String::as_str)
            .map(parse_hex_color)
            .unwrap_or(DEFAULT_COLOR_1),
        colors
            .get(1)
            .map(String::as_str)
            .map(parse_hex_color)
            .unwrap_or(DEFAULT_COLOR_2),
    ]
}

fn parse_hex_color(raw: &str) -> SkydimoRgb {
    let value = raw.trim();
    let value = value.strip_prefix('#').unwrap_or(value);
    let bytes = value.as_bytes();
    if bytes.len() != 6 {
        return SkydimoRgb { r: 0, g: 0, b: 0 };
    }

    let Some(r) = parse_hex_byte(bytes[0], bytes[1]) else {
        return SkydimoRgb { r: 0, g: 0, b: 0 };
    };
    let Some(g) = parse_hex_byte(bytes[2], bytes[3]) else {
        return SkydimoRgb { r: 0, g: 0, b: 0 };
    };
    let Some(b) = parse_hex_byte(bytes[4], bytes[5]) else {
        return SkydimoRgb { r: 0, g: 0, b: 0 };
    };
    SkydimoRgb { r, g, b }
}

fn parse_hex_byte(high: u8, low: u8) -> Option<u8> {
    Some((parse_hex_nibble(high)? << 4) | parse_hex_nibble(low)?)
}

fn parse_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn clamp_usize(value: isize, min: usize, max: usize) -> usize {
    value.clamp(min as isize, max as isize) as usize
}

fn parse_number(json: &str, key: &str) -> Option<f32> {
    let mut value = find_value_start(json, key)?;
    if let Some(stripped) = value.strip_prefix('"') {
        value = stripped;
    }

    let end = value
        .bytes()
        .position(|byte| !matches!(byte, b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E'))
        .unwrap_or(value.len());
    if end == 0 {
        return None;
    }
    value[..end].trim().parse::<f32>().ok()
}

fn parse_string(json: &str, key: &str) -> Option<String> {
    parse_json_string(find_value_start(json, key)?).map(|(value, _)| value)
}

fn parse_colors_array(json: &str, key: &str) -> Option<Vec<String>> {
    let mut rest = find_value_start(json, key)?.trim_start();
    rest = rest.strip_prefix('[')?;
    let mut out = Vec::with_capacity(2);

    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') {
            return Some(out);
        }
        if let Some((value, consumed)) = parse_json_string(rest) {
            out.push(value);
            rest = &rest[consumed..];
        } else {
            return None;
        }

        rest = rest.trim_start();
        if let Some(next) = rest.strip_prefix(',') {
            rest = next;
        } else if rest.starts_with(']') {
            return Some(out);
        } else {
            return None;
        }
    }
}

fn find_value_start<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{key}\"");
    let pos = json.find(&pattern)?;
    let after_key = &json[pos + pattern.len()..];
    let colon_pos = after_key.find(':')?;
    Some(after_key[colon_pos + 1..].trim_start())
}

fn parse_json_string(raw: &str) -> Option<(String, usize)> {
    let raw = raw.trim_start();
    if !raw.starts_with('"') {
        return None;
    }

    let mut out = String::new();
    let mut escaped = false;
    for (idx, ch) in raw[1..].char_indices() {
        let consumed = idx + 1 + ch.len_utf8();
        if escaped {
            match ch {
                '"' | '\\' | '/' => out.push(ch),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                _ => out.push(ch),
            }
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            return Some((out, consumed));
        } else {
            out.push(ch);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{normalize_color_pair_from_array, parse_colors_array, parse_hex_color, parse_number};

    #[test]
    fn parses_numeric_params() {
        let json = r#"{"speed":75,"avgSize":"12","colorMode":1}"#;
        assert_eq!(parse_number(json, "speed"), Some(75.0));
        assert_eq!(parse_number(json, "avgSize"), Some(12.0));
        assert_eq!(parse_number(json, "colorMode"), Some(1.0));
    }

    #[test]
    fn parses_color_pairs() {
        let colors = parse_colors_array(r##"{"colors":["#336699","#ABCDEF"]}"##, "colors")
            .expect("colors should parse");
        let pair = normalize_color_pair_from_array(&colors);
        assert_eq!((pair[0].r, pair[0].g, pair[0].b), (0x33, 0x66, 0x99));
        assert_eq!((pair[1].r, pair[1].g, pair[1].b), (0xAB, 0xCD, 0xEF));
    }

    #[test]
    fn invalid_hex_matches_lua_black_fallback() {
        let color = parse_hex_color("#bad");
        assert_eq!((color.r, color.g, color.b), (0, 0, 0));
    }
}
