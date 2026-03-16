local color   = require("lib.color")
local palette = require("lib.palette")

local plugin = {}

local config = {
  speed       = 50,
  avgSize     = 8,
  colorOffset = 180,
  colorSpread = 50,
  saturation  = 100,
  invertHue   = false,
  useAlbumArt = false,
}

local state = {
  peak_height  = 0.0,   -- peak indicator position [0, 1]
  palette_time = 0.0,   -- palette flow phase accumulator
}

local math_floor = math.floor
local math_max   = math.max
local math_min   = math.min
local math_abs   = math.abs
local clamp      = color.clamp
local rgb_to_hsv = color.rgb_to_hsv
local fill_black = color.fill_black
local sample_palette = palette.sample

-- ============================================================================
-- Color resolver: determines the color + brightness for a given VU position
-- ============================================================================

--- Get the display color for a single VU cell.
--- @param amp number current amplitude [0, 1]
--- @param pos number normalised position of this cell [0, 1] (0 = bottom)
--- @param pal table|nil palette extracted from album art
--- @return number r, number g, number b  (0-255 each)
local function get_color(amp, pos, pal)
  local peak = state.peak_height

  -- Determine brightness: lit below amplitude, peak indicator, off above
  local brightness
  if pos <= amp then
    brightness = 1.0
  elseif math_abs(pos - peak) < 0.02 and peak > 0.01 then
    -- peak indicator with soft falloff
    brightness = 1.0 - math_abs(pos - peak) / 0.02
  else
    return 0, 0, 0
  end

  local sat = clamp((config.saturation or 100) / 100.0, 0.0, 1.0)

  if pal then
    local flow_phase = (state.palette_time / 360.0) % 1.0
    local pr, pg, pb = sample_palette(pal, pos, flow_phase)
    local ph, _, _ = rgb_to_hsv(pr, pg, pb)
    return host.hsv_to_rgb(ph, sat, brightness)
  end

  -- Rainbow hue based on position
  local offset = config.colorOffset or 180
  local spread = (config.colorSpread or 50) * 0.01
  local hue = (offset + pos * 360.0 * spread) % 360.0
  if config.invertHue then
    hue = (360.0 - hue) % 360.0
  end

  return host.hsv_to_rgb(hue, sat, brightness)
end

-- ============================================================================
-- Linear mode: single strip VU meter (mirrored from center)
-- ============================================================================

local function tick_linear(buffer, n, amp, pal)
  for i = 1, n do
    -- Mirror from center: both halves grow outward
    local t = (i - 1) / math_max(n - 1, 1)
    local pos = math_abs(t - 0.5) * 2.0  -- 0 at center, 1 at edges → invert
    pos = 1.0 - pos                        -- 1 at center, 0 at edges

    local r, g, b = get_color(amp, pos, pal)
    buffer:set(i, r, g, b)
  end
end

-- ============================================================================
-- Matrix mode: each column is an independent VU bar (bottom to top)
-- ============================================================================

local function tick_matrix(buffer, n, width, height, amp, pal)
  local h = height - 1
  local i = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if i > n then return end
      -- y=0 is top row, y=height-1 is bottom row
      -- VU fills from bottom up, so invert: pos = 1 at bottom, 0 at top
      local pos = (h > 0) and ((h - y) / h) or 1.0

      local r, g, b = get_color(amp, pos, pal)
      buffer:set(i, r, g, b)
      i = i + 1
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
  local amp = tonumber(frame.amplitude) or 0.0
  if type(bins) ~= "table" then
    fill_black(buffer)
    return
  end

  amp = clamp(amp, 0.0, 1.0)

  -- Peak indicator: decay slowly, jump up instantly
  local speed = tonumber(config.speed) or 50.0
  local decay_rate = 0.05 * speed / 60.0

  if state.peak_height > amp then
    state.peak_height = math_max(0.0, state.peak_height - decay_rate)
  else
    state.peak_height = amp
  end

  -- Palette extraction from album art
  local pal = nil
  if config.useAlbumArt and media and type(media.album_art) == "function" then
    local art = media.album_art(64, 64)
    if art and art.pixels and art.width > 0 and art.height > 0 then
      pal = palette.get_cached(art)
    end
  end

  local is_linear = height == 1 or width == 1
  if is_linear then
    tick_linear(buffer, n, amp, pal)
  else
    tick_matrix(buffer, n, width, height, amp, pal)
  end

  state.palette_time = (state.palette_time + speed / 60.0) % 360.0
end

function plugin.on_shutdown() end

return plugin
