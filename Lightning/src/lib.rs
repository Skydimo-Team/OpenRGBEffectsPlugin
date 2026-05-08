mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const FPS: f32 = 60.0;
const MODE_WHOLE_STRIP: u8 = 0;
const MODE_PER_LED: u8 = 1;

#[derive(Clone, Copy)]
struct UnitRgb {
    r: f32,
    g: f32,
    b: f32,
}

#[derive(Clone, Copy)]
struct LightningState {
    hue: f32,
    saturation: u8,
    value: u8,
    unit: UnitRgb,
}

impl LightningState {
    fn new(hue: f32, saturation: u8) -> Self {
        Self {
            hue,
            saturation,
            value: 0,
            unit: hsv_unit_rgb(hue, saturation),
        }
    }

    #[inline]
    fn set_hsv(&mut self, hue: f32, saturation: u8) {
        if self.hue == hue && self.saturation == saturation {
            return;
        }
        self.hue = hue;
        self.saturation = saturation;
        self.unit = hsv_unit_rgb(hue, saturation);
    }

    #[inline]
    fn color(self) -> SkydimoRgb {
        if self.value == 0 {
            SkydimoRgb::default()
        } else {
            unit_rgb_to_value(self.unit, self.value)
        }
    }
}

struct LightningEffect {
    speed: u32,
    decay: u32,
    mode: u8,
    random_enabled: bool,
    user_hue: f32,
    user_saturation: u8,
    user_value: u8,
    states: Vec<LightningState>,
    last_tick_time: Option<f64>,
    frame_remainder: f64,
    first_tick: bool,
    rng: XorShift64,
}

#[derive(Clone, Copy)]
struct StepParams {
    trigger_mod: u64,
    speed: u32,
    decrease: f32,
    random_enabled: bool,
    user_hue: f32,
    user_saturation: u8,
    user_value: u8,
}

impl LightningEffect {
    fn new() -> Self {
        Self::with_seed(seed_from_time())
    }

    fn with_seed(seed: u64) -> Self {
        let (user_hue, user_saturation, user_value) = rgb_to_hsv_255(SkydimoRgb {
            r: 255,
            g: 0,
            b: 0,
        });
        Self {
            speed: 20,
            decay: 10,
            mode: MODE_WHOLE_STRIP,
            random_enabled: false,
            user_hue,
            user_saturation,
            user_value,
            states: Vec::new(),
            last_tick_time: None,
            frame_remainder: 0.0,
            first_tick: true,
            rng: XorShift64::new(seed),
        }
    }

    fn update_params(&mut self, json: &str) {
        if let Some(speed) = json_number(json, "speed") {
            self.speed = rounded_u32(speed).clamp(1, 100);
        }
        if let Some(decay) = json_number(json, "decay") {
            self.decay = rounded_u32(decay).clamp(2, 60);
        }
        if let Some(mode) = json_number(json, "mode") {
            match rounded_u32(mode) as u8 {
                MODE_WHOLE_STRIP => self.mode = MODE_WHOLE_STRIP,
                MODE_PER_LED => self.mode = MODE_PER_LED,
                _ => {}
            }
        }
        if let Some(random_enabled) = json_bool(json, "random") {
            self.random_enabled = random_enabled;
        }
        if let Some(color) = json_string(json, "color") {
            if let Some(rgb) = parse_hex_color(color) {
                let (hue, saturation, value) = rgb_to_hsv_255(rgb);
                self.user_hue = hue;
                self.user_saturation = saturation;
                self.user_value = value;
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let state_count = match self.mode {
            MODE_PER_LED => pixels.len(),
            _ => 1,
        };
        self.sync_lightning_count(state_count);
        self.step_simulation(pixels.len(), elapsed_seconds);
        self.render(pixels);
    }

    fn sync_lightning_count(&mut self, count: usize) {
        if self.states.len() > count {
            self.states.truncate(count);
        }
        if self.states.len() < count {
            let user_hue = self.user_hue;
            let user_saturation = self.user_saturation;
            self.states
                .resize_with(count, || LightningState::new(user_hue, user_saturation));
        }
    }

    fn step_simulation(&mut self, led_count: usize, elapsed_seconds: f64) {
        let steps = if self.first_tick {
            self.first_tick = false;
            self.last_tick_time = Some(elapsed_seconds);
            self.frame_remainder = 0.0;
            1usize
        } else {
            let last = self.last_tick_time.unwrap_or(elapsed_seconds);
            let dt = (elapsed_seconds - last).max(0.0);
            self.last_tick_time = Some(elapsed_seconds);

            let frames = dt * FPS as f64 + self.frame_remainder;
            let steps = frames.floor() as usize;
            self.frame_remainder = frames - steps as f64;
            steps
        };

        if steps == 0 || led_count == 0 {
            return;
        }

        let params = StepParams {
            trigger_mod: if self.mode == MODE_WHOLE_STRIP {
                1000
            } else {
                (1000u64).saturating_mul(led_count as u64).max(1)
            },
            speed: self.speed,
            decrease: 1.0 + self.decay as f32 / FPS,
            random_enabled: self.random_enabled,
            user_hue: self.user_hue,
            user_saturation: self.user_saturation,
            user_value: self.user_value,
        };
        let rng = &mut self.rng;
        let states = &mut self.states;

        for _ in 0..steps {
            if self.mode == MODE_WHOLE_STRIP {
                if let Some(state) = states.first_mut() {
                    advance_lightning(state, rng, params);
                }
            } else {
                for state in states.iter_mut().take(led_count) {
                    advance_lightning(state, rng, params);
                }
            }
        }
    }

    fn render(&self, pixels: &mut [SkydimoRgb]) {
        if self.mode == MODE_WHOLE_STRIP {
            let color = self
                .states
                .first()
                .copied()
                .map(LightningState::color)
                .unwrap_or_default();
            fill_rgb(pixels, color);
            return;
        }

        for (pixel, state) in pixels.iter_mut().zip(self.states.iter().copied()) {
            *pixel = state.color();
        }
    }
}

fn advance_lightning(
    state: &mut LightningState,
    rng: &mut XorShift64,
    params: StepParams,
) {
    let triggered = rng.next_range_inclusive(params.trigger_mod) <= params.speed as u64;
    if triggered {
        state.value = if params.random_enabled {
            255
        } else {
            params.user_value
        };
    } else if state.value > 0 {
        state.value = (state.value as f32 / params.decrease)
            .floor()
            .clamp(0.0, 255.0) as u8;
    } else {
        state.value = 0;
    }

    if params.random_enabled {
        if state.value == 0 {
            state.set_hsv(rng.next_range_inclusive(360).saturating_sub(1) as f32, rng.next_u8(255));
        }
    } else {
        state.set_hsv(params.user_hue, params.user_saturation);
    }
}

unsafe extern "C" fn lightning_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(LightningEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn lightning_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<LightningEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn lightning_resize(
    instance: *mut c_void,
    _width: u32,
    _height: u32,
    _led_count: u32,
) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 0 })
}

unsafe extern "C" fn lightning_update_params_json(
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

unsafe extern "C" fn lightning_tick(
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

unsafe extern "C" fn lightning_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid, writable pointer to a host-compatible
/// `SkydimoPluginApiV1`. The host must pass the ABI version declared in
/// `manifest.json`.
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
                    create: Some(lightning_create),
                    destroy: Some(lightning_destroy),
                    resize: Some(lightning_resize),
                    update_params_json: Some(lightning_update_params_json),
                    tick: Some(lightning_tick),
                    is_ready: Some(lightning_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut LightningEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<LightningEffect>() })
    }
}

fn hsv_unit_rgb(h: f32, saturation: u8) -> UnitRgb {
    let h = h.rem_euclid(360.0);
    let s = (saturation as f32 / 255.0).clamp(0.0, 1.0);
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

    UnitRgb {
        r: r + m,
        g: g + m,
        b: b + m,
    }
}

fn unit_rgb_to_value(unit: UnitRgb, value: u8) -> SkydimoRgb {
    let value = value as f32;
    SkydimoRgb {
        r: to_u8(unit.r * value),
        g: to_u8(unit.g * value),
        b: to_u8(unit.b * value),
    }
}

fn rgb_to_hsv_255(rgb: SkydimoRgb) -> (f32, u8, u8) {
    let rf = rgb.r as f32 / 255.0;
    let gf = rgb.g as f32 / 255.0;
    let bf = rgb.b as f32 / 255.0;
    let maxc = rf.max(gf).max(bf);
    let minc = rf.min(gf).min(bf);
    let delta = maxc - minc;

    let hue = if delta == 0.0 {
        0.0
    } else if maxc == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if maxc == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let saturation = if maxc > 0.0 { delta / maxc * 255.0 } else { 0.0 };

    (
        ((hue + 0.5).floor() as u32 % 360) as f32,
        to_u8(saturation),
        to_u8(maxc * 255.0),
    )
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

fn json_number(json: &str, key: &str) -> Option<f32> {
    let mut value = json_value_after_key(json, key)?;
    if let Some(rest) = value.strip_prefix('"') {
        value = rest;
    }
    let end = value
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(value.len());
    value[..end].trim().parse::<f32>().ok()
}

fn json_bool(json: &str, key: &str) -> Option<bool> {
    let value = json_value_after_key(json, key)?;
    if value.starts_with("true") {
        Some(true)
    } else if value.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let value = json_value_after_key(json, key)?;
    let raw = value.strip_prefix('"')?;
    let end = json_string_end(raw)?;
    Some(&raw[..end])
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

fn fill_rgb(buffer: &mut [SkydimoRgb], color: SkydimoRgb) {
    if buffer.is_empty() {
        return;
    }

    buffer[0] = color;
    let mut filled = 1usize;
    while filled < buffer.len() {
        let copy_len = filled.min(buffer.len() - filled);
        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                buffer.as_mut_ptr().add(filled),
                copy_len,
            );
        }
        filled += copy_len;
    }
}

fn rounded_u32(value: f32) -> u32 {
    (value + 0.5).floor().clamp(0.0, u32::MAX as f32) as u32
}

fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn seed_from_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.max(1),
        }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x.max(1);
        x
    }

    #[inline]
    fn next_range_inclusive(&mut self, upper: u64) -> u64 {
        if upper == 0 {
            0
        } else {
            self.next_u64() % upper + 1
        }
    }

    #[inline]
    fn next_u8(&mut self, upper_exclusive: u8) -> u8 {
        if upper_exclusive == 0 {
            0
        } else {
            (self.next_u64() % upper_exclusive as u64) as u8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fill_rgb, json_bool, json_number, json_string, parse_hex_color, rgb_to_hsv_255,
        LightningEffect, SkydimoRgb, MODE_PER_LED,
    };

    #[test]
    fn parses_lightning_params() {
        let json = r##"{"speed":75,"decay":"15","mode":1,"random":true,"color":"#0af"}"##;
        assert_eq!(json_number(json, "speed"), Some(75.0));
        assert_eq!(json_number(json, "decay"), Some(15.0));
        assert_eq!(json_bool(json, "random"), Some(true));
        assert_eq!(json_string(json, "color"), Some("#0af"));
    }

    #[test]
    fn parses_hex_and_hsv_defaults() {
        let color = parse_hex_color("#0af").unwrap();
        assert_eq!(color, SkydimoRgb { r: 0, g: 170, b: 255 });

        let (hue, saturation, value) = rgb_to_hsv_255(SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(hue, 0.0);
        assert_eq!(saturation, 255);
        assert_eq!(value, 255);
    }

    #[test]
    fn fills_whole_buffer() {
        let color = SkydimoRgb { r: 3, g: 7, b: 11 };
        let mut buffer = [SkydimoRgb::default(); 19];
        fill_rgb(&mut buffer, color);
        assert!(buffer.iter().all(|pixel| *pixel == color));
    }

    #[test]
    fn per_led_mode_allocates_one_state_per_pixel() {
        let mut effect = LightningEffect::with_seed(1);
        effect.update_params(r#"{"mode":1,"speed":100}"#);
        let mut buffer = [SkydimoRgb::default(); 8];
        effect.tick(0.0, &mut buffer);

        assert_eq!(effect.mode, MODE_PER_LED);
        assert_eq!(effect.states.len(), buffer.len());
    }
}
