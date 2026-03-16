local color   = require("lib.color")
local palette = require("lib.palette")

local plugin = {}

local config = {
  speed            = 50,
  avgSize          = 8,
  -- "repeat" is a Lua keyword; accessed via config["repeat"]
  ["repeat"]       = 1,
  thickness        = 10,
  glow             = 50,
  oscillation      = 0,
  colorMode        = 0,      -- 0 = spectrum_cycle, 1 = static
  colorChangeSpeed = 50,
  useAlbumArt      = false,
  backgroundColor  = "#000000",
  waveColor        = "#00FF00",
}

local state = {
  x_time           = 0,
  oscillation_time = 0,
  color_time       = 0,
  palette_time     = 0,
}

-- Localise hot-path functions
local math_floor = math.floor
local math_abs   = math.abs
local math_sin   = math.sin
local math_max   = math.max
local pi         = math.pi
local clamp      = color.clamp
local hex_to_rgb = color.hex_to_rgb
local rgb_to_hsv = color.rgb_to_hsv
local lerp_color = color.lerp_color
local fill_black = color.fill_black
local sample_palette = palette.sample

-- ============================================================================
-- Composite sine from FFT bins
-- ============================================================================

local function get_sine_value(x, width, bins, avg_size, height_mult)
  local rep = tonumber(config["repeat"]) or 1
  local xp  = (x + 1 + state.x_time) / (width + 1)

  local value = 0
  for i = 0, 255, avg_size do
    local bin_val = tonumber(bins[i + 1]) or 0
    value = value
          + height_mult
          * bin_val
          * math_sin(xp * 0.25 * rep * (i / avg_size) * pi)
  end
  return value
end

-- ============================================================================
-- Per-pixel colour from sine distance
-- ============================================================================

local function get_color(sine, y, height, x_percent, pal)
  local half_h = height * 0.5
  local peak   = half_h + sine * half_h
  local real_d = math_abs(peak - y)

  local thick_threshold = 0.01 * (tonumber(config.thickness) or 10) * height
  local glow_exp        = 0.01 * (tonumber(config.glow) or 50)

  local distance
  if real_d > thick_threshold then
    distance = (real_d / height) ^ glow_exp
  else
    distance = 0
  end
  distance = clamp(distance, 0, 1)

  local bg_r, bg_g, bg_b = hex_to_rgb(config.backgroundColor)
  local mode = tonumber(config.colorMode) or 0
  local r, g, b

  if mode == 0 then
    -- Spectrum cycle / album palette
    if config.useAlbumArt and pal then
      local flow = (state.palette_time / 360) % 1
      local pr, pg, pb = sample_palette(pal, x_percent, flow)
      local ph, ps, _  = rgb_to_hsv(pr, pg, pb)
      r, g, b = host.hsv_to_rgb(ph, ps, 1 - distance)
    else
      local hue = state.color_time % 360
      r, g, b = host.hsv_to_rgb(hue, 1.0, 1 - distance)
    end
  else
    -- Static wave colour / album palette
    local wr, wg, wb
    if config.useAlbumArt and pal then
      local flow = (state.palette_time / 360) % 1
      wr, wg, wb = sample_palette(pal, x_percent, flow)
    else
      wr, wg, wb = hex_to_rgb(config.waveColor)
    end
    local factor = clamp(1 - distance, 0, 1)
    r = math_floor(wr * factor + 0.5)
    g = math_floor(wg * factor + 0.5)
    b = math_floor(wb * factor + 0.5)
  end

  -- Blend towards background by distance
  r, g, b = lerp_color(r, g, b, bg_r, bg_g, bg_b, distance)
  return r, g, b
end

-- ============================================================================
-- Linear (1-D strip) renderer
-- ============================================================================

local function tick_linear(buffer, n, bins, avg_size, height_mult, pal)
  local n1 = math_max(n - 1, 1)
  for i = 1, n do
    local x    = i - 1
    local sine = get_sine_value(x, n, bins, avg_size, height_mult)
    local xp   = x / n1
    local r, g, b = get_color(sine, 0, 1, xp, pal)
    buffer:set(i, r, g, b)
  end
end

-- ============================================================================
-- Matrix (2-D) renderer
-- ============================================================================

local function tick_matrix(buffer, n, width, height, bins, avg_size, height_mult, pal)
  local w1 = math_max(width - 1, 1)
  local idx = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if idx > n then return end
      local sine = get_sine_value(x, width, bins, avg_size, height_mult)
      local xp   = x / w1
      local r, g, b = get_color(sine, y, height, xp, pal)
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

  if type(width)  ~= "number" or width  <= 0 then width  = n end
  if type(height) ~= "number" or height <= 0 then height = 1 end

  -- Audio capture
  if not audio or type(audio.capture) ~= "function" then
    fill_black(buffer)
    return
  end

  local avg_size = math_floor(tonumber(config.avgSize) or 8)
  avg_size = clamp(avg_size, 1, 256)

  local frame = audio.capture(avg_size)
  if not frame or type(frame) ~= "table" then
    fill_black(buffer)
    return
  end

  local bins = frame.bins
  if type(bins) ~= "table" then
    fill_black(buffer)
    return
  end

  -- Oscillation amplitude modulation
  local osc = tonumber(config.oscillation) or 0
  local height_mult = (osc > 0) and math_sin(state.oscillation_time * 0.1) or 1

  -- Album art palette
  local pal = nil
  if config.useAlbumArt and media and type(media.album_art) == "function" then
    local art = media.album_art(64, 64)
    if art and art.pixels and art.width > 0 and art.height > 0 then
      pal = palette.get_cached(art)
    end
  end

  -- Render
  local is_linear = (height == 1) or (width == 1)
  if is_linear then
    tick_linear(buffer, n, bins, avg_size, height_mult, pal)
  else
    tick_matrix(buffer, n, width, height, bins, avg_size, height_mult, pal)
  end

  -- Advance time accumulators (assume ~60 fps tick rate)
  local speed     = tonumber(config.speed) or 50
  local clr_speed = tonumber(config.colorChangeSpeed) or 50

  state.x_time           = state.x_time + speed / 60
  state.oscillation_time = state.oscillation_time + osc / 60
  state.color_time       = (state.color_time + clr_speed / 60) % 360

  if config.useAlbumArt then
    state.palette_time = (state.palette_time + clr_speed / 60) % 360
  end
end

function plugin.on_shutdown() end

return plugin
