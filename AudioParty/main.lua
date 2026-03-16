local color   = require("lib.color")
local palette = require("lib.palette")
local effects = require("lib.effects")

local plugin = {}

-- ============================================================================
-- Configuration (matches manifest.json defaults)
-- ============================================================================

local config = {
  speed          = 50,
  colorSpeed     = 25,
  divisions      = 4,
  avgSize        = 8,
  effectThreshold = 20,
  motionZoneEnd  = 64,
  colorZoneEnd   = 192,
  useAlbumArt    = false,
}

-- ============================================================================
-- Animation state
-- ============================================================================

local state = {
  x_shift        = 0.0,
  color_shift    = 0.0,
  palette_time   = 0.0,
  effect_progress = 1.0,
  effect_idx     = 0,
}

local math_floor = math.floor
local math_max   = math.max
local math_abs   = math.abs
local math_sin   = math.sin
local math_random = math.random
local pi         = math.pi
local clamp      = color.clamp
local rgb_to_hsv = color.rgb_to_hsv
local fill_black = color.fill_black
local interpolate = color.interpolate
local sample_palette = palette.sample
local get_effect_color = effects.get_effect_color
local effect_blend     = effects.blend

-- ============================================================================
-- Per-pixel color computation (port of AudioParty::GetColor)
-- ============================================================================

local function get_color(pos_x, pos_y, w, h, pal)
  local nx = (w > 0) and (pos_x / w) or 0.0
  local s = 0.5 * (1.0 + math_sin(nx * config.divisions * pi + state.x_shift))

  -- Base color
  local br, bg, bb

  if config.useAlbumArt and pal then
    local pos_01 = 0.0
    if h > 0 then
      pos_01 = pos_y / h
    end

    local shift = math.fmod(state.color_shift / 360.0, 1.0)
    pos_01 = math.fmod(pos_01 + shift, 1.0)
    if pos_01 < 0.0 then pos_01 = pos_01 + 1.0 end

    local flow_phase = math.fmod(state.palette_time / 360.0, 1.0)
    local pr, pg, pb = sample_palette(pal, pos_01, flow_phase)
    -- Keep the palette color brightness driven by the sine wave
    local ph, ps, _ = rgb_to_hsv(pr, pg, pb)
    br, bg, bb = host.hsv_to_rgb(ph, ps, 1.0)
  else
    -- HSV color cycling driven by color_shift
    local ny = (h > 0) and (pos_y / h) or 0.0
    local hue = 180.0 + math_sin(ny + state.color_shift) * 180.0
    hue = hue % 360.0
    br, bg, bb = host.hsv_to_rgb(hue, 1.0, 1.0)
  end

  -- Triggered effect overlay
  local er, eg, eb = get_effect_color(
    state.effect_idx, state.effect_progress,
    pos_x, pos_y, w, h
  )
  br, bg, bb = effect_blend(br, bg, bb, er, eg, eb)

  -- Modulate by sine wave (interpolate with black)
  return interpolate(br, bg, bb, s)
end

-- ============================================================================
-- Zone processing: scan FFT bins in each zone (port of AudioParty::StepEffect)
-- ============================================================================

local function process_zones(bins, delta, color_delta)
  local avg = math_max(1, math_floor(config.avgSize))
  local motion_end = math_floor(config.motionZoneEnd)
  local color_end  = math_floor(config.colorZoneEnd)
  local threshold  = config.effectThreshold / 100.0

  -- Motion zone: drive x_shift
  local c = 0
  local i = 1
  while i <= motion_end do
    local cur = tonumber(bins[i]) or 0.0
    local nxt = tonumber(bins[i + avg]) or 0.0
    if cur > nxt then
      local mult = (c % 2 == 0) and 1.0 or -1.0
      state.x_shift = state.x_shift + cur * delta * mult
      break
    end
    c = c + 1
    i = i + avg
  end

  -- Color zone: drive color_shift
  c = 0
  i = motion_end + 1
  while i <= color_end do
    local cur = tonumber(bins[i]) or 0.0
    local nxt = tonumber(bins[i + avg]) or 0.0
    if cur > nxt then
      local mult = (c % 2 == 0) and 1.0 or -1.0
      state.color_shift = state.color_shift + cur * color_delta * mult
      break
    end
    c = c + 1
    i = i + avg
  end

  -- Effect zone: trigger random effects
  i = color_end + 1
  while i <= 256 do
    local cur = tonumber(bins[i]) or 0.0
    local nxt = tonumber(bins[i + avg]) or 0.0
    if cur > threshold and cur > nxt and state.effect_progress >= 1.0 then
      state.effect_idx = math_random(0, 6)
      state.effect_progress = 0.0
      break
    end
    i = i + avg
  end

  -- Advance effect progress
  if state.effect_progress < 1.0 then
    state.effect_progress = state.effect_progress + 0.1 * delta
  end
end

-- ============================================================================
-- Rendering: linear (1D) and matrix (2D)
-- ============================================================================

local function render_linear(buffer, n, pal)
  local w = n
  local h = 1.0
  for i = 1, n do
    local pos_x = (i - 1)
    local r, g, b = get_color(pos_x, 0.0, w, h, pal)
    buffer:set(i, r, g, b)
  end
end

local function render_matrix(buffer, n, width, height, pal)
  local idx = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if idx > n then return end
      local r, g, b = get_color(x, y, width, height, pal)
      buffer:set(idx, r, g, b)
      idx = idx + 1
    end
  end
end

-- ============================================================================
-- Plugin callbacks
-- ============================================================================

function plugin.on_init() end

function plugin.on_params(p)
  if type(p) ~= "table" then return end
  for k, v in pairs(p) do
    config[k] = v
  end
end

function plugin.on_tick(_, buffer, width, height)
  local n = buffer:len()
  if n <= 0 then return end

  if type(width) ~= "number" or width <= 0 then width = n end
  if type(height) ~= "number" or height <= 0 then height = 1 end

  -- Audio capture
  if not audio or type(audio.capture) ~= "function" then
    fill_black(buffer)
    return
  end

  local avgSize = clamp(math_floor(tonumber(config.avgSize) or 8), 1, 256)

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

  -- Time deltas (approximate 60 FPS)
  local speed       = tonumber(config.speed) or 50.0
  local colorSpeed  = tonumber(config.colorSpeed) or 25.0
  local delta       = speed / 60.0
  local color_delta = colorSpeed / 60.0

  -- Process the three FFT zones
  process_zones(bins, delta, color_delta)

  -- Palette from album art
  local pal = nil
  if config.useAlbumArt and media and type(media.album_art) == "function" then
    local art = media.album_art(64, 64)
    if art and art.pixels and art.width > 0 and art.height > 0 then
      pal = palette.get_cached(art)
    end
    state.palette_time = state.palette_time + speed / 60.0
  end

  -- Render
  local is_linear = height == 1 or width == 1
  if is_linear then
    render_linear(buffer, n, pal)
  else
    render_matrix(buffer, n, width, height, pal)
  end
end

function plugin.on_shutdown() end

return plugin
