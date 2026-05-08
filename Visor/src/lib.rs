mod abi;

use std::ffi::{c_char, c_void};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_MIN_VERSION,
    SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f64 = 50.0;
const DEFAULT_WIDTH_PCT: f64 = 20.0;
const DEFAULT_COLOR_0: SkydimoRgb = SkydimoRgb { r: 255, g: 0, b: 0 };
const DEFAULT_COLOR_1: SkydimoRgb = SkydimoRgb { r: 0, g: 0, b: 255 };

struct VisorEffect {
    speed: f64,
    width_pct: f64,
    random_enabled: bool,
    user_c0: SkydimoRgb,
    user_c1: SkydimoRgb,
    c0: SkydimoRgb,
    c1: SkydimoRgb,
    progress: f64,
    last_step: bool,
    last_t: Option<f64>,
    width: usize,
    height: usize,
    rng: FastRng,
}

impl VisorEffect {
    fn new() -> Self {
        Self {
            speed: DEFAULT_SPEED,
            width_pct: DEFAULT_WIDTH_PCT,
            random_enabled: false,
            user_c0: DEFAULT_COLOR_0,
            user_c1: DEFAULT_COLOR_1,
            c0: DEFAULT_COLOR_0,
            c1: DEFAULT_COLOR_1,
            progress: 0.0,
            last_step: false,
            last_t: None,
            width: 0,
            height: 1,
            rng: FastRng::seeded(seed_now()),
        }
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = parse_json_number(json, "speed") {
            self.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(width) = parse_json_number(json, "width") {
            self.width_pct = width.clamp(1.0, 100.0);
        }
        if let Some(random) = parse_json_bool(json, "random") {
            self.random_enabled = random;
        }
        if let Some(colors) =
            parse_json_string_array(json, "colors").filter(|colors| colors.len() >= 2)
        {
            if let Some(color) = colors.first().and_then(|value| parse_hex_color(value)) {
                self.user_c0 = color;
            }
            if let Some(color) = colors.get(1).and_then(|value| parse_hex_color(value)) {
                self.user_c1 = color;
            }
        }
    }

    fn render(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width.max(1)
        };
        let height = self.height.max(1);

        if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            let delta = match self.last_t {
                Some(last_t) if elapsed_seconds >= last_t => elapsed_seconds - last_t,
                Some(_) | None => elapsed_seconds,
            };
            self.last_t = Some(elapsed_seconds);

            let next_progress = self.progress + 0.01 * self.speed * delta;
            self.progress = next_progress - next_progress.floor();
        }

        let w = 0.01 * self.width_pct;
        let p = self.progress;
        let step = p < 0.5;
        let p_step = if step { 2.0 * p } else { 2.0 * (1.0 - p) };

        let flipping = self.last_step != step;
        if flipping {
            self.last_step = step;
        }

        if flipping && self.random_enabled {
            self.c0 = random_rgb_color(&mut self.rng);
            self.c1 = random_rgb_color(&mut self.rng);
        } else if !self.random_enabled {
            self.c0 = self.user_c0;
            self.c1 = self.user_c1;
        }

        if height <= 1 {
            for (led, pixel) in pixels.iter_mut().enumerate() {
                *pixel = get_color(led, width, w, p_step, step, self.c0, self.c1);
            }
            return;
        }

        let mut idx = 0usize;
        for _ in 0..height {
            for col in 0..width {
                if idx >= pixels.len() {
                    return;
                }
                pixels[idx] = get_color(col, width, w, p_step, step, self.c0, self.c1);
                idx += 1;
            }
        }
    }
}

unsafe extern "C" fn visor_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    if out_instance.is_null() {
        return -1;
    }

    let effect = Box::new(VisorEffect::new());
    unsafe {
        *out_instance = Box::into_raw(effect).cast::<c_void>();
    }
    0
}

unsafe extern "C" fn visor_destroy(instance: *mut c_void) {
    if !instance.is_null() {
        unsafe {
            drop(Box::from_raw(instance.cast::<VisorEffect>()));
        }
    }
}

unsafe extern "C" fn visor_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    effect.width = if width == 0 {
        led_count.max(1) as usize
    } else {
        width as usize
    };
    effect.height = height.max(1) as usize;
    0
}

unsafe extern "C" fn visor_update_params_json(
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

    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    let Ok(json) = std::str::from_utf8(bytes) else {
        return -2;
    };
    effect.update_params(json);
    0
}

unsafe extern "C" fn visor_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
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

    let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
    effect.render(elapsed_seconds, pixels);
    0
}

unsafe extern "C" fn visor_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a `SkydimoPluginApiV1`.
/// The host passes the ABI version it selected from the plugin manifest.
pub unsafe extern "C" fn skydimo_plugin_get_api(
    requested_abi_version: u32,
    _host: *const SkydimoHostApiV1,
    out_api: *mut SkydimoPluginApiV1,
) -> i32 {
    if out_api.is_null()
        || !(SKYDIMO_NATIVE_C_ABI_MIN_VERSION..=SKYDIMO_NATIVE_C_ABI_VERSION)
            .contains(&requested_abi_version)
    {
        return -1;
    }

    unsafe {
        *out_api = SkydimoPluginApiV1 {
            size: std::mem::size_of::<SkydimoPluginApiV1>() as u32,
            abi_version: requested_abi_version,
            kind_mask: SKYDIMO_PLUGIN_KIND_EFFECT,
            effect: SkydimoEffectApiV1 {
                size: std::mem::size_of::<SkydimoEffectApiV1>() as u32,
                create: Some(visor_create),
                destroy: Some(visor_destroy),
                resize: Some(visor_resize),
                update_params_json: Some(visor_update_params_json),
                tick: Some(visor_tick),
                is_ready: Some(visor_is_ready),
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
}

unsafe fn effect_mut(instance: *mut c_void) -> Option<&'static mut VisorEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<VisorEffect>() })
    }
}

fn get_color(
    i: usize,
    count: usize,
    w: f64,
    p_step: f64,
    step: bool,
    c0: SkydimoRgb,
    c1: SkydimoRgb,
) -> SkydimoRgb {
    let count = count.max(1);
    let w = (1.5 / count as f64).max(w);
    let x_step = p_step * (1.0 + 4.0 * w) - 1.5 * w;
    let count_for_position = count.max(2);
    let x = i as f64 / (count_for_position - 1) as f64;
    let dist = x_step - x;

    if dist < 0.0 {
        let l = ((w + dist) / w).clamp(0.0, 1.0);
        return if step {
            enlight(c1, l)
        } else {
            enlight(c0, l)
        };
    }

    if dist > w {
        let l = (1.0 - ((dist - w) / w)).clamp(0.0, 1.0);
        return if step {
            enlight(c0, l)
        } else {
            enlight(c1, l)
        };
    }

    let interp = ((w - dist) / w).clamp(0.0, 1.0);
    if step {
        interpolate(c0, c1, interp)
    } else {
        interpolate(c1, c0, interp)
    }
}

fn enlight(color: SkydimoRgb, factor: f64) -> SkydimoRgb {
    let (h, s, v) = rgb_to_hsv(color);
    hsv_to_rgb(h, s, v * factor)
}

fn interpolate(color1: SkydimoRgb, color2: SkydimoRgb, fraction: f64) -> SkydimoRgb {
    SkydimoRgb {
        r: interpolate_channel(color1.r, color2.r, fraction),
        g: interpolate_channel(color1.g, color2.g, fraction),
        b: interpolate_channel(color1.b, color2.b, fraction),
    }
}

fn interpolate_channel(start: u8, end: u8, fraction: f64) -> u8 {
    ((end as f64 - start as f64) * fraction + start as f64)
        .floor()
        .clamp(0.0, 255.0) as u8
}

fn rgb_to_hsv(color: SkydimoRgb) -> (f64, f64, f64) {
    let rn = color.r as f64 / 255.0;
    let gn = color.g as f64 / 255.0;
    let bn = color.b as f64 / 255.0;
    let max_c = rn.max(gn).max(bn);
    let min_c = rn.min(gn).min(bn);
    let delta = max_c - min_c;

    let h = if delta == 0.0 {
        0.0
    } else if max_c == rn {
        60.0 * ((gn - bn) / delta).rem_euclid(6.0)
    } else if max_c == gn {
        60.0 * (((bn - rn) / delta) + 2.0)
    } else {
        60.0 * (((rn - gn) / delta) + 4.0)
    };

    let s = if max_c == 0.0 { 0.0 } else { delta / max_c };
    (h, s, max_c)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> SkydimoRgb {
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

fn to_u8(value: f64) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn parse_json_number(json: &str, key: &str) -> Option<f64> {
    let start = find_json_key_value_start(json, key)?;
    let raw = json.get(start..)?.trim_start();
    let raw = raw.strip_prefix('"').unwrap_or(raw);
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
    raw.get(..end)?.trim().parse::<f64>().ok()
}

fn parse_json_bool(json: &str, key: &str) -> Option<bool> {
    let start = find_json_key_value_start(json, key)?;
    let raw = json.get(start..)?.trim_start();
    if raw.starts_with("true") {
        Some(true)
    } else if raw.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn parse_json_string_array(json: &str, key: &str) -> Option<Vec<String>> {
    let mut i = skip_ws(json, find_json_key_value_start(json, key)?);
    if json.as_bytes().get(i) != Some(&b'[') {
        return None;
    }
    i += 1;

    let mut values = Vec::new();
    loop {
        i = skip_ws(json, i);
        match json.as_bytes().get(i) {
            Some(b']') => return Some(values),
            Some(b'"') => {
                let (value, end) = parse_json_string_at(json, i)?;
                values.push(value);
                i = skip_ws(json, end);
                match json.as_bytes().get(i) {
                    Some(b',') => i += 1,
                    Some(b']') => return Some(values),
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
}

fn find_json_key_value_start(json: &str, key: &str) -> Option<usize> {
    let mut i = 0usize;
    while i < json.len() {
        i = skip_ws(json, i);
        if json.as_bytes().get(i) != Some(&b'"') {
            i += json.get(i..)?.chars().next()?.len_utf8();
            continue;
        }

        let (candidate, end) = parse_json_string_at(json, i)?;
        let colon = skip_ws(json, end);
        if candidate == key && json.as_bytes().get(colon) == Some(&b':') {
            return Some(skip_ws(json, colon + 1));
        }
        i = end;
    }
    None
}

fn parse_json_string_at(json: &str, start: usize) -> Option<(String, usize)> {
    if json.as_bytes().get(start) != Some(&b'"') {
        return None;
    }

    let mut out = String::new();
    let mut i = start + 1;
    let mut escaped = false;
    while i < json.len() {
        let ch = json.get(i..)?.chars().next()?;
        let next = i + ch.len_utf8();
        if escaped {
            match ch {
                '"' | '\\' | '/' => out.push(ch),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let raw = json.get(next..next + 4)?;
                    let code = u16::from_str_radix(raw, 16).ok()?;
                    out.push(char::from_u32(code as u32)?);
                    i = next + 4;
                    escaped = false;
                    continue;
                }
                _ => return None,
            }
            escaped = false;
            i = next;
            continue;
        }

        match ch {
            '\\' => {
                escaped = true;
                i = next;
            }
            '"' => return Some((out, next)),
            _ => {
                out.push(ch);
                i = next;
            }
        }
    }
    None
}

fn skip_ws(input: &str, mut i: usize) -> usize {
    while matches!(input.as_bytes().get(i), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        i += 1;
    }
    i
}

fn parse_hex_color(value: &str) -> Option<SkydimoRgb> {
    let hex = value.trim().strip_prefix('#').unwrap_or(value.trim());
    let mut expanded = [0u8; 6];
    let bytes = hex.as_bytes();
    match bytes.len() {
        3 => {
            for i in 0..3 {
                expanded[i * 2] = bytes[i];
                expanded[i * 2 + 1] = bytes[i];
            }
        }
        6 => expanded.copy_from_slice(bytes),
        _ => return None,
    }

    Some(SkydimoRgb {
        r: parse_hex_byte(&expanded[0..2])?,
        g: parse_hex_byte(&expanded[2..4])?,
        b: parse_hex_byte(&expanded[4..6])?,
    })
}

fn parse_hex_byte(bytes: &[u8]) -> Option<u8> {
    Some((hex_value(*bytes.first()?)? << 4) | hex_value(*bytes.get(1)?)?)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn random_rgb_color(rng: &mut FastRng) -> SkydimoRgb {
    hsv_to_rgb(rng.next_unit() * 360.0, 1.0, 1.0)
}

struct FastRng {
    state: u64,
}

impl FastRng {
    fn seeded(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) * (1.0 / ((1u64 << 53) as f64))
    }
}

fn seed_now() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0xD1B5_4A32_D192_ED03);
    let stack_mix = (&nanos as *const u64 as usize) as u64;
    nanos ^ stack_mix.rotate_left(17)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_and_long_hex_colors() {
        assert_eq!(
            parse_hex_color("#F0A"),
            Some(SkydimoRgb {
                r: 255,
                g: 0,
                b: 170
            })
        );
        assert_eq!(
            parse_hex_color("0033FF"),
            Some(SkydimoRgb {
                r: 0,
                g: 51,
                b: 255
            })
        );
    }

    #[test]
    fn parses_effect_params() {
        let mut effect = VisorEffect::new();
        effect.update_params(
            r##"{"speed":75,"width":35,"random":true,"colors":["#00FF00","#112233"]}"##,
        );
        assert_eq!(effect.speed, 75.0);
        assert_eq!(effect.width_pct, 35.0);
        assert!(effect.random_enabled);
        assert_eq!(effect.user_c0, SkydimoRgb { r: 0, g: 255, b: 0 });
        assert_eq!(effect.user_c1, SkydimoRgb { r: 17, g: 34, b: 51 });
    }

    #[test]
    fn renders_non_black_after_progress_enters_strip() {
        let mut effect = VisorEffect::new();
        effect.width = 8;
        effect.height = 1;
        let mut pixels = [SkydimoRgb::default(); 8];

        effect.render(0.5, &mut pixels);

        assert!(pixels.iter().any(|pixel| *pixel != SkydimoRgb::default()));
    }
}
