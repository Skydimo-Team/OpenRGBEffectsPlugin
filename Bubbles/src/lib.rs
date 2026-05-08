use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;
const FPS: f32 = 60.0;
const DEFAULT_BACKGROUND: SkydimoRgb = SkydimoRgb { r: 0, g: 0, b: 0 };
const FALLBACK_COLOR: SkydimoRgb = SkydimoRgb {
    r: 255,
    g: 255,
    b: 255,
};
const DEFAULT_PALETTE: [SkydimoRgb; 5] = [
    SkydimoRgb { r: 255, g: 0, b: 0 },
    SkydimoRgb {
        r: 255,
        g: 153,
        b: 0,
    },
    SkydimoRgb {
        r: 255,
        g: 255,
        b: 0,
    },
    SkydimoRgb {
        r: 0,
        g: 255,
        b: 136,
    },
    SkydimoRgb {
        r: 0,
        g: 170,
        b: 255,
    },
];

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
pub struct SkydimoLedColorV1 {
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

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Hsv {
    h: f32,
    s: f32,
}

#[derive(Clone, Copy)]
struct BubblesConfig {
    max_bubbles: usize,
    rarity: u32,
    speed: f32,
    max_expansion: f32,
    thickness: f32,
    background: SkydimoRgb,
}

impl Default for BubblesConfig {
    fn default() -> Self {
        Self {
            max_bubbles: 10,
            rarity: 50,
            speed: 1.0,
            max_expansion: 100.0,
            thickness: 10.0,
            background: DEFAULT_BACKGROUND,
        }
    }
}

#[derive(Clone, Copy)]
struct Bubble {
    expansion: f32,
    speed: f32,
    cx: f32,
    cy: f32,
    hsv: Hsv,
}

struct BubblesEffect {
    config: BubblesConfig,
    palette: Vec<Hsv>,
    bubbles: Vec<Bubble>,
    rng: XorShift64,
    width: usize,
    height: usize,
    previous_elapsed: Option<f32>,
}

impl BubblesEffect {
    fn new() -> Self {
        let mut effect = Self {
            config: BubblesConfig::default(),
            palette: Vec::with_capacity(10),
            bubbles: Vec::with_capacity(20),
            rng: XorShift64::seeded(),
            width: 0,
            height: 1,
            previous_elapsed: None,
        };
        effect.set_palette(DEFAULT_PALETTE.iter().copied());
        effect
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        let fallback_width = (led_count as usize).max(1);
        self.width = if width == 0 {
            fallback_width
        } else {
            width as usize
        };
        self.height = height.max(1) as usize;
    }

    fn update_params_json(&mut self, bytes: &[u8]) {
        if let Some(max_bubbles) = json_number(bytes, b"max_bubbles") {
            self.config.max_bubbles = rounded_usize(max_bubbles).clamp(1, 20);
            if self.bubbles.len() > self.config.max_bubbles {
                self.bubbles.truncate(self.config.max_bubbles);
            }
        }
        if let Some(rarity) = json_number(bytes, b"rarity") {
            self.config.rarity = rounded_u32(rarity).clamp(1, 1000);
        }
        if let Some(speed) = json_number(bytes, b"speed") {
            self.config.speed = speed.clamp(1.0, 100.0);
        }
        if let Some(max_expansion) = json_number(bytes, b"max_expansion") {
            self.config.max_expansion = rounded_f32(max_expansion).clamp(1.0, 500.0);
            self.cleanup_bubbles();
        }
        if let Some(thickness) = json_number(bytes, b"thickness") {
            self.config.thickness = rounded_f32(thickness).clamp(1.0, 50.0);
        }
        if let Some(background) = json_color(bytes, b"background") {
            self.config.background = background;
        }

        let mut palette = Vec::new();
        if json_color_array(bytes, b"colors", &mut palette) && !palette.is_empty() {
            self.set_palette(palette.into_iter());
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) {
        if pixels.is_empty() {
            return;
        }

        let frame_steps = self.advance(elapsed_seconds as f32);
        self.spawn_bubbles(frame_steps);
        self.cleanup_bubbles();
        self.render(pixels);
    }

    fn advance(&mut self, elapsed_seconds: f32) -> f32 {
        let dt = match self.previous_elapsed {
            None => 1.0 / FPS,
            Some(previous) if elapsed_seconds.is_finite() && elapsed_seconds > previous => {
                elapsed_seconds - previous
            }
            Some(_) => 1.0 / FPS,
        };
        self.previous_elapsed = elapsed_seconds.is_finite().then_some(elapsed_seconds.max(0.0));

        let frame_steps = dt.max(0.0) * FPS;
        let expansion_step = 0.2 * self.config.speed / FPS * frame_steps;
        for bubble in &mut self.bubbles {
            bubble.expansion += expansion_step * bubble.speed;
        }
        frame_steps
    }

    fn spawn_bubbles(&mut self, frame_steps: f32) {
        let rolls = (frame_steps + 0.5).floor().max(1.0) as usize;

        for _ in 0..rolls {
            if self.bubbles.len() >= self.config.max_bubbles {
                break;
            }
            if self.rng.one_in(self.config.rarity) {
                self.init_bubble();
            }
        }
    }

    fn init_bubble(&mut self) {
        if self.palette.is_empty() {
            self.palette.push(rgb_to_hs(FALLBACK_COLOR));
        }
        let color_index = self.rng.range_usize(self.palette.len());
        self.bubbles.push(Bubble {
            expansion: 0.0,
            speed: 1.0 + 10.0 * self.rng.next_f32(),
            cx: self.rng.next_f32(),
            cy: self.rng.next_f32(),
            hsv: self.palette[color_index],
        });
    }

    fn cleanup_bubbles(&mut self) {
        let max_expansion = self.config.max_expansion;
        self.bubbles
            .retain(|bubble| bubble.expansion <= max_expansion);
    }

    fn render(&self, pixels: &mut [SkydimoRgb]) {
        let width = if self.width == 0 {
            pixels.len().max(1)
        } else {
            self.width.max(1)
        };
        let height = self.height.max(1);
        let mut index = 0usize;

        for y in 0..height {
            for x in 0..width {
                if index >= pixels.len() {
                    return;
                }
                pixels[index] = self.color_at(x as f32, y as f32, width as f32, height as f32);
                index += 1;
            }
        }

        if index < pixels.len() {
            pixels[index..].fill(self.config.background);
        }
    }

    #[inline]
    fn color_at(&self, x: f32, y: f32, width: f32, height: f32) -> SkydimoRgb {
        if self.bubbles.is_empty() {
            return self.config.background;
        }

        let mut best_value = 0.0f32;
        let mut best_hsv = Hsv::default();
        let denom = 0.1 * self.config.thickness;

        for bubble in &self.bubbles {
            let dx = width.mul_add(bubble.cx, -x);
            let dy = height.mul_add(bubble.cy, -y);
            let distance = dx.mul_add(dx, dy * dy).sqrt();
            let shallow = (distance - bubble.expansion).abs() / denom;
            let value = if shallow < 0.001 {
                255.0
            } else {
                (255.0 / (shallow * shallow)).min(255.0)
            };

            if value > best_value {
                best_value = value;
                best_hsv = bubble.hsv;
            }
        }

        let color = hsv_to_rgb(best_hsv.h, best_hsv.s, best_value / 255.0);
        screen_blend(color, self.config.background)
    }

    fn set_palette(&mut self, colors: impl Iterator<Item = SkydimoRgb>) {
        self.palette.clear();
        self.palette.extend(colors.map(rgb_to_hs));
        if self.palette.is_empty() {
            self.palette.push(rgb_to_hs(FALLBACK_COLOR));
        }
    }
}

unsafe extern "C" fn bubbles_create(
    _host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }

        let effect = Box::new(BubblesEffect::new());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn bubbles_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<BubblesEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn bubbles_resize(
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

unsafe extern "C" fn bubbles_update_params_json(
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
        effect.update_params_json(bytes);
        0
    })
}

unsafe extern "C" fn bubbles_tick(
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

unsafe extern "C" fn bubbles_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must point to writable storage for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must be the native-c ABI declared in manifest.json.
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
                    create: Some(bubbles_create),
                    destroy: Some(bubbles_destroy),
                    resize: Some(bubbles_resize),
                    update_params_json: Some(bubbles_update_params_json),
                    tick: Some(bubbles_tick),
                    is_ready: Some(bubbles_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut BubblesEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<BubblesEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

fn json_number(bytes: &[u8], key: &[u8]) -> Option<f32> {
    let mut start = json_value_start(bytes, key)?;
    let quoted = bytes.get(start).copied() == Some(b'"');
    if quoted {
        start += 1;
    }

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
    if quoted && bytes.get(end).copied() != Some(b'"') {
        return None;
    }

    std::str::from_utf8(&bytes[start..end]).ok()?.parse().ok()
}

fn json_color(bytes: &[u8], key: &[u8]) -> Option<SkydimoRgb> {
    let start = json_value_start(bytes, key)?;
    if bytes.get(start).copied() != Some(b'"') {
        return Some(FALLBACK_COLOR);
    }
    let (raw, _) = json_string_at(bytes, start + 1)?;
    Some(parse_hex_color(raw).unwrap_or(FALLBACK_COLOR))
}

fn json_color_array(bytes: &[u8], key: &[u8], out: &mut Vec<SkydimoRgb>) -> bool {
    let Some(mut pos) = json_value_start(bytes, key) else {
        return false;
    };
    if bytes.get(pos).copied() != Some(b'[') {
        return false;
    }
    pos += 1;

    loop {
        pos = skip_ascii_ws(bytes, pos);
        match bytes.get(pos).copied() {
            Some(b']') => return true,
            Some(b',') => {
                pos += 1;
            }
            Some(b'"') => {
                let Some((value, next_pos)) = json_string_at(bytes, pos + 1) else {
                    return true;
                };
                out.push(parse_hex_color(value).unwrap_or(FALLBACK_COLOR));
                pos = next_pos;
            }
            Some(_) => {
                let Some(next_pos) = find_next_array_separator(bytes, pos) else {
                    return true;
                };
                pos = next_pos;
            }
            None => return true,
        }
    }
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

fn json_string_at(bytes: &[u8], mut pos: usize) -> Option<(&[u8], usize)> {
    let start = pos;
    let mut escaped = false;
    while pos < bytes.len() {
        let byte = bytes[pos];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Some((&bytes[start..pos], pos + 1));
        }
        pos += 1;
    }
    None
}

fn find_next_array_separator(bytes: &[u8], mut pos: usize) -> Option<usize> {
    while pos < bytes.len() {
        if matches!(bytes[pos], b',' | b']') {
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

fn parse_hex_color(raw: &[u8]) -> Option<SkydimoRgb> {
    let mut digits = [0u8; 6];
    let mut len = 0usize;

    for &byte in raw {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'#' && len == 0 {
            continue;
        }
        if len == digits.len() {
            return None;
        }
        digits[len] = byte;
        len += 1;
    }

    match len {
        3 => Some(SkydimoRgb {
            r: parse_hex_nibble(digits[0])? * 17,
            g: parse_hex_nibble(digits[1])? * 17,
            b: parse_hex_nibble(digits[2])? * 17,
        }),
        6 => Some(SkydimoRgb {
            r: parse_hex_byte(digits[0], digits[1])?,
            g: parse_hex_byte(digits[2], digits[3])?,
            b: parse_hex_byte(digits[4], digits[5])?,
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

fn rounded_f32(value: f32) -> f32 {
    (value + 0.5).floor()
}

fn rounded_u32(value: f32) -> u32 {
    rounded_f32(value).max(0.0) as u32
}

fn rounded_usize(value: f32) -> usize {
    rounded_f32(value).max(0.0) as usize
}

fn rgb_to_hs(color: SkydimoRgb) -> Hsv {
    let r = color.r as f32 / 255.0;
    let g = color.g as f32 / 255.0;
    let b = color.b as f32 / 255.0;
    let max_channel = r.max(g).max(b);
    let min_channel = r.min(g).min(b);
    let delta = max_channel - min_channel;

    if delta <= 0.0 {
        return Hsv { h: 0.0, s: 0.0 };
    }

    let h = if max_channel == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max_channel == g {
        60.0 * (((b - r) / delta) + 2.0)
    } else {
        60.0 * (((r - g) / delta) + 4.0)
    };
    let s = if max_channel > 0.0 {
        delta / max_channel
    } else {
        0.0
    };

    Hsv { h, s }
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

fn screen_blend(color: SkydimoRgb, background: SkydimoRgb) -> SkydimoRgb {
    SkydimoRgb {
        r: screen_blend_channel(color.r, background.r),
        g: screen_blend_channel(color.g, background.g),
        b: screen_blend_channel(color.b, background.b),
    }
}

fn screen_blend_channel(a: u8, b: u8) -> u8 {
    let a = u32::from(a);
    let b = u32::from(b);
    ((((a + b) * 255).saturating_sub(a * b) + 127) / 255).min(255) as u8
}

fn to_u8(value: f32) -> u8 {
    (value + 0.5).floor().clamp(0.0, 255.0) as u8
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
        Self::new(nanos ^ 0xA076_1D64_78BD_642F)
    }

    fn new(seed: u64) -> Self {
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
        self.state = x.max(1);
        x
    }

    fn next_f32(&mut self) -> f32 {
        let value = (self.next_u64() >> 40) as u32;
        value as f32 / 16_777_216.0
    }

    fn one_in(&mut self, rarity: u32) -> bool {
        self.next_u64().is_multiple_of(u64::from(rarity.max(1)))
    }

    fn range_usize(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            (self.next_u64() % upper_exclusive as u64) as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{c_char, c_void};

    use super::{
        parse_hex_color, screen_blend_channel, skydimo_plugin_get_api, BubblesEffect, Hsv,
        SkydimoPluginApiV1, SkydimoRgb, XorShift64, SKYDIMO_NATIVE_C_ABI_VERSION,
        SKYDIMO_PLUGIN_KIND_EFFECT,
    };

    #[test]
    fn parses_params_without_allocating_json_values() {
        let mut effect = BubblesEffect::new();
        effect.update_params_json(
            br##"{"background":"#010203","colors":[" #0 a f ","#102030"],"speed":24.6,"rarity":"7","max_bubbles":3,"max_expansion":44.4,"thickness":5.2}"##,
        );

        assert_eq!(
            effect.config.background,
            SkydimoRgb { r: 1, g: 2, b: 3 }
        );
        assert_eq!(effect.palette.len(), 2);
        assert_eq!(effect.config.speed, 24.6);
        assert_eq!(effect.config.rarity, 7);
        assert_eq!(effect.config.max_bubbles, 3);
        assert_eq!(effect.config.max_expansion, 44.0);
        assert_eq!(effect.config.thickness, 5.0);
    }

    #[test]
    fn renders_a_bubble_directly_into_host_buffer() {
        let mut effect = BubblesEffect::new();
        effect.width = 4;
        effect.height = 1;
        effect.config.background = SkydimoRgb { r: 1, g: 2, b: 3 };
        effect.bubbles.push(super::Bubble {
            expansion: 0.0,
            speed: 1.0,
            cx: 0.0,
            cy: 0.0,
            hsv: Hsv { h: 0.0, s: 1.0 },
        });

        let mut pixels = [SkydimoRgb::default(); 4];
        effect.render(&mut pixels);

        assert!(pixels[0].r > pixels[0].g);
        assert_eq!(pixels[0].b, 3);
    }

    #[test]
    fn rarity_one_spawns_deterministically() {
        let mut effect = BubblesEffect::new();
        effect.config.rarity = 1;
        effect.config.max_bubbles = 1;
        effect.rng = XorShift64::new(1);

        let mut pixels = [SkydimoRgb::default(); 8];
        effect.tick(0.0, &mut pixels);

        assert_eq!(effect.bubbles.len(), 1);
        assert!(pixels.iter().any(|pixel| pixel.r > 0 || pixel.g > 0 || pixel.b > 0));
    }

    #[test]
    fn parses_short_hex_and_internal_whitespace_like_lua() {
        assert_eq!(
            parse_hex_color(b" #0 a f "),
            Some(SkydimoRgb {
                r: 0,
                g: 170,
                b: 255
            })
        );
        assert_eq!(parse_hex_color(b"#xyz"), None);
    }

    #[test]
    fn screen_blend_uses_lua_rounding_behavior() {
        assert_eq!(screen_blend_channel(0, 1), 1);
        assert_eq!(screen_blend_channel(128, 128), 192);
        assert_eq!(screen_blend_channel(255, 12), 255);
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
    fn ffi_create_tick_destroy_round_trip() {
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
        }

        let mut instance: *mut c_void = std::ptr::null_mut();
        unsafe {
            assert_eq!(api.effect.create.unwrap()(std::ptr::null(), &mut instance), 0);
            assert!(!instance.is_null());
            assert_eq!(api.effect.resize.unwrap()(instance, 8, 1, 8), 0);
            assert_eq!(
                api.effect.update_params_json.unwrap()(
                    instance,
                    br#"{"rarity":1}"#.as_ptr().cast::<c_char>(),
                    br#"{"rarity":1}"#.len(),
                ),
                0
            );

            let mut pixels = [SkydimoRgb::default(); 8];
            assert_eq!(
                api.effect
                    .tick
                    .unwrap()(instance, 0.0, pixels.as_mut_ptr(), pixels.len()),
                0
            );
            assert_eq!(api.effect.is_ready.unwrap()(instance), 1);
            api.effect.destroy.unwrap()(instance);
        }
    }
}
