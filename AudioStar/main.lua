local color     = require("lib.color")
local palette   = require("lib.palette")
local edge_beat = require("lib.edge_beat")

local plugin = {}

local config = {
  speed = 50,
  avgSize = 8,
  useAlbumArt = false,
  edgeBeat = false,
  edgeBeatHue = 0,
  edgeBeatSaturation = 0,
  edgeBeatSensitivity = 100,
}

local state = {
  time = 0,
}

local math_floor = math.floor
local math_max   = math.max
local math_abs   = math.abs
local math_atan2 = math.atan2
local pi         = math.pi
local clamp = color.clamp
local rgb_to_hsv = color.rgb_to_hsv
local fill_black = color.fill_black
local sample_palette = palette.sample

-- ============================================================================
-- Resolve color for a LED position
-- ============================================================================

local function resolve_palette_color(brightness, pal, pos_01, flow_phase)
  local pr, pg, pb = sample_palette(pal, pos_01, flow_phase)
  local ph, ps, _ = rgb_to_hsv(pr, pg, pb)
  return host.hsv_to_rgb(ph, ps, brightness)
end

local function resolve_hue_color(brightness, hue)
  return host.hsv_to_rgb(hue, 1.0, brightness)
end

-- ============================================================================
-- Render modes
-- ============================================================================

local function tick_linear(buffer, n, bins, amp, pal)
  local exponent = 1.0 / (amp + 1.0)
  local flow_phase = (state.time / 360.0) % 1.0

  for i = 1, n do
    local t = (i - 1) / math_max(n - 1, 1)
    local mirror_t = math_abs(t - 0.5) * 2.0
    local angle = mirror_t * pi

    local binIndex = math_floor(256 * (angle / (pi * 2.0)))
    binIndex = clamp(binIndex, 0, 255)

    local freqAmp = tonumber(bins[binIndex + 1]) or 0.0
    local brightness = freqAmp ^ exponent
    brightness = clamp(brightness, 0.0, 1.0)

    local r, g, b
    if pal then
      local pos_01 = angle / pi  -- [0, 1.0] full palette range
      r, g, b = resolve_palette_color(brightness, pal, pos_01, flow_phase)
    else
      local hue = (t * 360.0 + state.time) % 360.0
      r, g, b = resolve_hue_color(brightness, hue)
    end

    if config.edgeBeat then
      local edge_zone = math_max(1, math_floor(n * 0.1))
      if i <= edge_zone or i > n - edge_zone then
        r, g, b = edge_beat.apply(r, g, b, bins, config)
      end
    end

    buffer:set(i, r, g, b)
  end
end

local function tick_matrix(buffer, n, width, height, bins, amp, pal)
  local w = width - 1
  local h = height - 1
  local cx = w * 0.5
  local cy = h * 0.5
  local exponent = 1.0 / (amp + 1.0)
  local flow_phase = (state.time / 360.0) % 1.0

  local i = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if i > n then break end

      local angle = math_abs(math_atan2(x - cx, y - cy))

      local binIndex = math_floor(256 * (angle / (pi * 2.0)))
      binIndex = clamp(binIndex, 0, 255)

      local freqAmp = tonumber(bins[binIndex + 1]) or 0.0
      local brightness = freqAmp ^ exponent
      brightness = clamp(brightness, 0.0, 1.0)

      local r, g, b
      if pal then
        local pos_01 = angle / pi  -- [0, 1.0] full palette range
        r, g, b = resolve_palette_color(brightness, pal, pos_01, flow_phase)
      else
        local hue = ((angle / pi) * 360.0 + state.time) % 360.0
        r, g, b = resolve_hue_color(brightness, hue)
      end

      if config.edgeBeat then
        if x <= 0 or x >= w or y <= 0 or y >= h then
          r, g, b = edge_beat.apply(r, g, b, bins, config)
        end
      end

      buffer:set(i, r, g, b)
      i = i + 1
    end
    if i > n then break end
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
    tick_linear(buffer, n, bins, amp, pal)
  else
    tick_matrix(buffer, n, width, height, bins, amp, pal)
  end

  local speed = tonumber(config.speed) or 50.0
  state.time = (state.time + speed / 60.0) % 360.0
end

function plugin.on_shutdown() end

return plugin
