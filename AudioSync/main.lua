local color     = require("lib.color")
local palette   = require("lib.palette")
local edge_beat = require("lib.edge_beat")

local plugin = {}

-- ============================================================================
-- Configuration & State
-- ============================================================================

local config = {
  hueShift       = 0,
  fadeSpeed       = 50,
  saturationMode = 0,        -- 0=normal, 1=saturate_high, 2=bw
  rollMode       = 0,        -- 0=linear, 1=none, 2=radial, 3=wave, 4=vertical
  avgSize        = 8,
  bandpassRange  = { 0, 255 },
  bandpassMin    = 0,
  bandpassMax    = 255,
  useAlbumArt    = false,
  silentColor    = false,
  silentColorValue = "#000000",
  edgeBeat            = false,
  edgeBeatHue         = 0,
  edgeBeatSaturation  = 0,
  edgeBeatSensitivity = 100,
}

-- Tracking state for smooth hue transitions and color history
local state = {
  current_hue    = 0.0,   -- smoothed hue
  current_sat    = 255.0, -- smoothed saturation
  current_val    = 0.0,   -- smoothed brightness
  palette_time   = 0.0,   -- album art flow animation
  silent_timer   = 0,     -- frames since last sound
  colors         = {},    -- rotation buffer (max 1024)
  colors_count   = 0,
}

local SILENT_TIMEOUT = 120 -- ~2 seconds at 60fps
local MAX_COLORS     = 1024
local FPS_REF        = 60.0

local math_floor = math.floor
local math_ceil  = math.ceil
local math_max   = math.max
local math_min   = math.min
local math_abs   = math.abs
local math_sqrt  = math.sqrt
local math_pow   = math.pow
local clamp      = color.clamp
local rgb_to_hsv = color.rgb_to_hsv
local fill_black = color.fill_black
local lerp_color = color.lerp_color
local screen_blend = color.screen_blend
local unpack_rgb = color.unpack_rgb

-- ============================================================================
-- Pre-computed rainbow hue lookup (360 → 0, 256 entries)
-- ============================================================================

local rainbow_hues = {}
do
  local start_hue = 360
  local stop_hue  = 0
  local step = (stop_hue - start_hue) / 256.0
  for i = 0, 255 do
    rainbow_hues[i + 1] = math_ceil(start_hue + i * step)
  end
end

-- ============================================================================
-- Parse hex color string → r,g,b
-- ============================================================================

local function parse_hex_color(hex)
  if type(hex) ~= "string" then return 0, 0, 0 end
  hex = hex:gsub("^#", "")
  if #hex ~= 6 then return 0, 0, 0 end
  local r = tonumber(hex:sub(1, 2), 16) or 0
  local g = tonumber(hex:sub(3, 4), 16) or 0
  local b = tonumber(hex:sub(5, 6), 16) or 0
  return r, g, b
end

-- ============================================================================
-- Pack RGB to single integer for color buffer
-- ============================================================================

local function pack_rgb(r, g, b)
  return r * 65536 + g * 256 + b
end

local function normalize_bandpass_range(value)
  local min_value = config.bandpassMin
  local max_value = config.bandpassMax

  if type(value) == "table" then
    min_value = math_floor(tonumber(value[1]) or min_value or 0)
    max_value = math_floor(tonumber(value[2]) or max_value or 255)
  end

  min_value = clamp(min_value or 0, 0, 255)
  max_value = clamp(max_value or 255, 0, 255)

  if min_value > max_value then
    min_value, max_value = max_value, min_value
  end

  config.bandpassMin = min_value
  config.bandpassMax = max_value
  config.bandpassRange = { min_value, max_value }
end

-- ============================================================================
-- Core audio analysis: find dominant frequency and compute color
-- ============================================================================

local function analyze_audio(bins, fps)
  local bp_min = math_floor(tonumber(config.bandpassMin) or 0)
  local bp_max = math_floor(tonumber(config.bandpassMax) or 255)
  bp_min = clamp(bp_min, 0, 255)
  bp_max = clamp(bp_max, 0, 255)
  if bp_min > bp_max then bp_min, bp_max = bp_max, bp_min end

  local hue_shift = math_floor(tonumber(config.hueShift) or 0)
  local fade_speed = tonumber(config.fadeSpeed) or 50
  local sat_mode = math_floor(tonumber(config.saturationMode) or 0)

  -- Find peak frequency in bandpass range
  local max_idx = -1
  local max_value = 0.0
  for i = bp_min, bp_max do
    local v = tonumber(bins[i + 1]) or 0.0
    if v > max_value then
      max_value = v
      max_idx = i
    end
  end

  if max_idx >= bp_min and max_idx <= bp_max then
    -- Map peak frequency to hue via rainbow lookup
    local shifted = ((max_idx + hue_shift) % 256) + 1
    local immediate_hue = rainbow_hues[shifted] or 0

    -- Smooth hue transition (lerp toward target)
    local divisor = (1.0 - fade_speed / 100.0)
    if divisor < 0.01 then divisor = 0.01 end
    local delta = (immediate_hue - state.current_hue) / divisor / fps
    state.current_hue = state.current_hue + delta

    -- Wrap hue into [0, 360)
    state.current_hue = state.current_hue % 360
    if state.current_hue < 0 then
      state.current_hue = state.current_hue + 360
    end

    -- Saturation mode
    if sat_mode == 1 then
      -- Saturate high amplitudes: inverse cubic
      if max_value <= 0 then
        if state.current_sat > 1 then
          state.current_sat = state.current_sat + 1.0 / (state.current_sat * fps)
        end
      else
        state.current_sat = 255 - 255 * math_pow(max_value, 3)
      end
    elseif sat_mode == 2 then
      -- Black & white
      state.current_sat = 0
    else
      -- Normal: full saturation
      state.current_sat = 255
    end

    -- Brightness from amplitude
    state.current_val = max_value * 255
  else
    -- No peak found: decay
    if state.current_sat > 1 then
      state.current_sat = state.current_sat - 1.0 / (state.current_sat * fps)
    end
    if state.current_val > 1 then
      state.current_val = state.current_val - 1.0 / (state.current_val * fps)
    end
  end

  state.current_sat = clamp(state.current_sat, 0, 255)
  state.current_val = clamp(state.current_val, 0, 255)

  return max_idx, max_value
end

-- ============================================================================
-- Produce the dominant color for this frame
-- ============================================================================

local function compute_frame_color(bins, max_idx, max_value, pal)
  local h = state.current_hue
  local s = state.current_sat / 255.0
  local v = state.current_val / 255.0

  local r, g, b

  if config.useAlbumArt and pal and max_idx and max_idx >= 0 then
    -- Album art palette: use frequency position to sample palette
    local pos = max_idx / 256.0
    local flow_phase = (state.palette_time / 360.0) % 1.0
    local pr, pg, pb = palette.sample(pal, pos, flow_phase)
    -- Preserve real-time saturation and value from analysis
    local ph, ps, _ = rgb_to_hsv(pr, pg, pb)
    r, g, b = host.hsv_to_rgb(ph, s, v)
  else
    r, g, b = host.hsv_to_rgb(h, s, v)
  end

  -- Silent color fallback
  local is_silent = (r == 0 and g == 0 and b == 0)
  if is_silent and config.silentColor then
    state.silent_timer = math_min(state.silent_timer + 1, SILENT_TIMEOUT)
    local sr, sg, sb = parse_hex_color(config.silentColorValue)
    local t = state.silent_timer / SILENT_TIMEOUT
    r = math_floor(sr * t + 0.5)
    g = math_floor(sg * t + 0.5)
    b = math_floor(sb * t + 0.5)
  else
    state.silent_timer = 0
  end

  return r, g, b
end

-- ============================================================================
-- Roll mode renderers: map LED spatial position → color from rotation buffer
-- ============================================================================

local function get_color_from_buffer(idx)
  idx = clamp(idx, 0, state.colors_count - 1)
  local packed = state.colors[idx + 1]
  if not packed then return 0, 0, 0 end
  return unpack_rgb(packed)
end

local function render_roll(buffer, n, width, height)
  local roll = math_floor(tonumber(config.rollMode) or 0)
  local count = state.colors_count

  if count <= 0 then
    fill_black(buffer)
    return
  end

  local is_matrix = height > 1 and width > 1

  if roll == 1 then
    -- NONE: single color (latest)
    local r, g, b = get_color_from_buffer(0)
    for i = 1, n do
      local fr, fg, fb = r, g, b
      if config.edgeBeat and is_matrix then
        local row = math_floor((i - 1) / width)
        local col = (i - 1) % width
        if col == 0 or col == width - 1 or row == 0 or row == height - 1 then
          fr, fg, fb = edge_beat.apply(fr, fg, fb, state.last_bins, config)
        end
      elseif config.edgeBeat then
        local edge_zone = math_max(1, math_floor(n * 0.1))
        if i <= edge_zone or i > n - edge_zone then
          fr, fg, fb = edge_beat.apply(fr, fg, fb, state.last_bins, config)
        end
      end
      buffer:set(i, fr, fg, fb)
    end
    return
  end

  if not is_matrix then
    -- 1D modes: LINEAR uses position, others map index directly
    for i = 1, n do
      local idx
      if roll == 0 then
        -- LINEAR horizontal: LED position → buffer index
        idx = i - 1
      elseif roll == 4 then
        -- LINEAR2 vertical: reversed index
        idx = n - i
      else
        -- RADIAL/WAVE in 1D: distance from center
        local center = (n - 1) * 0.5
        local dist
        if roll == 3 then
          -- WAVE: distance from right end
          dist = math_abs((n - 1) * 1.1 - (i - 1))
        else
          dist = math_abs(center - (i - 1))
        end
        idx = math_floor(dist + 0.5)
      end

      local r, g, b = get_color_from_buffer(idx)

      if config.edgeBeat then
        local edge_zone = math_max(1, math_floor(n * 0.1))
        if i <= edge_zone or i > n - edge_zone then
          r, g, b = edge_beat.apply(r, g, b, state.last_bins, config)
        end
      end

      buffer:set(i, r, g, b)
    end
  else
    -- 2D matrix modes
    local w = width - 1
    local h = height - 1
    local cx = w * 0.5
    local cy = h * 0.5

    local i = 1
    for row = 0, height - 1 do
      for col = 0, width - 1 do
        if i > n then break end

        local idx
        if roll == 0 then
          -- LINEAR: column
          idx = col
        elseif roll == 4 then
          -- LINEAR2 vertical: reversed row
          idx = math_floor(w - row + 0.5)
        elseif roll == 2 then
          -- RADIAL: distance from center
          local dist = math_sqrt((cx - col) * (cx - col) + (cy - row) * (cy - row))
          idx = math_floor(dist + 0.5)
        elseif roll == 3 then
          -- WAVE: distance from right edge (offset 1.1x)
          local ox = w * 1.1
          local dist = math_sqrt((ox - col) * (ox - col) + (cy - row) * (cy - row))
          idx = math_floor(dist + 0.5)
        else
          idx = 0
        end

        local r, g, b = get_color_from_buffer(idx)

        if config.edgeBeat then
          if col == 0 or col == w or row == 0 or row == h then
            r, g, b = edge_beat.apply(r, g, b, state.last_bins, config)
          end
        end

        buffer:set(i, r, g, b)
        i = i + 1
      end
      if i > n then break end
    end
  end
end

-- ============================================================================
-- Plugin callbacks
-- ============================================================================

function plugin.on_init()
  state.colors = {}
  state.colors_count = 0
  state.current_hue = 0
  state.current_sat = 255
  state.current_val = 0
  state.palette_time = 0
  state.silent_timer = 0
  state.last_bins = {}
  normalize_bandpass_range(config.bandpassRange)
end

function plugin.on_params(p)
  if type(p) ~= "table" then return end

  local pending_bandpass_range = nil
  local legacy_bandpass_min = nil
  local legacy_bandpass_max = nil

  for k, v in pairs(p) do
    if k == "bandpassRange" then
      pending_bandpass_range = v
    elseif k == "bandpassMin" then
      legacy_bandpass_min = v
    elseif k == "bandpassMax" then
      legacy_bandpass_max = v
    else
      config[k] = v
    end
  end

  if pending_bandpass_range ~= nil then
    normalize_bandpass_range(pending_bandpass_range)
  elseif legacy_bandpass_min ~= nil or legacy_bandpass_max ~= nil then
    normalize_bandpass_range({
      legacy_bandpass_min ~= nil and legacy_bandpass_min or config.bandpassMin,
      legacy_bandpass_max ~= nil and legacy_bandpass_max or config.bandpassMax,
    })
  end
end

function plugin.on_tick(elapsed, buffer, width, height)
  local n = buffer:len()
  if n <= 0 then return end

  if type(width) ~= "number" or width <= 0 then width = n end
  if type(height) ~= "number" or height <= 0 then height = 1 end

  -- Check audio availability
  if not audio or type(audio.capture) ~= "function" then
    fill_black(buffer)
    return
  end

  local avgSize = math_floor(tonumber(config.avgSize) or 8)
  avgSize = clamp(avgSize, 1, 256)

  local frame = audio.capture(avgSize)
  if not frame or type(frame) ~= "table" then
    fill_black(buffer)
    return
  end

  local bins = frame.bins
  if type(bins) ~= "table" then
    fill_black(buffer)
    return
  end

  state.last_bins = bins

  -- Estimate FPS from elapsed (guard against zero/tiny values)
  local fps = FPS_REF
  if elapsed and elapsed > 0.001 then
    fps = 1.0 / elapsed
    fps = clamp(fps, 10, 240)
  end

  -- Album art palette
  local pal = nil
  if config.useAlbumArt and media and type(media.album_art) == "function" then
    local art = media.album_art(64, 64)
    if art and art.pixels and art.width > 0 and art.height > 0 then
      pal = palette.get_cached(art)
    end
  end

  -- Audio analysis: find dominant frequency, smooth hue/sat/val
  local max_idx, max_value = analyze_audio(bins, fps)

  -- Compute the dominant color for this frame
  local r, g, b = compute_frame_color(bins, max_idx, max_value, pal)

  -- Push new color into rotation buffer
  local packed = pack_rgb(r, g, b)
  table.insert(state.colors, 1, packed)
  state.colors_count = state.colors_count + 1

  -- Trim buffer
  while state.colors_count > MAX_COLORS do
    state.colors[state.colors_count] = nil
    state.colors_count = state.colors_count - 1
  end

  -- Update palette animation time
  if config.useAlbumArt then
    local fade_speed = tonumber(config.fadeSpeed) or 50
    state.palette_time = state.palette_time + fade_speed / fps
  end

  -- Render LEDs based on roll mode
  render_roll(buffer, n, width, height)
end

function plugin.on_shutdown() end

return plugin
