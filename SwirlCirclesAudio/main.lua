local color = require("lib.color")

local plugin = {}

-- ============================================================================
-- Localised hot-path functions
-- ============================================================================

local math_floor = math.floor
local math_sqrt  = math.sqrt
local math_sin   = math.sin
local math_cos   = math.cos
local math_pow   = math.pow

local clamp        = color.clamp
local screen_blend = color.screen_blend
local hex_to_rgb   = color.hex_to_rgb
local rgb_to_hsv   = color.rgb_to_hsv
local fill_black   = color.fill_black

local DEFAULT_CUSTOM_COLORS = { "#FF0000", "#00FF00" }

-- ============================================================================
-- Configuration (matches manifest.json defaults)
-- ============================================================================

local config = {
  speed     = 50,
  glow      = 50,
  radius    = 0,
  avgSize   = 8,
  colorMode = 0,       -- 0 = random cycle, 1 = custom color pair
  colors    = { DEFAULT_CUSTOM_COLORS[1], DEFAULT_CUSTOM_COLORS[2] },
}

-- ============================================================================
-- Animation state
-- ============================================================================

local progress      = 0.0
local current_level = 0.0

-- HSV state for the two circles (h = degrees 0-360, s/v = 0-1)
local hsv1_h, hsv1_s, hsv1_v = 0, 1.0, 1.0
local hsv2_h, hsv2_s, hsv2_v = 180, 1.0, 1.0

-- ============================================================================
-- Color helpers
-- ============================================================================

local function normalize_custom_colors(colors)
  local normalized = { DEFAULT_CUSTOM_COLORS[1], DEFAULT_CUSTOM_COLORS[2] }

  if type(colors) == "table" then
    if type(colors[1]) == "string" then
      normalized[1] = colors[1]
    end
    if type(colors[2]) == "string" then
      normalized[2] = colors[2]
    end
  end

  return normalized
end

local function reset_custom_colors()
  config.colors = normalize_custom_colors(config.colors)

  local r1, g1, b1 = hex_to_rgb(config.colors[1])
  hsv1_h, hsv1_s, hsv1_v = rgb_to_hsv(r1, g1, b1)

  local r2, g2, b2 = hex_to_rgb(config.colors[2])
  hsv2_h, hsv2_s, hsv2_v = rgb_to_hsv(r2, g2, b2)
end

local function reset_random_colors()
  hsv1_h = 0;   hsv1_s = 1.0; hsv1_v = 1.0
  hsv2_h = 180; hsv2_s = 1.0; hsv2_v = 1.0
end

-- ============================================================================
-- Per-pixel colour (port of SwirlCirclesAudio::GetColor)
--
-- Two circles orbit around the centre. Each pixel's brightness is driven by
-- its distance to each circle centre, modulated by the current audio level.
-- The two colours are composited with a Screen blend.
-- ============================================================================

local function get_color(x, y, w, h, x1, y1, radius, glow_mult)
  -- ---- Circle 1 ----
  local dx1 = x1 - x
  local dy1 = y1 - y
  local distance1 = math_sqrt(dx1 * dx1 + dy1 * dy1)

  local dist1_pct
  if distance1 < radius then
    -- Inside the core → brightness inversely proportional to audio level
    dist1_pct = 1.0 / (0.000001 + current_level)
  else
    -- Outside → glow falloff scaled by audio level
    dist1_pct = math_pow(distance1 / (h + w), glow_mult * current_level)
  end

  local v1 = clamp(hsv1_v * (1.0 - dist1_pct), 0.0, 1.0)
  local r1, g1, b1 = host.hsv_to_rgb(hsv1_h % 360, hsv1_s, v1)

  -- ---- Circle 2 (diametrically opposite) ----
  local x2 = w - x1
  local y2 = h - y1
  local dx2 = x2 - x
  local dy2 = y2 - y
  local distance2 = math_sqrt(dx2 * dx2 + dy2 * dy2)

  local dist2_pct
  if distance2 < radius then
    dist2_pct = 1.0 / (0.000001 + current_level)
  else
    dist2_pct = math_pow(distance2 / (h + w), glow_mult * current_level)
  end

  local v2 = clamp(hsv2_v * (1.0 - dist2_pct), 0.0, 1.0)
  local r2, g2, b2 = host.hsv_to_rgb(hsv2_h % 360, hsv2_s, v2)

  -- ---- Screen blend ----
  return screen_blend(r1, r2),
         screen_blend(g1, g2),
         screen_blend(b1, b2)
end

-- ============================================================================
-- Plugin callbacks
-- ============================================================================

function plugin.on_init()
  if config.colorMode == 0 then
    reset_random_colors()
  else
    reset_custom_colors()
  end
end

function plugin.on_params(p)
  if type(p) ~= "table" then return end

  local mode_changed   = false
  local colors_changed = false

  if p.colorMode ~= nil and p.colorMode ~= config.colorMode then
    mode_changed = true
  end

  for k, v in pairs(p) do
    if k ~= "colors" and k ~= "color1" and k ~= "color2" then
      config[k] = v
    end
  end

  if type(p.colors) == "table" then
    config.colors = normalize_custom_colors(p.colors)
    colors_changed = true
  elseif type(p.color1) == "string" or type(p.color2) == "string" then
    config.colors = normalize_custom_colors({
      type(p.color1) == "string" and p.color1 or config.colors[1],
      type(p.color2) == "string" and p.color2 or config.colors[2],
    })
    colors_changed = true
  end

  if mode_changed then
    if config.colorMode == 0 then
      reset_random_colors()
    else
      reset_custom_colors()
    end
  elseif colors_changed and config.colorMode == 1 then
    reset_custom_colors()
  end
end

function plugin.on_tick(_, buffer, width, height)
  local n = buffer:len()
  if n <= 0 then return end

  if type(width)  ~= "number" or width  <= 0 then width  = n end
  if type(height) ~= "number" or height <= 0 then height = 1 end

  -- ---- Audio capture ----
  if not audio or type(audio.capture) ~= "function" then
    fill_black(buffer)
    return
  end

  local avg_size = clamp(math_floor(tonumber(config.avgSize) or 8), 1, 256)

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

  -- Sum all FFT bins → current audio energy
  -- (original: for i=0..255: current_level += fft_fltr[i])
  current_level = 0.0
  for i = 1, 256 do
    current_level = current_level + (tonumber(bins[i]) or 0.0)
  end

  -- ---- Derived parameters ----
  local speed     = tonumber(config.speed)  or 50
  local glow      = tonumber(config.glow)   or 50
  local radius    = tonumber(config.radius) or 0
  local glow_mult = 0.001 * glow   -- original: 0.001 * Slider2Val

  -- Circle orbit positions
  local hx = 0.5 * width
  local hy = 0.5 * height
  local x1 = hx + hx * math_cos(progress)
  local y1 = hy + hy * math_sin(progress)

  -- ---- Render ----
  if height == 1 or width == 1 then
    -- Linear (1-D strip)
    for i = 1, n do
      local r, g, b = get_color(i - 1, 0, width, height, x1, y1, radius, glow_mult)
      buffer:set(i, r, g, b)
    end
  else
    -- Matrix (2-D)
    local idx = 1
    for y = 0, height - 1 do
      for x = 0, width - 1 do
        if idx > n then goto done end
        local r, g, b = get_color(x, y, width, height, x1, y1, radius, glow_mult)
        buffer:set(idx, r, g, b)
        idx = idx + 1
      end
    end
    ::done::
  end

  -- Advance rotation (original: progress += 0.1 * Speed / FPS)
  progress = progress + 0.1 * speed / 60.0

  -- Random-colour mode: hue advances 1°/frame (original: hsv.hue++)
  if config.colorMode == 0 then
    hsv1_h = (hsv1_h + 1) % 360
    hsv2_h = (hsv2_h + 1) % 360
  end
end

function plugin.on_shutdown() end

return plugin
