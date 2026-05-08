mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const MIN_DELAY_TIME: f32 = 0.0;
const MAX_DELAY_TIME: f32 = 1.0;
const MIN_ON_TIME: f32 = 1.0;
const MAX_ON_TIME: f32 = 3.0;
const ON_RANGE_SELECTOR: f32 = 50.0;
const MIN_FADE_TIME: f32 = 1.0;
const MAX_FADE_TIME: f32 = 3.0;
const FADE_RANGE_SELECTOR: f32 = 50.0;

#[derive(Clone)]
struct Config {
    background: SkydimoRgb,
    background_scaled: SkydimoRgb,
    bg_brightness: f32,
    random_enabled: bool,
    density: f32,
    fade_in_speed: f32,
    fade_out_speed: f32,
    star_on_time: f32,
    palette: Vec<SkydimoRgb>,
}

impl Default for Config {
    fn default() -> Self {
        let mut config = Self {
            background: SkydimoRgb::default(),
            background_scaled: SkydimoRgb::default(),
            bg_brightness: 50.0,
            random_enabled: false,
            density: 50.0,
            fade_in_speed: 50.0,
            fade_out_speed: 50.0,
            star_on_time: 50.0,
            palette: vec![
                SkydimoRgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
                SkydimoRgb {
                    r: 136,
                    g: 204,
                    b: 255,
                },
                SkydimoRgb {
                    r: 255,
                    g: 204,
                    b: 68,
                },
            ],
        };
        config.rebuild_background();
        config
    }
}

impl Config {
    fn rebuild_background(&mut self) {
        let factor = self.bg_brightness.clamp(0.0, 100.0) / 100.0;
        self.background_scaled = SkydimoRgb {
            r: to_u8(self.background.r as f32 * factor),
            g: to_u8(self.background.g as f32 * factor),
            b: to_u8(self.background.b as f32 * factor),
        };
    }

    fn update_from_json(&mut self, json: &str) {
        if let Some(background) = parse_string_field(json, "background").and_then(hex_to_rgb) {
            self.background = background;
            self.rebuild_background();
        }
        if let Some(value) = parse_number_field(json, "bg_brightness") {
            self.bg_brightness = value;
            self.rebuild_background();
        }
        if let Some(value) = parse_bool_field(json, "random") {
            self.random_enabled = value;
        }
        if let Some(value) = parse_number_field(json, "density") {
            self.density = value.clamp(0.0, 100.0);
        }
        if let Some(value) = parse_number_field(json, "fade_in_speed") {
            self.fade_in_speed = value.clamp(1.0, 100.0);
        }
        if let Some(value) = parse_number_field(json, "fade_out_speed") {
            self.fade_out_speed = value.clamp(1.0, 100.0);
        }
        if let Some(value) = parse_number_field(json, "star_on_time") {
            self.star_on_time = value.clamp(1.0, 100.0);
        }
        if let Some(colors) = parse_color_array_field(json, "colors") {
            if !colors.is_empty() {
                self.palette = colors;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StarPhase {
    Delayed,
    FadeIn,
    On,
    FadeOut,
    Off,
}

struct Star {
    led: usize,
    phase: StarPhase,
    state_start: f32,
    period: f32,
    color: SkydimoRgb,
    current: SkydimoRgb,
}

struct StarryNightEffect {
    config: Config,
    stars: Vec<Star>,
    occupied: Vec<u8>,
    total_leds: usize,
    width: usize,
    height: usize,
    rng: XorShift64,
}

impl StarryNightEffect {
    fn new() -> Self {
        Self {
            config: Config::default(),
            stars: Vec::new(),
            occupied: Vec::new(),
            total_leds: 0,
            width: 0,
            height: 1,
            rng: XorShift64::seeded(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = width.max(1) as usize;
        self.height = height.max(1) as usize;
        let next_led_count = led_count as usize;
        if next_led_count != 0 && next_led_count != self.total_leds {
            self.reset_led_state(next_led_count);
        }
    }

    fn reset_led_state(&mut self, total_leds: usize) {
        self.stars.clear();
        self.occupied.clear();
        self.occupied.resize(total_leds, 0);
        self.total_leds = total_leds;
    }

    fn update_params(&mut self, json: &str) {
        self.config.update_from_json(json);
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        let led_count = pixels.len();
        if led_count == 0 {
            return;
        }

        if self.total_leds != led_count || self.occupied.len() != led_count {
            self.reset_led_state(led_count);
        }

        let elapsed = elapsed_seconds.max(0.0) as f32;
        self.sync_star_count(led_count, elapsed);
        self.advance_stars(elapsed);
        self.reactivate_finished_stars(led_count, elapsed);

        pixels.fill(self.config.background_scaled);
        for star in &self.stars {
            if star.led < led_count {
                pixels[star.led] = star.current;
            }
        }
    }

    fn sync_star_count(&mut self, led_count: usize, elapsed: f32) {
        let target_count = ((led_count as f32 * self.config.density / 100.0).ceil() as usize)
            .min(led_count);
        let current_count = self.stars.len();

        if target_count > current_count {
            self.stars.reserve(target_count - current_count);
            for slot in current_count..target_count {
                self.activate_star(slot, led_count, elapsed);
            }
        } else if target_count < current_count {
            for star in &self.stars[target_count..] {
                if star.led < self.occupied.len() {
                    self.occupied[star.led] = 0;
                }
            }
            self.stars.truncate(target_count);
        }
    }

    fn advance_stars(&mut self, elapsed: f32) {
        let background = self.config.background_scaled;
        let fade_in_speed = self.config.fade_in_speed;
        let fade_out_speed = self.config.fade_out_speed;
        let star_on_time = self.config.star_on_time;
        let rng = &mut self.rng;

        for star in &mut self.stars {
            let dt = elapsed - star.state_start;
            match star.phase {
                StarPhase::Delayed => {
                    star.current = background;
                    if dt >= star.period {
                        star.phase = StarPhase::FadeIn;
                        star.state_start = elapsed;
                        star.period = random_fade_period(rng, fade_in_speed);
                    }
                }
                StarPhase::FadeIn => {
                    if dt >= star.period {
                        star.phase = StarPhase::On;
                        star.state_start = elapsed;
                        star.period = random_on_period(rng, star_on_time);
                        star.current = star.color;
                    } else {
                        star.current = lerp_rgb(background, star.color, dt / star.period);
                    }
                }
                StarPhase::On => {
                    star.current = star.color;
                    if dt >= star.period {
                        star.phase = StarPhase::FadeOut;
                        star.state_start = elapsed;
                        star.period = random_fade_period(rng, fade_out_speed);
                    }
                }
                StarPhase::FadeOut => {
                    if dt >= star.period {
                        star.phase = StarPhase::Off;
                        star.current = background;
                    } else {
                        star.current = lerp_rgb(star.color, background, dt / star.period);
                    }
                }
                StarPhase::Off => {
                    star.current = background;
                }
            }
        }
    }

    fn reactivate_finished_stars(&mut self, led_count: usize, elapsed: f32) {
        let mut slot = 0usize;
        while slot < self.stars.len() {
            if self.stars[slot].phase == StarPhase::Off {
                let led = self.stars[slot].led;
                if led < self.occupied.len() {
                    self.occupied[led] = 0;
                }
                self.activate_star(slot, led_count, elapsed);
            }
            slot += 1;
        }
    }

    fn activate_star(&mut self, slot: usize, led_count: usize, elapsed: f32) {
        let previous_led = self.stars.get(slot).map(|star| star.led);
        if let Some(led) = previous_led.filter(|led| *led < self.occupied.len()) {
            self.occupied[led] = 0;
        }

        let Some(led) = self.find_free_led(led_count) else {
            if let Some(led) = previous_led.filter(|led| *led < self.occupied.len()) {
                self.occupied[led] = 1;
            }
            return;
        };

        let star = Star {
            led,
            phase: StarPhase::Delayed,
            state_start: elapsed,
            period: random_delay(&mut self.rng),
            color: self.pick_star_color(),
            current: self.config.background_scaled,
        };
        self.occupied[led] = 1;

        if slot < self.stars.len() {
            self.stars[slot] = star;
        } else {
            self.stars.push(star);
        }
    }

    fn find_free_led(&mut self, led_count: usize) -> Option<usize> {
        if led_count == 0 {
            return None;
        }

        for _ in 0..led_count {
            let idx = self.rng.next_usize(led_count);
            if self.occupied.get(idx).copied().unwrap_or(1) == 0 {
                return Some(idx);
            }
        }

        self.occupied
            .iter()
            .take(led_count)
            .position(|occupied| *occupied == 0)
    }

    fn pick_star_color(&mut self) -> SkydimoRgb {
        if self.config.random_enabled {
            return hsv_to_rgb(self.rng.next_f32() * 360.0, 1.0, 1.0);
        }

        if self.config.palette.is_empty() {
            return SkydimoRgb {
                r: 255,
                g: 255,
                b: 255,
            };
        }

        let idx = self.rng.next_usize(self.config.palette.len());
        self.config.palette[idx]
    }
}

unsafe extern "C" fn starry_night_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(StarryNightEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn starry_night_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<StarryNightEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn starry_night_resize(
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

unsafe extern "C" fn starry_night_update_params_json(
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

unsafe extern "C" fn starry_night_tick(
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

unsafe extern "C" fn starry_night_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer. The host must pass the ABI
/// version declared by this plugin manifest.
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
                    create: Some(starry_night_create),
                    destroy: Some(starry_night_destroy),
                    resize: Some(starry_night_resize),
                    update_params_json: Some(starry_night_update_params_json),
                    tick: Some(starry_night_tick),
                    is_ready: Some(starry_night_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut StarryNightEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<StarryNightEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn random_delay(rng: &mut XorShift64) -> f32 {
    MIN_DELAY_TIME + rng.next_f32() * (MAX_DELAY_TIME - MIN_DELAY_TIME)
}

fn random_fade_period(rng: &mut XorShift64, speed_value: f32) -> f32 {
    let base = MIN_FADE_TIME + rng.next_f32() * (MAX_FADE_TIME - MIN_FADE_TIME);
    base * speed_value / FADE_RANGE_SELECTOR
}

fn random_on_period(rng: &mut XorShift64, star_on_time: f32) -> f32 {
    let base = MIN_ON_TIME + rng.next_f32() * (MAX_ON_TIME - MIN_ON_TIME);
    base * star_on_time / ON_RANGE_SELECTOR
}

fn lerp_rgb(left: SkydimoRgb, right: SkydimoRgb, t: f32) -> SkydimoRgb {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    SkydimoRgb {
        r: to_u8(left.r as f32 * inv + right.r as f32 * t),
        g: to_u8(left.g as f32 * inv + right.g as f32 * t),
        b: to_u8(left.b as f32 * inv + right.b as f32 * t),
    }
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

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn parse_number_field(json: &str, key: &str) -> Option<f32> {
    let raw = json_value_after_key(json, key)?;
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
    raw[..end].trim().parse::<f32>().ok()
}

fn parse_bool_field(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_after_key(json, key)?;
    if raw.starts_with("true") || raw.starts_with("1") {
        Some(true)
    } else if raw.starts_with("false") || raw.starts_with("0") {
        Some(false)
    } else {
        None
    }
}

fn parse_string_field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let raw = json_value_after_key(json, key)?;
    let rest = raw.strip_prefix('"')?;
    let end = find_json_string_end(rest)?;
    Some(&rest[..end])
}

fn parse_color_array_field(json: &str, key: &str) -> Option<Vec<SkydimoRgb>> {
    let raw = json_value_after_key(json, key)?;
    let start = raw.find('[')?;
    let bytes = raw.as_bytes();
    let mut idx = start + 1;
    let mut colors = Vec::new();

    while idx < bytes.len() {
        match bytes[idx] {
            b']' => break,
            b'"' => {
                idx += 1;
                let start = idx;
                while idx < bytes.len() {
                    match bytes[idx] {
                        b'\\' => idx = idx.saturating_add(2),
                        b'"' => break,
                        _ => idx += 1,
                    }
                }
                if idx >= bytes.len() {
                    break;
                }
                if let Ok(raw_color) = std::str::from_utf8(&bytes[start..idx]) {
                    if let Some(color) = hex_to_rgb(raw_color) {
                        colors.push(color);
                    }
                }
                idx += 1;
            }
            _ => idx += 1,
        }
    }

    Some(colors)
}

fn json_value_after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    Some(after_key[colon + 1..].trim_start())
}

fn find_json_string_end(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'\\' => idx = idx.saturating_add(2),
            b'"' => return Some(idx),
            _ => idx += 1,
        }
    }
    None
}

fn hex_to_rgb(raw: &str) -> Option<SkydimoRgb> {
    let mut hex = raw.trim();
    if let Some(stripped) = hex.strip_prefix('#') {
        hex = stripped;
    }

    if hex.len() == 3 {
        let bytes = hex.as_bytes();
        return Some(SkydimoRgb {
            r: parse_hex_nibble(bytes[0])? * 17,
            g: parse_hex_nibble(bytes[1])? * 17,
            b: parse_hex_nibble(bytes[2])? * 17,
        });
    }

    if hex.len() != 6 {
        return None;
    }

    let bytes = hex.as_bytes();
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

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn seeded() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self {
            state: (nanos ^ 0xA076_1D64_78BD_642F).max(1),
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x.max(1);
        x
    }

    fn next_f32(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{hex_to_rgb, parse_color_array_field, Config, StarryNightEffect};
    use crate::abi::SkydimoRgb;

    #[test]
    fn parses_full_and_short_hex_colors() {
        assert_eq!(
            hex_to_rgb("#88CCFF"),
            Some(SkydimoRgb {
                r: 136,
                g: 204,
                b: 255
            })
        );
        assert_eq!(
            hex_to_rgb("#fc4"),
            Some(SkydimoRgb {
                r: 255,
                g: 204,
                b: 68
            })
        );
        assert_eq!(hex_to_rgb("nope"), None);
    }

    #[test]
    fn parses_multi_color_param() {
        let colors = parse_color_array_field(
            r##"{"colors":["#FFFFFF","#88CCFF","invalid","#FFCC44"]}"##,
            "colors",
        )
        .expect("colors field should parse");

        assert_eq!(colors.len(), 3);
        assert_eq!(colors[1], SkydimoRgb { r: 136, g: 204, b: 255 });
    }

    #[test]
    fn updates_config_from_params() {
        let mut config = Config::default();
        config.update_from_json(
            r##"{"background":"#204060","bg_brightness":25,"random":true,"density":75}"##,
        );

        assert_eq!(config.background_scaled, SkydimoRgb { r: 8, g: 16, b: 24 });
        assert!(config.random_enabled);
        assert_eq!(config.density, 75.0);
    }

    #[test]
    fn keeps_unique_star_leds() {
        let mut effect = StarryNightEffect::new();
        effect.config.density = 100.0;
        let mut pixels = vec![SkydimoRgb::default(); 32];
        effect.tick(0.0, &mut pixels);

        assert_eq!(effect.stars.len(), 32);
        assert!(effect.occupied.iter().all(|occupied| *occupied == 1));
    }
}
