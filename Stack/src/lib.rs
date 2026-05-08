mod abi;

use std::ffi::{c_char, c_void};
use std::time::{SystemTime, UNIX_EPOCH};

use abi::{
    HostLogFn, SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1,
    SkydimoHostApiV1, SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION,
    SKYDIMO_PLUGIN_KIND_EFFECT,
};

const DEFAULT_SPEED: f32 = 10.0;
const DEFAULT_COLOR: SkydimoRgb = SkydimoRgb { r: 255, g: 0, b: 0 };

struct StackEffect {
    speed: f32,
    direction: u8,
    random_enabled: bool,
    user_color: SkydimoRgb,
    zone_color: SkydimoRgb,
    stop: usize,
    progress: f32,
    effective_count: usize,
    prev_width: usize,
    prev_height: usize,
    needs_reset: bool,
    last_elapsed: Option<f64>,
    width: usize,
    height: usize,
    rng: XorShift64,
    host_ctx: *mut c_void,
    log: Option<HostLogFn>,
}

impl StackEffect {
    fn new(host: *const SkydimoHostApiV1) -> Self {
        let (host_ctx, log) = unsafe {
            if host.is_null() {
                (std::ptr::null_mut(), None)
            } else {
                ((*host).host_ctx, (*host).log)
            }
        };

        Self {
            speed: DEFAULT_SPEED,
            direction: 0,
            random_enabled: false,
            user_color: DEFAULT_COLOR,
            zone_color: DEFAULT_COLOR,
            stop: 1,
            progress: 0.0,
            effective_count: 0,
            prev_width: 0,
            prev_height: 0,
            needs_reset: false,
            last_elapsed: None,
            width: 0,
            height: 1,
            rng: XorShift64::seeded(),
            host_ctx,
            log,
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = width as usize;
        self.height = height as usize;
        if self.width == 0 {
            self.width = led_count.max(1) as usize;
        }
        if self.height == 0 {
            self.height = 1;
        }
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        if let Some(speed) = json_number(bytes, b"speed") {
            if speed.is_finite() {
                self.speed = speed.clamp(1.0, 20.0);
            }
        }

        if let Some(direction) = json_number(bytes, b"direction") {
            let next = if direction >= 0.5 { 1 } else { 0 };
            if next != self.direction {
                self.direction = next;
                self.needs_reset = true;
            }
        }

        if let Some(random_enabled) = json_bool(bytes, b"random") {
            self.random_enabled = random_enabled;
        }

        if let Some(color) = json_string(bytes, b"color") {
            if let Some(rgb) = parse_hex_color(color) {
                self.user_color = rgb;
            }
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
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
        let is_matrix = height > 1;
        let count = if is_matrix && self.direction == 1 {
            height
        } else {
            width
        }
        .max(1);

        if width != self.prev_width
            || height != self.prev_height
            || self.needs_reset
            || self.effective_count != count
        {
            self.prev_width = width;
            self.prev_height = height;
            self.needs_reset = false;
            self.reset_state(count);
        }

        let dt = match self.last_elapsed {
            Some(last) if elapsed_seconds >= last => (elapsed_seconds - last) as f32,
            _ => 0.0,
        };
        self.last_elapsed = Some(elapsed_seconds);

        self.render(pixels, width, height, is_matrix);
        self.advance(dt);
    }

    fn reset_state(&mut self, count: usize) {
        self.effective_count = count.max(1);
        self.stop = self.effective_count.saturating_sub(1).max(1);
        self.progress = 0.0;
        self.pick_zone_color();
    }

    fn pick_zone_color(&mut self) {
        self.zone_color = if self.random_enabled {
            hsv_to_rgb(self.rng.next_f32() * 360.0, 1.0, 1.0)
        } else {
            self.user_color
        };
    }

    fn render(&self, pixels: &mut [SkydimoRgb], width: usize, height: usize, is_matrix: bool) {
        if is_matrix {
            if self.direction == 0 {
                self.render_horizontal_matrix(pixels, width, height);
            } else {
                self.render_vertical_matrix(pixels, width, height);
            }
        } else {
            self.render_linear(pixels);
        }
    }

    fn render_linear(&self, pixels: &mut [SkydimoRgb]) {
        for (idx, pixel) in pixels.iter_mut().enumerate() {
            *pixel = self.color_at(idx);
        }
    }

    fn render_horizontal_matrix(&self, pixels: &mut [SkydimoRgb], width: usize, height: usize) {
        let mut index = 0usize;
        for _row in 0..height {
            for col in 0..width {
                if index >= pixels.len() {
                    return;
                }
                pixels[index] = self.color_at(col);
                index += 1;
            }
        }
        pixels[index..].fill(SkydimoRgb::default());
    }

    fn render_vertical_matrix(&self, pixels: &mut [SkydimoRgb], width: usize, height: usize) {
        let mut index = 0usize;
        for row in 0..height {
            let color = self.color_at(row);
            for _col in 0..width {
                if index >= pixels.len() {
                    return;
                }
                pixels[index] = color;
                index += 1;
            }
        }
        pixels[index..].fill(SkydimoRgb::default());
    }

    fn color_at(&self, axis_index: usize) -> SkydimoRgb {
        if self.stop < axis_index {
            return self.zone_color;
        }

        let distance = (self.progress - axis_index as f32).abs();
        if distance > 1.0 {
            return SkydimoRgb::default();
        }

        let factor = 1.0 - distance;
        SkydimoRgb {
            r: scale_channel(self.zone_color.r, factor),
            g: scale_channel(self.zone_color.g, factor),
            b: scale_channel(self.zone_color.b, factor),
        }
    }

    fn advance(&mut self, dt: f32) {
        if !(dt > 0.0 && dt <= 0.5) {
            return;
        }

        self.progress += 0.1 * self.speed * self.effective_count as f32 * dt;
        if self.progress < self.stop as f32 {
            return;
        }

        self.stop = self.stop.saturating_sub(1);
        if self.stop == 0 {
            self.reset_state(self.effective_count);
        } else {
            self.progress = 0.0;
        }
    }

    #[allow(dead_code)]
    fn log(&self, level: u32, message: &str) {
        if let Some(log) = self.log {
            unsafe {
                log(
                    self.host_ctx,
                    level,
                    message.as_ptr().cast::<c_char>(),
                    message.len(),
                );
            }
        }
    }
}

unsafe extern "C" fn stack_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    if out_instance.is_null() {
        return -1;
    }

    let effect = Box::new(StackEffect::new(host));
    unsafe {
        *out_instance = Box::into_raw(effect).cast::<c_void>();
    }
    0
}

unsafe extern "C" fn stack_destroy(instance: *mut c_void) {
    if !instance.is_null() {
        unsafe {
            drop(Box::from_raw(instance.cast::<StackEffect>()));
        }
    }
}

unsafe extern "C" fn stack_resize(
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

unsafe extern "C" fn stack_update_params_json(
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
    effect.update_params_json(bytes);
    0
}

unsafe extern "C" fn stack_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    let Some(effect) = effect_mut(instance) else {
        return -1;
    };
    if buffer.is_null() && len > 0 {
        return -1;
    }
    if len == 0 {
        return 0;
    }

    let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
    effect.tick(elapsed_seconds, pixels);
    0
}

unsafe extern "C" fn stack_is_ready(instance: *mut c_void) -> i32 {
    if instance.is_null() {
        -1
    } else {
        1
    }
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
                create: Some(stack_create),
                destroy: Some(stack_destroy),
                resize: Some(stack_resize),
                update_params_json: Some(stack_update_params_json),
                tick: Some(stack_tick),
                is_ready: Some(stack_is_ready),
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

unsafe fn effect_mut(instance: *mut c_void) -> Option<&'static mut StackEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<StackEffect>() })
    }
}

fn scale_channel(channel: u8, factor: f32) -> u8 {
    if factor <= 0.0 {
        0
    } else if factor >= 1.0 {
        channel
    } else {
        (channel as f32).mul_add(factor, 0.5).floor() as u8
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

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f32> {
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

fn json_bool(bytes: &[u8], key: &[u8]) -> Option<bool> {
    let start = json_value_start(bytes, key)?;
    if bytes[start..].starts_with(b"true") {
        Some(true)
    } else if bytes[start..].starts_with(b"false") {
        Some(false)
    } else {
        None
    }
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
            state: nanos ^ 0xA076_1D64_78BD_642F,
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

    fn next_f32(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::{
        json_bool, json_number, json_string, parse_hex_color, scale_channel,
        skydimo_plugin_get_api, StackEffect,
    };
    use crate::abi::{
        SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_stack_params() {
        let json = br##"{"speed":12,"direction":1,"random":true,"color":"#0af"}"##;
        assert_eq!(json_number(json, b"speed"), Some(12.0));
        assert_eq!(json_number(json, b"direction"), Some(1.0));
        assert_eq!(json_bool(json, b"random"), Some(true));
        assert_eq!(json_string(json, b"color"), Some(&b"#0af"[..]));
    }

    #[test]
    fn parses_hex_colors() {
        let full = parse_hex_color(b"#FF8000").unwrap();
        assert_eq!((full.r, full.g, full.b), (255, 128, 0));

        let short = parse_hex_color(b"#0af").unwrap();
        assert_eq!((short.r, short.g, short.b), (0, 170, 255));
    }

    #[test]
    fn channel_scaling_matches_lua_rounding() {
        assert_eq!(scale_channel(255, 0.0), 0);
        assert_eq!(scale_channel(255, 1.0), 255);
        assert_eq!(scale_channel(255, 0.5), 128);
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
        assert_eq!(api.kind_mask, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }

    #[test]
    fn renders_initial_linear_stack_head() {
        let mut effect = StackEffect::new(std::ptr::null());
        effect.resize(4, 1, 4);
        let mut pixels = [SkydimoRgb::default(); 4];

        effect.tick(0.0, &mut pixels);

        assert_eq!(pixels[0], SkydimoRgb { r: 255, g: 0, b: 0 });
        assert_eq!(pixels[1], SkydimoRgb::default());
        assert_eq!(pixels[2], SkydimoRgb::default());
        assert_eq!(pixels[3], SkydimoRgb::default());
    }

    #[test]
    fn ffi_create_tick_destroy_round_trip() {
        let mut instance: *mut c_void = std::ptr::null_mut();
        let create = unsafe {
            let mut api = SkydimoPluginApiV1::default();
            assert_eq!(
                skydimo_plugin_get_api(
                    SKYDIMO_NATIVE_C_ABI_VERSION,
                    std::ptr::null(),
                    &mut api,
                ),
                0
            );
            api.effect.create.unwrap()
        };

        assert_eq!(unsafe { create(std::ptr::null(), &mut instance) }, 0);
        assert!(!instance.is_null());

        let mut pixels = [SkydimoRgb::default(); 4];
        let mut api = SkydimoPluginApiV1::default();
        unsafe {
            assert_eq!(
                skydimo_plugin_get_api(
                    SKYDIMO_NATIVE_C_ABI_VERSION,
                    std::ptr::null(),
                    &mut api,
                ),
                0
            );
            assert_eq!(api.effect.resize.unwrap()(instance, 4, 1, 4), 0);
            assert_eq!(
                api.effect
                    .tick
                    .unwrap()(instance, 0.0, pixels.as_mut_ptr(), pixels.len()),
                0
            );
            api.effect.destroy.unwrap()(instance);
        }

        assert_eq!(pixels[0], SkydimoRgb { r: 255, g: 0, b: 0 });
    }
}
