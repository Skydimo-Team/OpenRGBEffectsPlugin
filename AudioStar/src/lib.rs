use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;

const SKYDIMO_NATIVE_C_ABI_VERSION: u32 = 3;
const SKYDIMO_PLUGIN_KIND_EFFECT: u32 = 1 << 0;

const PI: f32 = std::f32::consts::PI;
const FFT_BINS: usize = 256;
const ALBUM_ART_SIZE: usize = 64;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoStr {
    pub ptr: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoFloatSliceV1 {
    pub ptr: *const f32,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoAudioFrameV1 {
    pub amplitude: f32,
    pub bins: SkydimoFloatSliceV1,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoRgbFrameV1 {
    pub width: usize,
    pub height: usize,
    pub pixels: *const SkydimoRgb,
    pub pixels_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoMatrixMapV1 {
    pub width: usize,
    pub height: usize,
    pub map: *const i64,
    pub map_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoLayoutTransformV1 {
    pub flip_horizontal: u8,
    pub flip_vertical: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputCapabilitiesV1 {
    pub editable: u8,
    pub min_total_leds: usize,
    pub max_total_leds: usize,
    pub allowed_total_leds: *const usize,
    pub allowed_total_leds_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputDefinitionV1 {
    pub id: SkydimoStr,
    pub name: SkydimoStr,
    pub output_type: u32,
    pub leds_count: usize,
    pub matrix: *const SkydimoMatrixMapV1,
    pub transform: SkydimoLayoutTransformV1,
    pub capabilities: SkydimoOutputCapabilitiesV1,
    pub default_effect: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoOutputFrameV1 {
    pub output_id: SkydimoStr,
    pub colors: *const SkydimoRgb,
    pub colors_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoLedColorV1 {
    pub index: usize,
    pub color: SkydimoRgb,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoDeviceInfoV1 {
    pub manufacturer: SkydimoStr,
    pub model: SkydimoStr,
    pub serial_id: SkydimoStr,
    pub description: SkydimoStr,
    pub device_type: u32,
    pub image_url: SkydimoStr,
    pub controller_id: SkydimoStr,
    pub controller_name: SkydimoStr,
    pub device_path: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SkydimoHardwareCandidateV1 {
    pub candidate_type: u32,
    pub port_key: SkydimoStr,
    pub path: SkydimoStr,
    pub vendor_id: u32,
    pub product_id: u32,
    pub has_vendor_id: u8,
    pub has_product_id: u8,
    pub interface_number: i32,
    pub has_interface_number: u8,
    pub serial_number: SkydimoStr,
    pub manufacturer_string: SkydimoStr,
    pub product_string: SkydimoStr,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SkydimoHostApiV1 {
    pub size: u32,
    pub abi_version: u32,
    pub host_ctx: *mut c_void,
    pub log: Option<unsafe extern "C" fn(*mut c_void, u32, *const c_char, usize)>,
    pub call_json: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *mut u8,
            usize,
            *mut usize,
        ) -> i32,
    >,
    pub controller_set_device_info:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoDeviceInfoV1) -> i32>,
    pub controller_add_output:
        Option<unsafe extern "C" fn(*mut c_void, *const SkydimoOutputDefinitionV1) -> i32>,
    pub controller_output_led_count:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> usize>,
    pub controller_get_rgb_bytes:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut u8, usize) -> isize>,
    pub controller_write: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_read: Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u32) -> isize>,
    pub controller_hid_send_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> isize>,
    pub controller_hid_get_feature_report:
        Option<unsafe extern "C" fn(*mut c_void, *mut u8, usize, u8) -> isize>,
    pub extension_lock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
            *mut usize,
            *mut usize,
        ) -> i32,
    >,
    pub extension_unlock_leds: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const usize,
            usize,
        ) -> i32,
    >,
    pub extension_set_leds_rgb: Option<
        unsafe extern "C" fn(
            *mut c_void,
            *const c_char,
            usize,
            *const c_char,
            usize,
            *const SkydimoLedColorV1,
            usize,
        ) -> i32,
    >,
    pub effect_audio_capture:
        Option<unsafe extern "C" fn(*mut c_void, usize, *mut SkydimoAudioFrameV1) -> i32>,
    pub effect_screen_capture:
        Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut SkydimoRgbFrameV1) -> i32>,
    pub effect_album_art:
        Option<unsafe extern "C" fn(*mut c_void, usize, usize, *mut SkydimoRgbFrameV1) -> i32>,
    pub get_plugin_id: Option<unsafe extern "C" fn(*mut c_void, *mut SkydimoStr) -> i32>,
}

impl Default for SkydimoHostApiV1 {
    fn default() -> Self {
        Self {
            size: std::mem::size_of::<Self>() as u32,
            abi_version: SKYDIMO_NATIVE_C_ABI_VERSION,
            host_ctx: std::ptr::null_mut(),
            log: None,
            call_json: None,
            controller_set_device_info: None,
            controller_add_output: None,
            controller_output_led_count: None,
            controller_get_rgb_bytes: None,
            controller_write: None,
            controller_read: None,
            controller_hid_send_feature_report: None,
            controller_hid_get_feature_report: None,
            extension_lock_leds: None,
            extension_unlock_leds: None,
            extension_set_leds_rgb: None,
            effect_audio_capture: None,
            effect_screen_capture: None,
            effect_album_art: None,
            get_plugin_id: None,
        }
    }
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

#[derive(Clone, Copy)]
struct HostBridge {
    host: SkydimoHostApiV1,
}

struct AudioFrameRef {
    amplitude: f32,
    bins: &'static [f32],
}

struct RgbFrameRef {
    width: usize,
    height: usize,
    pixels: &'static [SkydimoRgb],
}

impl HostBridge {
    fn from_raw(host: *const SkydimoHostApiV1) -> Self {
        let host = if host.is_null() {
            SkydimoHostApiV1::default()
        } else {
            unsafe { *host }
        };
        Self { host }
    }

    fn capture_audio(&self, avg_size: usize) -> Option<AudioFrameRef> {
        let capture = self.host.effect_audio_capture?;
        let mut frame = SkydimoAudioFrameV1::default();
        let status = unsafe { capture(self.host.host_ctx, avg_size, &mut frame) };
        if status <= 0 || frame.bins.ptr.is_null() || frame.bins.len == 0 {
            return None;
        }
        let bins = unsafe { slice::from_raw_parts(frame.bins.ptr, frame.bins.len.min(FFT_BINS)) };
        Some(AudioFrameRef {
            amplitude: frame.amplitude.max(0.0),
            bins,
        })
    }

    fn capture_album_art(&self, width: usize, height: usize) -> Option<RgbFrameRef> {
        let capture = self.host.effect_album_art?;
        let mut frame = SkydimoRgbFrameV1::default();
        let status = unsafe { capture(self.host.host_ctx, width, height, &mut frame) };
        if status <= 0 || frame.pixels.is_null() || frame.width == 0 || frame.height == 0 {
            return None;
        }
        let expected = frame.width.saturating_mul(frame.height);
        if expected == 0 || frame.pixels_len < expected {
            return None;
        }
        let pixels = unsafe { slice::from_raw_parts(frame.pixels, expected) };
        Some(RgbFrameRef {
            width: frame.width,
            height: frame.height,
            pixels,
        })
    }

}

#[derive(Clone, Copy)]
struct Config {
    speed: f32,
    avg_size: f32,
    use_album_art: bool,
    edge_beat: bool,
    edge_beat_hue: f32,
    edge_beat_saturation: f32,
    edge_beat_sensitivity: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speed: 50.0,
            avg_size: 8.0,
            use_album_art: false,
            edge_beat: false,
            edge_beat_hue: 0.0,
            edge_beat_saturation: 0.0,
            edge_beat_sensitivity: 100.0,
        }
    }
}

struct AudioStarEffect {
    host: HostBridge,
    config: Config,
    time: f32,
    width: usize,
    height: usize,
    led_count: usize,
    palette_cache: PaletteCache,
}

impl AudioStarEffect {
    fn new(host: HostBridge) -> Self {
        Self {
            host,
            config: Config::default(),
            time: 0.0,
            width: 0,
            height: 0,
            led_count: 0,
            palette_cache: PaletteCache::default(),
        }
    }

    fn resize(&mut self, width: u32, height: u32, led_count: u32) {
        self.width = width as usize;
        self.height = height as usize;
        self.led_count = led_count as usize;
    }

    fn update_params_json(&mut self, raw: &str) {
        if let Some(value) = json_number(raw, "speed") {
            self.config.speed = value;
        }
        if let Some(value) = json_number(raw, "avgSize") {
            self.config.avg_size = value;
        }
        if let Some(value) = json_bool(raw, "useAlbumArt") {
            self.config.use_album_art = value;
        }
        if let Some(value) = json_bool(raw, "edgeBeat") {
            self.config.edge_beat = value;
        }
        if let Some(value) = json_number(raw, "edgeBeatHue") {
            self.config.edge_beat_hue = value;
        }
        if let Some(value) = json_number(raw, "edgeBeatSaturation") {
            self.config.edge_beat_saturation = value;
        }
        if let Some(value) = json_number(raw, "edgeBeatSensitivity") {
            self.config.edge_beat_sensitivity = value;
        }
    }

    fn tick(&mut self, buffer: &mut [SkydimoRgb]) {
        if buffer.is_empty() {
            return;
        }

        let width = self.width.max(1);
        let height = self.height.max(1);
        let width = if self.width == 0 { buffer.len() } else { width };
        let height = if self.height == 0 { 1 } else { height };
        let avg_size = clamp_f32(self.config.avg_size.floor(), 1.0, FFT_BINS as f32) as usize;

        let Some(audio) = self.host.capture_audio(avg_size) else {
            fill_black(buffer);
            return;
        };

        let palette_len = if self.config.use_album_art {
            self.host
                .capture_album_art(ALBUM_ART_SIZE, ALBUM_ART_SIZE)
                .and_then(|art| {
                    self.palette_cache
                        .refresh(art.pixels, art.width, art.height, 8)
                })
                .map_or(0, |palette| palette.len())
        } else {
            0
        };
        let palette = (palette_len > 0).then_some(&self.palette_cache.colors[..palette_len]);

        let render = RenderInputs {
            bins: audio.bins,
            amp: audio.amplitude,
            palette,
            config: self.config,
            time: self.time,
        };

        if height == 1 || width == 1 {
            render_linear(buffer, render);
        } else {
            render_matrix(buffer, width, height, render);
        }

        self.time = (self.time + self.config.speed / 60.0).rem_euclid(360.0);
    }
}

#[derive(Clone, Copy)]
struct PaletteColor {
    r: u8,
    g: u8,
    b: u8,
    weight: f32,
}

#[derive(Default)]
struct PaletteCache {
    checksum: Option<u32>,
    colors: Vec<PaletteColor>,
    flat: Vec<u8>,
    temp: Vec<u8>,
    blurred: Vec<u8>,
    kernel: Vec<f32>,
}

#[derive(Clone, Copy)]
struct RenderInputs<'a> {
    bins: &'a [f32],
    amp: f32,
    palette: Option<&'a [PaletteColor]>,
    config: Config,
    time: f32,
}

impl PaletteCache {
    fn refresh(
        &mut self,
        pixels: &[SkydimoRgb],
        width: usize,
        height: usize,
        max_colors: usize,
    ) -> Option<&[PaletteColor]> {
        if pixels.is_empty() || width == 0 || height == 0 {
            self.colors.clear();
            self.checksum = None;
            return None;
        }

        let checksum = art_fingerprint(pixels)?;
        if self.checksum == Some(checksum) && !self.colors.is_empty() {
            return Some(&self.colors);
        }

        self.extract(pixels, width, height, max_colors.clamp(1, 16));
        self.checksum = Some(checksum);
        (!self.colors.is_empty()).then_some(&self.colors)
    }

    fn extract(&mut self, pixels: &[SkydimoRgb], width: usize, height: usize, max_colors: usize) {
        let total = width.saturating_mul(height);
        if total == 0 || pixels.len() < total {
            self.colors.clear();
            return;
        }

        let channels = total.saturating_mul(3);
        self.flat.resize(channels, 0);
        self.temp.resize(channels, 0);
        self.blurred.resize(channels, 0);
        for (i, pixel) in pixels.iter().take(total).enumerate() {
            let off = i * 3;
            self.flat[off] = pixel.r;
            self.flat[off + 1] = pixel.g;
            self.flat[off + 2] = pixel.b;
        }

        gaussian_blur(
            &self.flat,
            width,
            height,
            8,
            &mut self.kernel,
            &mut self.temp,
            &mut self.blurred,
        );

        self.colors.clear();
        let grid_size = 4usize;
        let min_dist_sq = 30i32 * 30i32;

        for gy in 0..grid_size {
            for gx in 0..grid_size {
                let mut sx = (gx * width) / grid_size + width / (grid_size * 2);
                let mut sy = (gy * height) / grid_size + height / (grid_size * 2);
                if sx >= width {
                    sx = width - 1;
                }
                if sy >= height {
                    sy = height - 1;
                }

                let off = (sy * width + sx) * 3;
                let r = self.blurred[off];
                let g = self.blurred[off + 1];
                let b = self.blurred[off + 2];
                if r.max(g).max(b) < 24 {
                    continue;
                }

                let mut closest_idx = None;
                let mut closest_dist = i32::MAX;
                for (idx, color) in self.colors.iter().enumerate() {
                    let dr = color.r as i32 - r as i32;
                    let dg = color.g as i32 - g as i32;
                    let db = color.b as i32 - b as i32;
                    let dist = dr * dr + dg * dg + db * db;
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_idx = Some(idx);
                    }
                }

                if let Some(idx) = closest_idx.filter(|_| closest_dist < min_dist_sq) {
                    self.colors[idx].weight += 1.0;
                } else if self.colors.len() < max_colors {
                    self.colors.push(PaletteColor {
                        r,
                        g,
                        b,
                        weight: 1.0,
                    });
                }
            }
        }

        let total_weight = self.colors.iter().map(|color| color.weight).sum::<f32>();
        if total_weight <= 0.0 {
            let uniform = 1.0 / self.colors.len().max(1) as f32;
            for color in &mut self.colors {
                color.weight = uniform;
            }
        } else {
            for color in &mut self.colors {
                color.weight /= total_weight;
            }
        }
        self.colors
            .sort_by(|left, right| right.weight.total_cmp(&left.weight));
    }
}

fn render_linear(
    out: &mut [SkydimoRgb],
    render: RenderInputs<'_>,
) {
    let n = out.len();
    let exponent = 1.0 / (render.amp + 1.0);
    let flow_phase = (render.time / 360.0).rem_euclid(1.0);
    let edge_zone = ((n as f32 * 0.1).floor() as usize).max(1);
    let edge_color = render.config.edge_beat.then(|| edge_beat_color(render.bins, render.config));

    for (idx, dst) in out.iter_mut().enumerate() {
        let t = if n > 1 {
            idx as f32 / (n - 1) as f32
        } else {
            0.0
        };
        let mirror_t = (t - 0.5).abs() * 2.0;
        let angle = mirror_t * PI;
        let bin_index = fft_bin_for_angle(angle);
        let brightness = render.bins
            .get(bin_index)
            .copied()
            .unwrap_or(0.0)
            .max(0.0)
            .powf(exponent)
            .clamp(0.0, 1.0);

        let mut rgb = if let Some(palette) = render.palette {
            let (r, g, b) = sample_palette(palette, angle / PI, flow_phase);
            let (h, s, _) = rgb_to_hsv(r, g, b);
            hsv_to_rgb(h, s, brightness)
        } else {
            hsv_to_rgb((t * 360.0 + render.time).rem_euclid(360.0), 1.0, brightness)
        };

        if let Some(edge) = edge_color {
            if idx < edge_zone || idx >= n.saturating_sub(edge_zone) {
                rgb = screen_blend_rgb(rgb, edge);
            }
        }

        *dst = SkydimoRgb {
            r: rgb.0,
            g: rgb.1,
            b: rgb.2,
        };
    }
}

fn render_matrix(
    out: &mut [SkydimoRgb],
    width: usize,
    height: usize,
    render: RenderInputs<'_>,
) {
    let w = width.saturating_sub(1) as f32;
    let h = height.saturating_sub(1) as f32;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let exponent = 1.0 / (render.amp + 1.0);
    let flow_phase = (render.time / 360.0).rem_euclid(1.0);
    let edge_color = render.config.edge_beat.then(|| edge_beat_color(render.bins, render.config));

    let mut idx = 0usize;
    for y in 0..height {
        for x in 0..width {
            if idx >= out.len() {
                return;
            }

            let angle = ((x as f32) - cx).atan2((y as f32) - cy).abs();
            let bin_index = fft_bin_for_angle(angle);
            let brightness = render.bins
                .get(bin_index)
                .copied()
                .unwrap_or(0.0)
                .max(0.0)
                .powf(exponent)
                .clamp(0.0, 1.0);

            let mut rgb = if let Some(palette) = render.palette {
                let (r, g, b) = sample_palette(palette, angle / PI, flow_phase);
                let (h, s, _) = rgb_to_hsv(r, g, b);
                hsv_to_rgb(h, s, brightness)
            } else {
                hsv_to_rgb(((angle / PI) * 360.0 + render.time).rem_euclid(360.0), 1.0, brightness)
            };

            if let Some(edge) = edge_color {
                if x == 0 || x as f32 >= w || y == 0 || y as f32 >= h {
                    rgb = screen_blend_rgb(rgb, edge);
                }
            }

            out[idx] = SkydimoRgb {
                r: rgb.0,
                g: rgb.1,
                b: rgb.2,
            };
            idx += 1;
        }
    }
}

fn gaussian_blur(
    pixels: &[u8],
    width: usize,
    height: usize,
    radius: usize,
    kernel: &mut Vec<f32>,
    temp: &mut [u8],
    out: &mut [u8],
) {
    if radius == 0 {
        out.copy_from_slice(pixels);
        return;
    }

    let ksize = radius * 2 + 1;
    kernel.resize(ksize, 0.0);
    let sigma = radius as f32 / 3.0;
    let sigma2 = 2.0 * sigma * sigma;
    let mut ksum = 0.0;
    for (i, slot) in kernel.iter_mut().enumerate() {
        let x = i as f32 - radius as f32;
        let value = (-(x * x) / sigma2).exp();
        *slot = value;
        ksum += value;
    }
    if ksum > 0.0 {
        for value in kernel.iter_mut() {
            *value /= ksum;
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut rs = 0.0;
            let mut gs = 0.0;
            let mut bs = 0.0;
            for (k, weight) in kernel.iter().enumerate() {
                let sx = clamp_isize(x as isize + k as isize - radius as isize, 0, width as isize - 1) as usize;
                let off = (y * width + sx) * 3;
                rs += pixels[off] as f32 * *weight;
                gs += pixels[off + 1] as f32 * *weight;
                bs += pixels[off + 2] as f32 * *weight;
            }
            let off = (y * width + x) * 3;
            temp[off] = round_u8(rs);
            temp[off + 1] = round_u8(gs);
            temp[off + 2] = round_u8(bs);
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut rs = 0.0;
            let mut gs = 0.0;
            let mut bs = 0.0;
            for (k, weight) in kernel.iter().enumerate() {
                let sy = clamp_isize(y as isize + k as isize - radius as isize, 0, height as isize - 1) as usize;
                let off = (sy * width + x) * 3;
                rs += temp[off] as f32 * *weight;
                gs += temp[off + 1] as f32 * *weight;
                bs += temp[off + 2] as f32 * *weight;
            }
            let off = (y * width + x) * 3;
            out[off] = round_u8(rs);
            out[off + 1] = round_u8(gs);
            out[off + 2] = round_u8(bs);
        }
    }
}

fn sample_palette(palette: &[PaletteColor], pos_01: f32, flow_phase_01: f32) -> (u8, u8, u8) {
    if palette.is_empty() {
        return (0, 0, 0);
    }
    if palette.len() == 1 {
        let color = palette[0];
        return (color.r, color.g, color.b);
    }

    let mut pos = clamp_f32(pos_01, 0.0, 1.0) + clamp_f32(flow_phase_01, 0.0, 1.0);
    if pos > 1.0 {
        pos -= 1.0;
    }

    let mut cumulative = 0.0;
    let mut index0 = 0usize;
    let mut index1 = 1usize;
    let mut local_t = 0.0;

    for (idx, color) in palette.iter().enumerate() {
        let weight = color.weight;
        if weight <= 0.0 {
            continue;
        }
        let next_cum = cumulative + weight;
        if pos <= next_cum || idx == palette.len() - 1 {
            index0 = idx;
            index1 = if idx + 1 < palette.len() { idx + 1 } else { 0 };
            local_t = clamp_f32((pos - cumulative) / weight.max(0.0001), 0.0, 1.0);
            break;
        }
        cumulative = next_cum;
    }

    let c0 = palette[index0];
    let c1 = palette[index1];
    (
        lerp_u8(c0.r, c1.r, local_t),
        lerp_u8(c0.g, c1.g, local_t),
        lerp_u8(c0.b, c1.b, local_t),
    )
}

fn art_fingerprint(pixels: &[SkydimoRgb]) -> Option<u32> {
    if pixels.is_empty() {
        return None;
    }
    let mut hash = 0x5555_5555u64;
    let step = (pixels.len() / 16).max(1);
    for idx in (0..pixels.len()).step_by(step) {
        let pixel = pixels[idx];
        let packed = ((pixel.r as u64) << 16) | ((pixel.g as u64) << 8) | pixel.b as u64;
        hash = ((hash * 31) + packed) % 0x7fff_ffff;
    }
    Some(hash as u32)
}

fn edge_beat_color(bins: &[f32], config: Config) -> (u8, u8, u8) {
    let bass_amp = bins.first().copied().unwrap_or(0.0) + bins.get(8).copied().unwrap_or(0.0);
    let edge_value = clamp_f32(0.01 * config.edge_beat_sensitivity * bass_amp, 0.0, 1.0);
    let saturation = clamp_f32(config.edge_beat_saturation / 255.0, 0.0, 1.0);
    hsv_to_rgb(config.edge_beat_hue.rem_euclid(360.0), saturation, edge_value)
}

fn fft_bin_for_angle(angle: f32) -> usize {
    clamp_isize((FFT_BINS as f32 * (angle / (PI * 2.0))).floor() as isize, 0, FFT_BINS as isize - 1)
        as usize
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0);
    let s = clamp_f32(s, 0.0, 1.0);
    let v = clamp_f32(v, 0.0, 1.0);

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

    (
        round_u8((r + m) * 255.0),
        round_u8((g + m) * 255.0),
        round_u8((b + m) * 255.0),
    )
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let maxc = rf.max(gf).max(bf);
    let minc = rf.min(gf).min(bf);
    let delta = maxc - minc;
    let v = maxc;
    let s = if maxc == 0.0 { 0.0 } else { delta / maxc };
    let h = if delta == 0.0 {
        0.0
    } else if maxc == rf {
        60.0 * ((gf - bf) / delta).rem_euclid(6.0)
    } else if maxc == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    (h, s, v)
}

fn screen_blend_rgb(base: (u8, u8, u8), edge: (u8, u8, u8)) -> (u8, u8, u8) {
    (
        screen_blend(base.0, edge.0),
        screen_blend(base.1, edge.1),
        screen_blend(base.2, edge.2),
    )
}

fn screen_blend(a: u8, b: u8) -> u8 {
    let af = a as f32 / 255.0;
    let bf = b as f32 / 255.0;
    ((1.0 - (1.0 - af) * (1.0 - bf)) * 255.0).floor() as u8
}

fn fill_black(out: &mut [SkydimoRgb]) {
    out.fill(SkydimoRgb::default());
}

fn json_number(raw: &str, key: &str) -> Option<f32> {
    let value = json_value_start(raw, key)?;
    let bytes = value.as_bytes();
    let mut end = 0usize;
    while end < bytes.len() {
        let ch = bytes[end] as char;
        if ch.is_ascii_digit() || matches!(ch, '-' | '+' | '.' | 'e' | 'E') {
            end += 1;
        } else {
            break;
        }
    }
    (end > 0).then(|| value[..end].parse::<f32>().ok()).flatten()
}

fn json_bool(raw: &str, key: &str) -> Option<bool> {
    let value = json_value_start(raw, key)?;
    if value.starts_with("true") {
        Some(true)
    } else if value.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_value_start<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let mut offset = 0usize;
    while let Some(pos) = raw[offset..].find(&needle) {
        let mut idx = offset + pos + needle.len();
        idx = skip_json_ws(raw, idx);
        if raw.as_bytes().get(idx) == Some(&b':') {
            idx = skip_json_ws(raw, idx + 1);
            return raw.get(idx..);
        }
        offset = idx;
    }
    None
}

fn skip_json_ws(raw: &str, mut idx: usize) -> usize {
    while let Some(ch) = raw.as_bytes().get(idx) {
        if !matches!(ch, b' ' | b'\n' | b'\r' | b'\t') {
            break;
        }
        idx += 1;
    }
    idx
}

fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

fn clamp_isize(value: isize, min: isize, max: isize) -> isize {
    value.max(min).min(max)
}

fn round_u8(value: f32) -> u8 {
    clamp_f32(value.round(), 0.0, 255.0) as u8
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    round_u8(a as f32 * (1.0 - t) + b as f32 * t)
}

unsafe fn effect_mut(instance: *mut c_void) -> Option<&'static mut AudioStarEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<AudioStarEffect>() })
    }
}

fn ffi_status(result: std::thread::Result<i32>) -> i32 {
    result.unwrap_or(-100)
}

unsafe extern "C" fn audio_star_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    ffi_status(catch_unwind(AssertUnwindSafe(|| {
        if out_instance.is_null() {
            return -1;
        }
        let effect = Box::new(AudioStarEffect::new(HostBridge::from_raw(host)));
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })))
}

unsafe extern "C" fn audio_star_destroy(instance: *mut c_void) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<AudioStarEffect>()));
            }
        }
    }));
}

unsafe extern "C" fn audio_star_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    led_count: u32,
) -> i32 {
    ffi_status(catch_unwind(AssertUnwindSafe(|| {
        let Some(effect) = (unsafe { effect_mut(instance) }) else {
            return -1;
        };
        effect.resize(width, height, led_count);
        0
    })))
}

unsafe extern "C" fn audio_star_update_params_json(
    instance: *mut c_void,
    ptr: *const c_char,
    len: usize,
) -> i32 {
    ffi_status(catch_unwind(AssertUnwindSafe(|| {
        let Some(effect) = (unsafe { effect_mut(instance) }) else {
            return -1;
        };
        if ptr.is_null() || len == 0 {
            return 0;
        }
        let bytes = unsafe { slice::from_raw_parts(ptr.cast::<u8>(), len) };
        let Ok(raw) = std::str::from_utf8(bytes) else {
            return -2;
        };
        effect.update_params_json(raw);
        0
    })))
}

unsafe extern "C" fn audio_star_tick(
    instance: *mut c_void,
    _elapsed_seconds: f64,
    buffer: *mut SkydimoRgb,
    len: usize,
) -> i32 {
    ffi_status(catch_unwind(AssertUnwindSafe(|| {
        let Some(effect) = (unsafe { effect_mut(instance) }) else {
            return -1;
        };
        if buffer.is_null() && len > 0 {
            return -2;
        }
        let out = if len == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(buffer, len) }
        };
        effect.tick(out);
        0
    })))
}

unsafe extern "C" fn audio_star_is_ready(_instance: *mut c_void) -> i32 {
    1
}

#[no_mangle]
/// # Safety
///
/// `out_api` must be a valid writable pointer supplied by the Skydimo host.
/// The host must pass ABI version 3 because this plugin uses typed effect callbacks.
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
                create: Some(audio_star_create),
                destroy: Some(audio_star_destroy),
                resize: Some(audio_star_resize),
                update_params_json: Some(audio_star_update_params_json),
                tick: Some(audio_star_tick),
                is_ready: Some(audio_star_is_ready),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_api_accepts_discovery_host_without_effect_callbacks() {
        let host = SkydimoHostApiV1::default();
        let mut api = SkydimoPluginApiV1::default();

        let status = unsafe {
            skydimo_plugin_get_api(
                SKYDIMO_NATIVE_C_ABI_VERSION,
                &host as *const SkydimoHostApiV1,
                &mut api as *mut SkydimoPluginApiV1,
            )
        };

        assert_eq!(status, 0);
        assert_eq!(api.abi_version, SKYDIMO_NATIVE_C_ABI_VERSION);
        assert_eq!(
            api.kind_mask & SKYDIMO_PLUGIN_KIND_EFFECT,
            SKYDIMO_PLUGIN_KIND_EFFECT
        );
        assert!(api.effect.create.is_some());
    }
}
