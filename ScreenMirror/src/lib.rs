mod abi;

use std::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use abi::{
    EffectRgbCaptureFn, HostLogFn, SkydimoControllerApiV1, SkydimoEffectApiV1,
    SkydimoExtensionApiV1, SkydimoHostApiV1, SkydimoPluginApiV1, SkydimoRgb,
    SkydimoRgbFrameV1, SKYDIMO_NATIVE_C_ABI_VERSION, SKYDIMO_PLUGIN_KIND_EFFECT,
};

const CAPTURE_MIN_WIDTH: usize = 64;
const CAPTURE_MIN_HEIGHT: usize = 36;
const CAPTURE_MAX_WIDTH: usize = 256;
const CAPTURE_MAX_HEIGHT: usize = 256;
const NEUTRAL_KELVIN: f32 = 6500.0;

#[derive(Clone, Copy)]
struct NativeHost {
    host_ctx: *mut c_void,
    log: Option<HostLogFn>,
    screen_capture: Option<EffectRgbCaptureFn>,
}

impl NativeHost {
    fn from_api(api: &SkydimoHostApiV1) -> Self {
        Self {
            host_ctx: api.host_ctx,
            log: api.log,
            screen_capture: api.effect_screen_capture,
        }
    }
}

#[derive(Clone, Copy)]
struct Config {
    smoothness: f32,
    brightness: f32,
    saturation: f32,
    gamma: f32,
    color_temperature: f32,
    red_calibration: f32,
    green_calibration: f32,
    blue_calibration: f32,
    blur: usize,
    auto_crop: bool,
    bb_threshold: f32,
    bb_mode: i32,
    bb_border_frame_cnt: usize,
    bb_unknown_frame_cnt: usize,
    bb_max_inconsistent_cnt: usize,
    bb_blur_remove_cnt: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            smoothness: 80.0,
            brightness: 1.0,
            saturation: 1.0,
            gamma: 1.0,
            color_temperature: NEUTRAL_KELVIN,
            red_calibration: 1.0,
            green_calibration: 1.0,
            blue_calibration: 1.0,
            blur: 0,
            auto_crop: false,
            bb_threshold: 5.0,
            bb_mode: 0,
            bb_border_frame_cnt: 50,
            bb_unknown_frame_cnt: 600,
            bb_max_inconsistent_cnt: 10,
            bb_blur_remove_cnt: 1,
        }
    }
}

#[derive(Clone)]
struct ColorPipeline {
    gain_r: f32,
    gain_g: f32,
    gain_b: f32,
    red_calibration: f32,
    green_calibration: f32,
    blue_calibration: f32,
    gamma_lut: [u8; 256],
}

impl ColorPipeline {
    fn new(config: &Config) -> Self {
        let (temp_r, temp_g, temp_b) = color_temperature_gains(config.color_temperature);
        let mut gamma_lut = [0u8; 256];
        for (idx, value) in gamma_lut.iter_mut().enumerate() {
            let normalized = idx as f32 / 255.0;
            *value = to_u8(255.0 * normalized.powf(config.gamma.max(0.1)));
        }

        Self {
            gain_r: temp_r * config.brightness,
            gain_g: temp_g * config.brightness,
            gain_b: temp_b * config.brightness,
            red_calibration: config.red_calibration,
            green_calibration: config.green_calibration,
            blue_calibration: config.blue_calibration,
            gamma_lut,
        }
    }

    #[inline]
    fn apply(&self, rgb: SkydimoRgb, saturation: f32) -> SkydimoRgb {
        let mut r = rgb.r as f32;
        let mut g = rgb.g as f32;
        let mut b = rgb.b as f32;

        if (saturation - 1.0).abs() > 0.01 {
            let gray = r.mul_add(0.299, g.mul_add(0.587, b * 0.114));
            r = gray + (r - gray) * saturation;
            g = gray + (g - gray) * saturation;
            b = gray + (b - gray) * saturation;
        }

        let r = self.gamma_lut[to_u8(r * self.gain_r) as usize];
        let g = self.gamma_lut[to_u8(g * self.gain_g) as usize];
        let b = self.gamma_lut[to_u8(b * self.gain_b) as usize];

        SkydimoRgb {
            r: to_u8(r as f32 * self.red_calibration),
            g: to_u8(g as f32 * self.green_calibration),
            b: to_u8(b as f32 * self.blue_calibration),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct BorderState {
    unknown: bool,
    horizontal_size: usize,
    vertical_size: usize,
}

impl BorderState {
    fn unknown() -> Self {
        Self {
            unknown: true,
            horizontal_size: 0,
            vertical_size: 0,
        }
    }
}

struct BorderProcessor {
    enabled: bool,
    unknown_switch_cnt: usize,
    border_switch_cnt: usize,
    max_inconsistent_cnt: usize,
    blur_remove_cnt: usize,
    mode: i32,
    threshold_percent: f32,
    current_border: BorderState,
    previous_detected_border: BorderState,
    consistent_cnt: usize,
    inconsistent_cnt: usize,
}

impl Default for BorderProcessor {
    fn default() -> Self {
        Self {
            enabled: true,
            unknown_switch_cnt: 600,
            border_switch_cnt: 50,
            max_inconsistent_cnt: 10,
            blur_remove_cnt: 1,
            mode: 0,
            threshold_percent: 5.0,
            current_border: BorderState::unknown(),
            previous_detected_border: BorderState::unknown(),
            consistent_cnt: 0,
            inconsistent_cnt: 10,
        }
    }
}

impl BorderProcessor {
    fn reset_state(&mut self) {
        self.current_border = BorderState::unknown();
        self.previous_detected_border = BorderState::unknown();
        self.consistent_cnt = 0;
        self.inconsistent_cnt = self.max_inconsistent_cnt;
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset_state();
        }
    }

    fn configure(&mut self, cfg: &Config) {
        self.threshold_percent = cfg.bb_threshold.clamp(0.0, 100.0);
        self.mode = cfg.bb_mode;
        self.border_switch_cnt = cfg.bb_border_frame_cnt;
        self.unknown_switch_cnt = cfg.bb_unknown_frame_cnt;
        self.max_inconsistent_cnt = cfg.bb_max_inconsistent_cnt;
        self.blur_remove_cnt = cfg.bb_blur_remove_cnt;
    }

    fn process_frame(&mut self, frame: FrameView<'_>) {
        if !self.enabled {
            self.current_border = BorderState::unknown();
            return;
        }

        let mut detected = self.detect(frame);
        if !detected.unknown {
            if detected.horizontal_size > 0 {
                detected.horizontal_size = detected
                    .horizontal_size
                    .saturating_add(self.blur_remove_cnt);
            }
            if detected.vertical_size > 0 {
                detected.vertical_size = detected
                    .vertical_size
                    .saturating_add(self.blur_remove_cnt);
            }
        }
        self.update_border(detected);
    }

    fn crop_region_for(&self, frame: FrameView<'_>) -> Crop {
        if self.current_border.unknown {
            return Crop::default();
        }

        let width = frame.width.max(1) as f32;
        let height = frame.height.max(1) as f32;
        let top = (self.current_border.horizontal_size as f32 / height).clamp(0.0, 0.45);
        let left = (self.current_border.vertical_size as f32 / width).clamp(0.0, 0.45);
        Crop {
            left,
            right: left,
            top,
            bottom: top,
        }
    }

    fn detect(&self, frame: FrameView<'_>) -> BorderState {
        if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
            return BorderState::unknown();
        }

        let threshold = to_u8((self.threshold_percent.clamp(0.0, 100.0) / 100.0) * 255.0);
        if frame
            .pixels
            .iter()
            .all(|pixel| is_black(*pixel, threshold))
        {
            return BorderState::unknown();
        }

        let top = self.scan_top(frame, threshold);
        let bottom = self.scan_bottom(frame, threshold);
        let mut left = self.scan_left(frame, threshold);
        let right = self.scan_right(frame, threshold);
        let horizontal = top.min(bottom);
        left = left.min(right);

        if self.mode == 3 {
            left = 0;
        }

        BorderState {
            unknown: false,
            horizontal_size: horizontal,
            vertical_size: left,
        }
    }

    fn scan_top(&self, frame: FrameView<'_>, threshold: u8) -> usize {
        let mut top = 0usize;
        for y in 0..frame.height {
            if self.row_has_content(frame, y, threshold) {
                break;
            }
            top += 1;
        }
        top
    }

    fn scan_bottom(&self, frame: FrameView<'_>, threshold: u8) -> usize {
        let mut bottom = 0usize;
        for y in (0..frame.height).rev() {
            if self.row_has_content(frame, y, threshold) {
                break;
            }
            bottom += 1;
        }
        bottom
    }

    fn scan_left(&self, frame: FrameView<'_>, threshold: u8) -> usize {
        let mut left = 0usize;
        for x in 0..frame.width {
            if self.column_has_content(frame, x, threshold) {
                break;
            }
            left += 1;
        }
        left
    }

    fn scan_right(&self, frame: FrameView<'_>, threshold: u8) -> usize {
        let mut right = 0usize;
        for x in (0..frame.width).rev() {
            if self.column_has_content(frame, x, threshold) {
                break;
            }
            right += 1;
        }
        right
    }

    fn row_has_content(&self, frame: FrameView<'_>, y: usize, threshold: u8) -> bool {
        let row_start = y.saturating_mul(frame.width);
        for x in 0..frame.width {
            if !is_black(frame.pixel_at(row_start + x), threshold) {
                return true;
            }
        }
        false
    }

    fn column_has_content(&self, frame: FrameView<'_>, x: usize, threshold: u8) -> bool {
        for y in 0..frame.height {
            let idx = y.saturating_mul(frame.width).saturating_add(x);
            if !is_black(frame.pixel_at(idx), threshold) {
                return true;
            }
        }
        false
    }

    fn update_border(&mut self, detected: BorderState) {
        if border_equal(detected, self.previous_detected_border) {
            self.consistent_cnt = self.consistent_cnt.saturating_add(1);
            self.inconsistent_cnt = 0;
        } else {
            self.inconsistent_cnt = self.inconsistent_cnt.saturating_add(1);
            if self.inconsistent_cnt <= self.max_inconsistent_cnt {
                return;
            }
            self.previous_detected_border = detected;
            self.consistent_cnt = 0;
        }

        if border_equal(self.current_border, detected) {
            self.inconsistent_cnt = 0;
            return;
        }

        if detected.unknown {
            if self.consistent_cnt == self.unknown_switch_cnt {
                self.current_border = detected;
            }
        } else if self.current_border.unknown || self.consistent_cnt == self.border_switch_cnt {
            self.current_border = detected;
        }
    }
}

struct ScreenMirrorEffect {
    host: NativeHost,
    config: Config,
    pipeline: ColorPipeline,
    border_processor: BorderProcessor,
    width: usize,
    height: usize,
    previous: Vec<SkydimoRgb>,
    colors: Vec<SkydimoRgb>,
    blur_temp: Vec<SkydimoRgb>,
    blur_work: Vec<SkydimoRgb>,
    blur_out: Vec<SkydimoRgb>,
    blur_kernel: Vec<f32>,
    blur_kernel_radius: usize,
}

impl ScreenMirrorEffect {
    fn new(host: NativeHost) -> Self {
        let config = Config::default();
        Self {
            host,
            pipeline: ColorPipeline::new(&config),
            config,
            border_processor: BorderProcessor::default(),
            width: 0,
            height: 1,
            previous: Vec::new(),
            colors: Vec::new(),
            blur_temp: Vec::new(),
            blur_work: Vec::new(),
            blur_out: Vec::new(),
            blur_kernel: Vec::new(),
            blur_kernel_radius: usize::MAX,
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width as usize;
        self.height = if height == 0 { 1 } else { height as usize };
    }

    fn update_params(&mut self, json: &str) {
        let old_gamma = self.config.gamma;
        let old_brightness = self.config.brightness;
        let old_temperature = self.config.color_temperature;
        let old_red_calibration = self.config.red_calibration;
        let old_green_calibration = self.config.green_calibration;
        let old_blue_calibration = self.config.blue_calibration;

        if let Some(value) = parse_number_field(json, "smoothness") {
            self.config.smoothness = value.clamp(0.0, 100.0);
        }
        if let Some(value) = parse_number_field(json, "brightness") {
            self.config.brightness = value.clamp(0.0, 3.0);
        }
        if let Some(value) = parse_number_field(json, "saturation") {
            self.config.saturation = value.clamp(0.0, 3.0);
        }
        if let Some(value) = parse_number_field(json, "gamma") {
            self.config.gamma = value.clamp(0.1, 4.0);
        }
        if let Some(value) = parse_number_field(json, "colorTemperature") {
            self.config.color_temperature = value.round().clamp(2000.0, 10000.0);
        }
        if let Some(value) = parse_number_field(json, "redCalibration") {
            self.config.red_calibration = value.clamp(0.0, 2.0);
        }
        if let Some(value) = parse_number_field(json, "greenCalibration") {
            self.config.green_calibration = value.clamp(0.0, 2.0);
        }
        if let Some(value) = parse_number_field(json, "blueCalibration") {
            self.config.blue_calibration = value.clamp(0.0, 2.0);
        }
        if let Some(value) = parse_number_field(json, "blur") {
            self.config.blur = round_to_usize(value.clamp(0.0, 50.0));
        }
        if let Some(value) = parse_bool_field(json, "autoCrop") {
            self.config.auto_crop = value;
        }
        if let Some(value) = parse_number_field(json, "bbThreshold") {
            self.config.bb_threshold = value.clamp(0.0, 100.0);
        }
        if let Some(value) = parse_number_field(json, "bbMode") {
            self.config.bb_mode = round_to_i32(value);
        }
        if let Some(value) = parse_number_field(json, "bbBorderFrameCnt") {
            self.config.bb_border_frame_cnt = round_to_usize(value.clamp(0.0, 9999.0));
        }
        if let Some(value) = parse_number_field(json, "bbUnknownFrameCnt") {
            self.config.bb_unknown_frame_cnt = round_to_usize(value.clamp(0.0, 9999.0));
        }
        if let Some(value) = parse_number_field(json, "bbMaxInconsistentCnt") {
            self.config.bb_max_inconsistent_cnt = round_to_usize(value.clamp(0.0, 9999.0));
        }
        if let Some(value) = parse_number_field(json, "bbBlurRemoveCnt") {
            self.config.bb_blur_remove_cnt = round_to_usize(value.clamp(0.0, 9999.0));
        }

        if old_gamma != self.config.gamma
            || old_brightness != self.config.brightness
            || old_temperature != self.config.color_temperature
            || old_red_calibration != self.config.red_calibration
            || old_green_calibration != self.config.green_calibration
            || old_blue_calibration != self.config.blue_calibration
        {
            self.pipeline = ColorPipeline::new(&self.config);
        }
    }

    fn tick(&mut self, buffer: &mut [SkydimoRgb]) -> i32 {
        if buffer.is_empty() {
            return 0;
        }

        let (layout_w, layout_h) = self.layout_size(buffer.len());
        let cap_w = layout_w.clamp(CAPTURE_MIN_WIDTH, CAPTURE_MAX_WIDTH);
        let cap_h = layout_h.clamp(CAPTURE_MIN_HEIGHT, CAPTURE_MAX_HEIGHT);
        let Some(frame) = self.capture_screen(cap_w, cap_h) else {
            self.previous.clear();
            self.border_processor.reset_state();
            fill_black(buffer);
            return 0;
        };

        self.render(frame, buffer, layout_w, layout_h);
        0
    }

    fn layout_size(&self, led_count: usize) -> (usize, usize) {
        let mut width = self.width;
        let mut height = self.height.max(1);
        if width == 0 {
            width = led_count.max(1);
            height = 1;
        }
        if height <= 1 && width < led_count {
            width = led_count.max(1);
            height = 1;
        }
        (width.max(1), height.max(1))
    }

    fn capture_screen(&self, width: usize, height: usize) -> Option<FrameView<'static>> {
        let capture = self.host.screen_capture?;
        let mut frame = SkydimoRgbFrameV1::default();
        let status = unsafe { capture(self.host.host_ctx, width, height, &mut frame) };
        if status <= 0 || frame.pixels.is_null() || frame.width == 0 || frame.height == 0 {
            return None;
        }
        let total = frame.width.saturating_mul(frame.height).min(frame.pixels_len);
        if total == 0 {
            return None;
        }

        Some(FrameView {
            width: frame.width,
            height: frame.height,
            pixels: unsafe { std::slice::from_raw_parts(frame.pixels, total) },
        })
    }

    fn render(
        &mut self,
        frame: FrameView<'_>,
        buffer: &mut [SkydimoRgb],
        width: usize,
        height: usize,
    ) {
        let n = buffer.len();
        let previous_len = self.previous.len();
        if self.previous.len() < n {
            self.previous.resize(n, black());
        }
        if self.previous.len() > n {
            self.previous.truncate(n);
        }

        self.colors.clear();
        self.colors.resize(n, black());

        let crop = if self.config.auto_crop {
            self.border_processor.set_enabled(true);
            self.border_processor.configure(&self.config);
            self.border_processor.process_frame(frame);
            self.border_processor.crop_region_for(frame)
        } else {
            self.border_processor.set_enabled(false);
            Crop::default()
        };

        let mut idx = 0usize;
        for y in 0..height {
            let ry = if height == 1 {
                0.5
            } else {
                (y as f32 + 0.5) / height as f32
            };
            for x in 0..width {
                if idx >= n {
                    break;
                }
                let rx = if width == 1 {
                    0.5
                } else {
                    (x as f32 + 0.5) / width as f32
                };
                let target = self.sample(frame, rx, ry, crop);
                let prev = if idx < previous_len {
                    self.previous[idx]
                } else {
                    target
                };
                let out = smooth(prev, target, self.config.smoothness);
                self.previous[idx] = out;
                self.colors[idx] = out;
                idx += 1;
            }
        }

        if self.config.blur > 0 {
            self.apply_gaussian_blur(width, height, n);
            let update_len = self.colors.len().min(self.previous.len());
            self.previous[..update_len].copy_from_slice(&self.colors[..update_len]);
        }

        buffer.copy_from_slice(&self.colors[..n]);
    }

    fn sample(&self, frame: FrameView<'_>, ratio_x: f32, ratio_y: f32, crop: Crop) -> SkydimoRgb {
        let crop_left = crop.left.clamp(0.0, 0.45);
        let crop_right = crop.right.clamp(0.0, 0.45);
        let crop_top = crop.top.clamp(0.0, 0.45);
        let crop_bottom = crop.bottom.clamp(0.0, 0.45);

        let roi_w = (1.0 - crop_left - crop_right).max(0.1);
        let roi_h = (1.0 - crop_top - crop_bottom).max(0.1);
        let rx = (crop_left + ratio_x.clamp(0.0, 1.0) * roi_w).clamp(0.0, 1.0);
        let ry = (crop_top + ratio_y.clamp(0.0, 1.0) * roi_h).clamp(0.0, 1.0);

        let x = round_to_usize((frame.width.saturating_sub(1)) as f32 * rx).min(frame.width - 1);
        let y = round_to_usize((frame.height.saturating_sub(1)) as f32 * ry).min(frame.height - 1);
        let pixel = frame.pixel_at(y.saturating_mul(frame.width).saturating_add(x));
        self.pipeline.apply(pixel, self.config.saturation)
    }

    fn apply_gaussian_blur(&mut self, width: usize, height: usize, led_count: usize) {
        let radius = self.config.blur;
        if radius == 0 || led_count == 0 {
            return;
        }
        self.ensure_blur_kernel(radius);

        if height <= 1 {
            let len = width.min(led_count);
            self.blur_out.resize(len, black());
            blur_1d(&self.colors[..len], &self.blur_kernel, radius, &mut self.blur_out);
            self.colors.fill(black());
            self.colors[..len].copy_from_slice(&self.blur_out[..len]);
            return;
        }

        let total = width.saturating_mul(height);
        if total == 0 {
            return;
        }
        self.blur_temp.resize(total, black());
        self.blur_work.resize(total, black());
        self.blur_out.resize(total, black());

        let copy_len = total.min(led_count);
        self.blur_temp.fill(black());
        self.blur_temp[..copy_len].copy_from_slice(&self.colors[..copy_len]);

        gaussian_blur_matrix(
            &self.blur_temp,
            width,
            height,
            &self.blur_kernel,
            radius,
            &mut self.blur_work,
            &mut self.blur_out,
        );

        self.colors.fill(black());
        self.colors[..copy_len].copy_from_slice(&self.blur_out[..copy_len]);
    }

    fn ensure_blur_kernel(&mut self, radius: usize) {
        if self.blur_kernel_radius == radius && !self.blur_kernel.is_empty() {
            return;
        }
        self.blur_kernel_radius = radius;
        self.blur_kernel.clear();
        if radius == 0 {
            self.blur_kernel.push(1.0);
            return;
        }

        let sigma = radius as f32 * 0.5 + 0.5;
        let denom = 2.0 * sigma * sigma;
        let kernel_len = radius * 2 + 1;
        self.blur_kernel.reserve(kernel_len);

        let mut sum = 0.0f32;
        for idx in 0..kernel_len {
            let x = idx as isize - radius as isize;
            let value = (-((x * x) as f32) / denom).exp();
            self.blur_kernel.push(value);
            sum += value;
        }
        if sum > 0.0 {
            let inv = 1.0 / sum;
            for value in &mut self.blur_kernel {
                *value *= inv;
            }
        }
    }

    #[allow(dead_code)]
    fn log(&self, level: u32, message: &str) {
        if let Some(log) = self.host.log {
            unsafe {
                log(
                    self.host.host_ctx,
                    level,
                    message.as_ptr().cast::<c_char>(),
                    message.len(),
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
struct FrameView<'a> {
    width: usize,
    height: usize,
    pixels: &'a [SkydimoRgb],
}

impl FrameView<'_> {
    #[inline]
    fn pixel_at(&self, index: usize) -> SkydimoRgb {
        self.pixels.get(index).copied().unwrap_or_else(black)
    }
}

#[derive(Clone, Copy, Default)]
struct Crop {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

unsafe extern "C" fn screen_mirror_create(
    host: *const SkydimoHostApiV1,
    out_instance: *mut *mut c_void,
) -> i32 {
    catch_ffi(|| {
        if out_instance.is_null() || host.is_null() {
            return -1;
        }
        let host = unsafe { &*host };
        if host.abi_version < SKYDIMO_NATIVE_C_ABI_VERSION {
            return -2;
        }
        let effect = Box::new(ScreenMirrorEffect::new(NativeHost::from_api(host)));
        unsafe {
            *out_instance = Box::into_raw(effect).cast::<c_void>();
        }
        0
    })
}

unsafe extern "C" fn screen_mirror_destroy(instance: *mut c_void) {
    let _ = catch_ffi(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance.cast::<ScreenMirrorEffect>()));
            }
        }
        0
    });
}

unsafe extern "C" fn screen_mirror_resize(
    instance: *mut c_void,
    width: u32,
    height: u32,
    _led_count: u32,
) -> i32 {
    catch_ffi(|| {
        let Some(effect) = effect_mut(instance) else {
            return -1;
        };
        effect.resize(width, height);
        0
    })
}

unsafe extern "C" fn screen_mirror_update_params_json(
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

unsafe extern "C" fn screen_mirror_tick(
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
        let pixels = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
        effect.tick(pixels)
    })
}

unsafe extern "C" fn screen_mirror_is_ready(instance: *mut c_void) -> i32 {
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
                    create: Some(screen_mirror_create),
                    destroy: Some(screen_mirror_destroy),
                    resize: Some(screen_mirror_resize),
                    update_params_json: Some(screen_mirror_update_params_json),
                    tick: Some(screen_mirror_tick),
                    is_ready: Some(screen_mirror_is_ready),
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

fn effect_mut(instance: *mut c_void) -> Option<&'static mut ScreenMirrorEffect> {
    if instance.is_null() {
        None
    } else {
        Some(unsafe { &mut *instance.cast::<ScreenMirrorEffect>() })
    }
}

fn catch_ffi(f: impl FnOnce() -> i32) -> i32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(-100)
}

#[inline]
fn black() -> SkydimoRgb {
    SkydimoRgb { r: 0, g: 0, b: 0 }
}

fn fill_black(pixels: &mut [SkydimoRgb]) {
    pixels.fill(black());
}

#[inline]
fn is_black(pixel: SkydimoRgb, threshold: u8) -> bool {
    pixel.r < threshold && pixel.g < threshold && pixel.b < threshold
}

fn border_equal(a: BorderState, b: BorderState) -> bool {
    if a.unknown {
        return b.unknown;
    }
    if b.unknown {
        return false;
    }
    a.horizontal_size == b.horizontal_size && a.vertical_size == b.vertical_size
}

fn smooth(prev: SkydimoRgb, target: SkydimoRgb, smoothness: f32) -> SkydimoRgb {
    if smoothness <= 0.0 {
        return target;
    }
    if smoothness >= 100.0 {
        return prev;
    }

    let factor = (100.0 - smoothness) / 100.0;
    SkydimoRgb {
        r: smooth_channel(prev.r, target.r, factor),
        g: smooth_channel(prev.g, target.g, factor),
        b: smooth_channel(prev.b, target.b, factor),
    }
}

fn smooth_channel(prev: u8, target: u8, factor: f32) -> u8 {
    if prev == target {
        return target;
    }

    let value = prev as f32 + (target as f32 - prev as f32) * factor;
    let mut rounded = to_u8(value);
    if rounded == prev {
        rounded = if target > prev {
            prev.saturating_add(1)
        } else {
            prev.saturating_sub(1)
        };
    }
    if (target as f32 - rounded as f32).abs() <= 0.5 {
        target
    } else {
        rounded
    }
}

fn blur_1d(src: &[SkydimoRgb], kernel: &[f32], radius: usize, out: &mut [SkydimoRgb]) {
    if src.is_empty() || out.is_empty() {
        return;
    }
    let last = src.len() - 1;
    for (x, pixel) in out.iter_mut().enumerate() {
        let mut r = 0.0f32;
        let mut g = 0.0f32;
        let mut b = 0.0f32;
        for (k, weight) in kernel.iter().enumerate() {
            let offset = k as isize - radius as isize;
            let sx = (x as isize + offset).clamp(0, last as isize) as usize;
            let rgb = src[sx];
            r += rgb.r as f32 * *weight;
            g += rgb.g as f32 * *weight;
            b += rgb.b as f32 * *weight;
        }
        *pixel = SkydimoRgb {
            r: to_u8(r),
            g: to_u8(g),
            b: to_u8(b),
        };
    }
}

fn gaussian_blur_matrix(
    src: &[SkydimoRgb],
    width: usize,
    height: usize,
    kernel: &[f32],
    radius: usize,
    work: &mut [SkydimoRgb],
    out: &mut [SkydimoRgb],
) {
    let total = width
        .saturating_mul(height)
        .min(src.len())
        .min(work.len())
        .min(out.len());
    if total == 0 {
        return;
    }

    for y in 0..height {
        for x in 0..width {
            let idx = y.saturating_mul(width).saturating_add(x);
            if idx >= total {
                continue;
            }

            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;
            for (k, weight) in kernel.iter().enumerate() {
                let offset = k as isize - radius as isize;
                let sx = (x as isize + offset).clamp(0, width as isize - 1) as usize;
                let sample_idx = y.saturating_mul(width).saturating_add(sx);
                let rgb = src.get(sample_idx).copied().unwrap_or_else(black);
                r += rgb.r as f32 * *weight;
                g += rgb.g as f32 * *weight;
                b += rgb.b as f32 * *weight;
            }
            work[idx] = SkydimoRgb {
                r: to_u8(r),
                g: to_u8(g),
                b: to_u8(b),
            };
        }
    }

    for y in 0..height {
        for x in 0..width {
            let idx = y.saturating_mul(width).saturating_add(x);
            if idx >= total {
                continue;
            }

            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;
            for (k, weight) in kernel.iter().enumerate() {
                let offset = k as isize - radius as isize;
                let sy = (y as isize + offset).clamp(0, height as isize - 1) as usize;
                let sample_idx = sy.saturating_mul(width).saturating_add(x);
                let rgb = work.get(sample_idx).copied().unwrap_or_else(black);
                r += rgb.r as f32 * *weight;
                g += rgb.g as f32 * *weight;
                b += rgb.b as f32 * *weight;
            }
            out[idx] = SkydimoRgb {
                r: to_u8(r),
                g: to_u8(g),
                b: to_u8(b),
            };
        }
    }
}

fn color_temperature_gains(kelvin: f32) -> (f32, f32, f32) {
    let (neutral_r, neutral_g, neutral_b) = kelvin_to_rgb_scales(NEUTRAL_KELVIN);
    let (r, g, b) = kelvin_to_rgb_scales(kelvin.clamp(2000.0, 10000.0));
    (
        if neutral_r > 0.0 { r / neutral_r } else { 1.0 },
        if neutral_g > 0.0 { g / neutral_g } else { 1.0 },
        if neutral_b > 0.0 { b / neutral_b } else { 1.0 },
    )
}

fn kelvin_to_rgb_scales(kelvin: f32) -> (f32, f32, f32) {
    let temp = kelvin.clamp(1000.0, 40000.0) / 100.0;
    let (r, g, b) = if temp <= 66.0 {
        let r = 255.0;
        let g = 99.470_8 * temp.ln() - 161.119_57;
        let b = if temp <= 19.0 {
            0.0
        } else {
            138.517_73 * (temp - 10.0).ln() - 305.044_8
        };
        (r, g, b)
    } else {
        let r = 329.698_73 * (temp - 60.0).powf(-0.133_204_76);
        let g = 288.122_16 * (temp - 60.0).powf(-0.075_514_85);
        (r, g, 255.0)
    };

    (
        r.clamp(0.0, 255.0) / 255.0,
        g.clamp(0.0, 255.0) / 255.0,
        b.clamp(0.0, 255.0) / 255.0,
    )
}

fn parse_number_field(json: &str, key: &str) -> Option<f32> {
    json_value_slice(json, key)?.parse::<f32>().ok()
}

fn parse_bool_field(json: &str, key: &str) -> Option<bool> {
    let raw = json_value_slice(json, key)?;
    match raw {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
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

#[inline]
fn to_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

#[inline]
fn round_to_usize(value: f32) -> usize {
    (value + 0.5).floor().max(0.0) as usize
}

#[inline]
fn round_to_i32(value: f32) -> i32 {
    if value >= 0.0 {
        (value + 0.5).floor() as i32
    } else {
        (value - 0.5).ceil() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::{border_equal, smooth_channel, BorderState, ColorPipeline, Config, SkydimoRgb};

    #[test]
    fn border_unknown_matches_only_unknown() {
        assert!(border_equal(BorderState::unknown(), BorderState::unknown()));
        assert!(!border_equal(
            BorderState::unknown(),
            BorderState {
                unknown: false,
                horizontal_size: 0,
                vertical_size: 0,
            },
        ));
    }

    #[test]
    fn smoothing_moves_at_least_one_step() {
        assert_eq!(smooth_channel(0, 10, 0.01), 1);
        assert_eq!(smooth_channel(10, 0, 0.01), 9);
    }

    #[test]
    fn rgb_calibration_scales_channels() {
        let config = Config {
            red_calibration: 0.5,
            green_calibration: 1.0,
            blue_calibration: 2.0,
            ..Config::default()
        };
        let pipeline = ColorPipeline::new(&config);
        let out = pipeline.apply(
            SkydimoRgb {
                r: 100,
                g: 100,
                b: 100,
            },
            1.0,
        );

        assert_eq!(out.r, 50);
        assert_eq!(out.g, 100);
        assert_eq!(out.b, 200);
    }
}
