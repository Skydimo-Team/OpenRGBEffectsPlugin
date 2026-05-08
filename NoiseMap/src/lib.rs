mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    SkydimoControllerApiV1, SkydimoEffectApiV1, SkydimoExtensionApiV1, SkydimoHostApiV1,
    SkydimoPluginApiV1, SkydimoRgb, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const REFERENCE_FPS: f64 = 60.0;
const FRAME_DT: f64 = 1.0 / REFERENCE_FPS;
const MAX_COLORS: usize = 8;
const GRADIENT_SAMPLES: usize = 101;
const MODE_RAINBOW: u32 = 0;
const MODE_INVERSE_RAINBOW: u32 = 1;
const MODE_CUSTOM: u32 = 2;
const MOTION_UP: u32 = 0;
const MOTION_DOWN: u32 = 1;
const MOTION_LEFT: u32 = 2;
const MOTION_RIGHT: u32 = 3;
const BLACK: SkydimoRgb = SkydimoRgb { r: 0, g: 0, b: 0 };

const PRESET_LAVA: [SkydimoRgb; 3] = [
    SkydimoRgb {
        r: 0xFF,
        g: 0x55,
        b: 0x00,
    },
    SkydimoRgb {
        r: 0xFF,
        g: 0xC8,
        b: 0x00,
    },
    SkydimoRgb {
        r: 0xC8,
        g: 0x00,
        b: 0x00,
    },
];
const PRESET_BOREALIS: [SkydimoRgb; 5] = [
    SkydimoRgb {
        r: 0x14,
        g: 0xE8,
        b: 0x1E,
    },
    SkydimoRgb {
        r: 0x00,
        g: 0xEA,
        b: 0x8D,
    },
    SkydimoRgb {
        r: 0x01,
        g: 0x7E,
        b: 0xD5,
    },
    SkydimoRgb {
        r: 0xB5,
        g: 0x3D,
        b: 0xFF,
    },
    SkydimoRgb {
        r: 0x8D,
        g: 0x00,
        b: 0xC4,
    },
];
const PRESET_OCEAN: [SkydimoRgb; 4] = [
    SkydimoRgb {
        r: 0x00,
        g: 0x00,
        b: 0x7F,
    },
    SkydimoRgb {
        r: 0x00,
        g: 0x00,
        b: 0xFF,
    },
    SkydimoRgb {
        r: 0x00,
        g: 0xFF,
        b: 0xFF,
    },
    SkydimoRgb {
        r: 0x00,
        g: 0xAA,
        b: 0xFF,
    },
];
const PRESET_CHEMICALS: [SkydimoRgb; 5] = [
    SkydimoRgb {
        r: 0x93,
        g: 0x46,
        b: 0xFF,
    },
    SkydimoRgb {
        r: 0x88,
        g: 0x68,
        b: 0xB5,
    },
    SkydimoRgb {
        r: 0x7A,
        g: 0xFC,
        b: 0x94,
    },
    SkydimoRgb {
        r: 0x29,
        g: 0xFF,
        b: 0x48,
    },
    SkydimoRgb {
        r: 0x4B,
        g: 0xFF,
        b: 0x00,
    },
];
static PRESETS: [&[SkydimoRgb]; 4] = [
    &PRESET_LAVA,
    &PRESET_BOREALIS,
    &PRESET_OCEAN,
    &PRESET_CHEMICALS,
];

const PERM: [u8; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, 140, 36, 103, 30,
    69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, 247, 120, 234, 75, 0, 26, 197, 62, 94,
    252, 219, 203, 117, 35, 11, 32, 57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136,
    171, 168, 68, 175, 74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229,
    122, 60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, 65, 25, 63,
    161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169, 200, 196, 135, 130, 116, 188,
    159, 86, 164, 100, 109, 198, 173, 186, 3, 64, 52, 217, 226, 250, 124, 123, 5, 202, 38,
    147, 118, 126, 255, 82, 85, 212, 207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42,
    223, 183, 170, 213, 119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172,
    9, 129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, 218, 246,
    97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241, 81, 51, 145, 235,
    249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157, 184, 84, 204, 176, 115, 121,
    50, 45, 127, 4, 150, 254, 138, 236, 205, 93, 222, 114, 67, 29, 24, 72, 243, 141, 128,
    195, 78, 66, 215, 61, 156, 180,
];

#[derive(Clone, Copy)]
struct Params {
    speed: u32,
    frequency: f64,
    amplitude: f64,
    lacunarity: f64,
    persistence: f64,
    octaves: u32,
    motion: u32,
    motion_speed: u32,
    colors_choice: u32,
    preset: usize,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            speed: 50,
            frequency: 0.12,
            amplitude: 3.9,
            lacunarity: 0.75,
            persistence: 0.5,
            octaves: 2,
            motion: MOTION_UP,
            motion_speed: 0,
            colors_choice: MODE_RAINBOW,
            preset: 0,
        }
    }
}

#[derive(Clone, Copy)]
struct SimplexNoise {
    frequency: f64,
    amplitude: f64,
    lacunarity: f64,
    persistence: f64,
}

impl SimplexNoise {
    fn from_params(params: &Params) -> Self {
        Self {
            frequency: params.frequency,
            amplitude: params.amplitude,
            lacunarity: params.lacunarity,
            persistence: params.persistence,
        }
    }

    #[inline(always)]
    fn fractal(self, octaves: u32, x: f64, y: f64, z: f64) -> f64 {
        let mut output = 0.0;
        let mut denom = 0.0;
        let mut frequency = self.frequency;
        let mut amplitude = self.amplitude;

        for _ in 0..octaves.max(1) {
            output += amplitude * noise3(x * frequency, y * frequency, z * frequency);
            denom += amplitude;
            frequency *= self.lacunarity;
            amplitude *= self.persistence;
        }

        if denom == 0.0 {
            0.0
        } else {
            output / denom
        }
    }
}

struct NoiseMapEffect {
    params: Params,
    noise: SimplexNoise,
    progress: f64,
    last_elapsed: Option<f64>,
    time_carry: f64,
    width: usize,
    height: usize,
    active_preset: usize,
    custom_colors: [SkydimoRgb; MAX_COLORS],
    custom_color_count: usize,
    gradient_samples: [SkydimoRgb; GRADIENT_SAMPLES],
}

impl Default for NoiseMapEffect {
    fn default() -> Self {
        let params = Params::default();
        let mut effect = Self {
            params,
            noise: SimplexNoise::from_params(&params),
            progress: 0.0,
            last_elapsed: None,
            time_carry: 0.0,
            width: 0,
            height: 1,
            active_preset: params.preset,
            custom_colors: [BLACK; MAX_COLORS],
            custom_color_count: 0,
            gradient_samples: [BLACK; GRADIENT_SAMPLES],
        };
        effect.apply_preset(params.preset);
        effect
    }
}

impl NoiseMapEffect {
    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = width as usize;
        self.height = height as usize;
        if self.width == 0 && led_count > 0 {
            self.width = led_count as usize;
            self.height = 1;
        }
        if self.height == 0 {
            self.height = 1;
        }
    }

    fn update_params(&mut self, json: &str) {
        if let Some(value) = parse_number_field(json, "speed") {
            self.params.speed = rounded_clamped_u32(value, 1, 100);
        }

        let mut noise_dirty = false;
        if let Some(value) = parse_number_field(json, "frequency") {
            let next = clamped_f64(value, 0.0001, 0.5);
            if next != self.params.frequency {
                self.params.frequency = next;
                noise_dirty = true;
            }
        }
        if let Some(value) = parse_number_field(json, "amplitude") {
            let next = clamped_f64(value, 0.0001, 5.0);
            if next != self.params.amplitude {
                self.params.amplitude = next;
                noise_dirty = true;
            }
        }
        if let Some(value) = parse_number_field(json, "lacunarity") {
            let next = clamped_f64(value, 0.0001, 5.0);
            if next != self.params.lacunarity {
                self.params.lacunarity = next;
                noise_dirty = true;
            }
        }
        if let Some(value) = parse_number_field(json, "persistence") {
            let next = clamped_f64(value, 0.0001, 5.0);
            if next != self.params.persistence {
                self.params.persistence = next;
                noise_dirty = true;
            }
        }
        if let Some(value) = parse_number_field(json, "octaves") {
            let next = rounded_clamped_u32(value, 1, 20);
            if next != self.params.octaves {
                self.params.octaves = next;
                noise_dirty = true;
            }
        }

        if let Some(value) = parse_number_field(json, "motion") {
            self.params.motion = rounded_clamped_u32(value, MOTION_UP, MOTION_RIGHT);
        }
        if let Some(value) = parse_number_field(json, "motion_speed") {
            self.params.motion_speed = rounded_clamped_u32(value, 0, 99);
        }
        if let Some(value) = parse_number_field(json, "colors_choice") {
            self.params.colors_choice = rounded_clamped_u32(value, MODE_RAINBOW, MODE_CUSTOM);
        }

        let mut preset_changed = false;
        if let Some(value) = parse_number_field(json, "preset") {
            let next = rounded_clamped_usize(value, 0, PRESETS.len() - 1);
            if next != self.active_preset {
                self.apply_preset(next);
                preset_changed = true;
            } else {
                self.params.preset = next;
            }
        }

        if !preset_changed {
            if let Some((colors, count)) = parse_color_list_field(json, "colors") {
                self.set_custom_colors(&colors[..count]);
            }
        }

        if noise_dirty {
            self.noise = SimplexNoise::from_params(&self.params);
        }
    }

    fn tick(&mut self, elapsed_seconds: f64, pixels: &mut [SkydimoRgb]) -> i32 {
        if pixels.is_empty() {
            return 0;
        }

        let width = if self.width == 0 {
            pixels.len()
        } else {
            self.width
        }
        .max(1);
        let height = self.height.max(1);

        let (x_shift, y_shift) = match self.params.motion {
            MOTION_UP => (0.0, self.params.motion_speed as f64 * self.progress),
            MOTION_DOWN => (0.0, self.params.motion_speed as f64 * -self.progress),
            MOTION_LEFT => (self.params.motion_speed as f64 * self.progress, 0.0),
            MOTION_RIGHT => (self.params.motion_speed as f64 * -self.progress, 0.0),
            _ => (0.0, 0.0),
        };

        let mut idx = 0usize;
        for y in 0..height {
            let sample_y = y as f64 + y_shift;
            for x in 0..width {
                if idx >= pixels.len() {
                    self.advance(elapsed_seconds);
                    return 0;
                }

                let value = self.noise.fractal(
                    self.params.octaves,
                    x as f64 + x_shift,
                    sample_y,
                    self.progress,
                );
                let frac = (1.0 + value) * 0.5;
                pixels[idx] = self.color_for_fraction(frac);
                idx += 1;
            }
        }

        if idx < pixels.len() {
            pixels[idx..].fill(BLACK);
        }
        self.advance(elapsed_seconds);
        0
    }

    fn color_for_fraction(&self, frac: f64) -> SkydimoRgb {
        match self.params.colors_choice {
            MODE_RAINBOW => hsv_to_rgb(360.0 * frac, 1.0, 1.0),
            MODE_INVERSE_RAINBOW => hsv_to_rgb(360.0 - (360.0 * frac), 1.0, 1.0),
            _ => {
                let scaled = if frac.is_finite() {
                    ((1.0 - frac) * 100.0).floor().clamp(0.0, 100.0)
                } else {
                    0.0
                };
                self.gradient_samples[scaled as usize]
            }
        }
    }

    fn apply_preset(&mut self, index: usize) {
        let index = index.min(PRESETS.len() - 1);
        self.active_preset = index;
        self.params.preset = index;
        self.set_custom_colors(PRESETS[index]);
    }

    fn set_custom_colors(&mut self, colors: &[SkydimoRgb]) {
        if colors.is_empty() {
            return;
        }
        let count = colors.len().min(MAX_COLORS);
        self.custom_colors[..count].copy_from_slice(&colors[..count]);
        if count < MAX_COLORS {
            self.custom_colors[count..].fill(BLACK);
        }
        self.custom_color_count = count;
        self.generate_gradient();
    }

    fn generate_gradient(&mut self) {
        let mut i = 0usize;
        while i < GRADIENT_SAMPLES {
            self.gradient_samples[i] = self.sample_gradient(i as f64 / 100.0);
            i += 1;
        }
    }

    fn sample_gradient(&self, t: f64) -> SkydimoRgb {
        let count = self.custom_color_count;
        if count == 0 {
            return BLACK;
        }
        if count == 1 {
            return self.custom_colors[0];
        }
        if t <= 0.0 {
            return self.custom_colors[0];
        }

        let step = 1.0 / count as f64;
        let last_stop = (count - 1) as f64 * step;
        if t >= last_stop {
            return self.custom_colors[count - 1];
        }

        let segment = (t / step).floor() as usize;
        if segment >= count - 1 {
            return self.custom_colors[count - 1];
        }
        let local_t = (t - segment as f64 * step) / step;
        lerp_rgb(self.custom_colors[segment], self.custom_colors[segment + 1], local_t)
    }

    fn advance(&mut self, elapsed_seconds: f64) {
        let mut dt = FRAME_DT;
        if elapsed_seconds.is_finite() && elapsed_seconds >= 0.0 {
            if let Some(last) = self.last_elapsed {
                dt = (elapsed_seconds - last).max(0.0);
            }
            self.last_elapsed = Some(elapsed_seconds);
        } else {
            self.last_elapsed = None;
        }

        self.time_carry += dt;
        if self.time_carry >= FRAME_DT {
            let steps = (self.time_carry / FRAME_DT).floor();
            self.progress += steps * (0.1 * self.params.speed as f64 / REFERENCE_FPS);
            self.time_carry -= steps * FRAME_DT;
        }
    }
}

unsafe extern "C" fn noise_map_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() {
            return -1;
        }
        if !host.is_null() {
            let host_ref = unsafe { &*host };
            if host_ref.abi_version < SKYDIMO_NATIVE_C_ABI_VERSION {
                return -2;
            }
        }

        let effect = Box::new(NoiseMapEffect::default());
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn noise_map_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<NoiseMapEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn noise_map_resize(
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

unsafe extern "C" fn noise_map_update_params_json(
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

unsafe extern "C" fn noise_map_tick(
    instance: *mut c_void,
    elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        if len == 0 {
            return 0;
        }
        if buffer.is_null() {
            return -2;
        }
        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(elapsed_seconds, pixels)
    })
}

unsafe extern "C" fn noise_map_is_ready(instance: *mut c_void) -> i32 {
    catch_ffi(|| if instance.is_null() { -1 } else { 1 })
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be writable for one `SkydimoPluginApiV1`.
/// `requested_abi_version` must match the ABI declared by the manifest.
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
                    create: Some(noise_map_create),
                    destroy: Some(noise_map_destroy),
                    resize: Some(noise_map_resize),
                    update_params_json: Some(noise_map_update_params_json),
                    tick: Some(noise_map_tick),
                    is_ready: Some(noise_map_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut NoiseMapEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<NoiseMapEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline(always)]
fn noise3(x: f64, y: f64, z: f64) -> f64 {
    let mut n0 = 0.0;
    let mut n1 = 0.0;
    let mut n2 = 0.0;
    let mut n3 = 0.0;

    let s = (x + y + z) * (1.0 / 3.0);
    let i = fastfloor(x + s);
    let j = fastfloor(y + s);
    let k = fastfloor(z + s);

    let t = (i + j + k) as f64 * (1.0 / 6.0);
    let x0 = x - (i as f64 - t);
    let y0 = y - (j as f64 - t);
    let z0 = z - (k as f64 - t);

    let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
        if y0 >= z0 {
            (1, 0, 0, 1, 1, 0)
        } else if x0 >= z0 {
            (1, 0, 0, 1, 0, 1)
        } else {
            (0, 0, 1, 1, 0, 1)
        }
    } else if y0 < z0 {
        (0, 0, 1, 0, 1, 1)
    } else if x0 < z0 {
        (0, 1, 0, 0, 1, 1)
    } else {
        (0, 1, 0, 1, 1, 0)
    };

    let x1 = x0 - i1 as f64 + (1.0 / 6.0);
    let y1 = y0 - j1 as f64 + (1.0 / 6.0);
    let z1 = z0 - k1 as f64 + (1.0 / 6.0);
    let x2 = x0 - i2 as f64 + (2.0 / 6.0);
    let y2 = y0 - j2 as f64 + (2.0 / 6.0);
    let z2 = z0 - k2 as f64 + (2.0 / 6.0);
    let x3 = x0 - 1.0 + (3.0 / 6.0);
    let y3 = y0 - 1.0 + (3.0 / 6.0);
    let z3 = z0 - 1.0 + (3.0 / 6.0);

    let gi0 = hash(i + hash(j + hash(k) as i32) as i32);
    let gi1 = hash(i + i1 + hash(j + j1 + hash(k + k1) as i32) as i32);
    let gi2 = hash(i + i2 + hash(j + j2 + hash(k + k2) as i32) as i32);
    let gi3 = hash(i + 1 + hash(j + 1 + hash(k + 1) as i32) as i32);

    let t0 = 0.6 - x0 * x0 - y0 * y0 - z0 * z0;
    if t0 >= 0.0 {
        let t0_sq = t0 * t0;
        n0 = t0_sq * t0_sq * grad3(gi0, x0, y0, z0);
    }

    let t1 = 0.6 - x1 * x1 - y1 * y1 - z1 * z1;
    if t1 >= 0.0 {
        let t1_sq = t1 * t1;
        n1 = t1_sq * t1_sq * grad3(gi1, x1, y1, z1);
    }

    let t2 = 0.6 - x2 * x2 - y2 * y2 - z2 * z2;
    if t2 >= 0.0 {
        let t2_sq = t2 * t2;
        n2 = t2_sq * t2_sq * grad3(gi2, x2, y2, z2);
    }

    let t3 = 0.6 - x3 * x3 - y3 * y3 - z3 * z3;
    if t3 >= 0.0 {
        let t3_sq = t3 * t3;
        n3 = t3_sq * t3_sq * grad3(gi3, x3, y3, z3);
    }

    32.0 * (n0 + n1 + n2 + n3)
}

#[inline(always)]
fn fastfloor(fp: f64) -> i32 {
    let i = fp as i32;
    if fp < i as f64 {
        i - 1
    } else {
        i
    }
}

#[inline(always)]
fn hash(i: i32) -> u8 {
    PERM[(i & 255) as usize]
}

#[inline(always)]
fn grad3(hash_value: u8, x: f64, y: f64, z: f64) -> f64 {
    let h = hash_value & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if (h & 1) != 0 { -u } else { u }) + (if (h & 2) != 0 { -v } else { v })
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

fn lerp_rgb(a: SkydimoRgb, b: SkydimoRgb, t: f64) -> SkydimoRgb {
    SkydimoRgb {
        r: lerp_channel(a.r, b.r, t),
        g: lerp_channel(a.g, b.g, t),
        b: lerp_channel(a.b, b.b, t),
    }
}

fn lerp_channel(a: u8, b: u8, t: f64) -> u8 {
    to_u8(a as f64 + (b as f64 - a as f64) * t)
}

fn parse_number_field(json: &str, key: &str) -> Option<f64> {
    json_value_slice(json, key)?.parse::<f64>().ok()
}

fn parse_color_list_field(json: &str, key: &str) -> Option<([SkydimoRgb; MAX_COLORS], usize)> {
    let raw = json_array_slice(json, key)?;
    let mut parsed = [BLACK; MAX_COLORS];
    let mut count = 0usize;
    let bytes = raw.as_bytes();
    let mut pos = 0usize;

    while pos < bytes.len() {
        while pos < bytes.len() && bytes[pos] != b'"' {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        let start = pos + 1;
        pos = start;
        let mut escaped = false;
        while pos < bytes.len() {
            let byte = bytes[pos];
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                break;
            }
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        if let Some(color) = parse_hex_color(&raw[start..pos]) {
            if count < MAX_COLORS {
                parsed[count] = color;
                count += 1;
            }
        }
        pos += 1;
    }

    (count > 0).then_some((parsed, count))
}

fn parse_hex_color(value: &str) -> Option<SkydimoRgb> {
    let trimmed = value.trim();
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

fn json_value_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();

    if let Some(rest) = raw.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(rest[..end].trim());
    }

    let end = raw
        .char_indices()
        .find_map(|(idx, ch)| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '+' | '.') {
                None
            } else {
                Some(idx)
            }
        })
        .unwrap_or(raw.len());
    Some(raw[..end].trim())
}

fn json_array_slice<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let pos = json.find(&needle)?;
    let after_key = &json[pos + needle.len()..];
    let colon = after_key.find(':')?;
    let raw = after_key[colon + 1..].trim_start();
    let bytes = raw.as_bytes();
    if bytes.first().copied()? != b'[' {
        return None;
    }

    let mut in_string = false;
    let mut escaped = false;
    let mut depth = 0usize;
    for (idx, byte) in bytes.iter().copied().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&raw[1..idx]);
                }
            }
            _ => {}
        }
    }
    None
}

fn rounded_clamped_u32(value: f64, min: u32, max: u32) -> u32 {
    if !value.is_finite() {
        return min;
    }
    (value + 0.5).floor().clamp(min as f64, max as f64) as u32
}

fn rounded_clamped_usize(value: f64, min: usize, max: usize) -> usize {
    if !value.is_finite() {
        return min;
    }
    (value + 0.5).floor().clamp(min as f64, max as f64) as usize
}

fn clamped_f64(value: f64, min: f64, max: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        min
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_effect_api_for_current_abi() {
        let mut api = SkydimoPluginApiV1::default();
        let status = unsafe {
            skydimo_plugin_get_api(
                SKYDIMO_NATIVE_C_ABI_VERSION,
                std::ptr::null(),
                &mut api as *mut SkydimoPluginApiV1,
            )
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(api.kind_mask, SKYDIMO_PLUGIN_KIND_EFFECT);
        assert!(api.effect.create.is_some());
        assert!(api.effect.tick.is_some());
    }

    #[test]
    fn parses_color_arrays_and_short_hex() {
        let (colors, count) =
            parse_color_list_field(r##"{"colors":["#0af","#123456"]}"##, "colors").unwrap();

        assert_eq!(count, 2);
        assert_eq!(
            colors[0],
            SkydimoRgb {
                r: 0,
                g: 170,
                b: 255,
            }
        );
        assert_eq!(
            colors[1],
            SkydimoRgb {
                r: 18,
                g: 52,
                b: 86,
            }
        );
    }

    #[test]
    fn renders_custom_gradient_pixels() {
        let mut effect = NoiseMapEffect::default();
        effect.resize(8, 1, 8);
        effect.update_params(
            r##"{"colors_choice":2,"colors":["#FF0000","#0000FF"],"speed":50}"##,
        );
        let mut pixels = [BLACK; 8];

        assert_eq!(effect.tick(0.0, &mut pixels), 0);
        assert!(pixels.iter().any(|pixel| *pixel != BLACK));
    }

    #[test]
    fn simplex_origin_is_stable() {
        assert_eq!(noise3(0.0, 0.0, 0.0), 0.0);
        let sample = noise3(0.25, -0.5, 0.75);
        assert!(sample.is_finite());
        assert!((-1.5..=1.5).contains(&sample));
    }
}
