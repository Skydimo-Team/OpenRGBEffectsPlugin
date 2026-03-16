local plugin = {}

-- ============================================================================
-- Math helpers
-- ============================================================================

local math_floor  = math.floor
local math_sqrt   = math.sqrt
local math_abs    = math.abs
local math_min    = math.min
local math_max    = math.max
local math_pow    = math.pow
local math_random = math.random

local FPS = 60.0

local function clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

local function to_int(v, fallback)
  local n = tonumber(v)
  if n == nil then return fallback end
  return math_floor(n + 0.5)
end

-- ============================================================================
-- Color helpers
-- ============================================================================

local function hex_to_rgb(hex)
  if type(hex) ~= "string" then return 255, 255, 255 end
  hex = hex:gsub("%s+", "")
  if hex:sub(1, 1) == "#" then
    hex = hex:sub(2)
  end
  if #hex == 3 then
    hex = hex:sub(1, 1):rep(2) .. hex:sub(2, 2):rep(2) .. hex:sub(3, 3):rep(2)
  end
  if #hex ~= 6 then return 255, 255, 255 end

  return tonumber(hex:sub(1, 2), 16) or 255,
         tonumber(hex:sub(3, 4), 16) or 255,
         tonumber(hex:sub(5, 6), 16) or 255
end

local function rgb_to_hsv(r, g, b)
  local rf, gf, bf = r / 255.0, g / 255.0, b / 255.0
  local maxc = math_max(rf, gf, bf)
  local minc = math_min(rf, gf, bf)
  local delta = maxc - minc
  local h, s, v = 0.0, 0.0, maxc

  if delta > 0 then
    if maxc == rf then
      h = 60.0 * (((gf - bf) / delta) % 6)
    elseif maxc == gf then
      h = 60.0 * (((bf - rf) / delta) + 2)
    else
      h = 60.0 * (((rf - gf) / delta) + 4)
    end

    if maxc > 0 then
      s = delta / maxc
    end
  end

  if h < 0 then h = h + 360.0 end
  return h, s, v
end

local function screen_blend_channel(a, b)
  local af = a / 255.0
  local bf = b / 255.0
  return math_floor((1.0 - (1.0 - af) * (1.0 - bf)) * 255.0)
end

-- ============================================================================
-- Presets (ported from OpenRGB AudioBubbles)
-- ============================================================================

local presets = {
  {
    "#FF0000", "#FF00E6", "#0000FF", "#00B3FF",
    "#00FF51", "#EAFF00", "#FFB300", "#FF0000"
  },
  {
    "#14E81E", "#00EA8D", "#017ED5", "#B53DFF", "#8D00C4", "#14E81E"
  },
  {
    "#00007F", "#0000FF", "#00FFFF", "#00AAFF", "#00007F"
  },
  {
    "#FE00C5", "#00C5FF", "#00C5FF", "#FE00C5"
  },
  {
    "#FEE000", "#FE00FE", "#FE00FE", "#FEE000"
  },
  {
    "#FF5500", "#000000", "#000000", "#000000", "#FF5500"
  },
  {
    "#FF2100", "#AA00FF", "#AA00FF", "#FF2100", "#FF2100", "#FF2100"
  },
  {
    "#03FFFA", "#55007F", "#55007F", "#03FFFA"
  },
  {
    "#FF0000", "#0000FF", "#0000FF", "#FF0000", "#FF0000"
  },
  {
    "#00FF00", "#0032FF", "#0032FF", "#00FF00", "#00FF00"
  },
  {
    "#FF2100", "#AB006D", "#C01C52", "#D53737",
    "#EA531B", "#FF6E00", "#FF0000", "#FF2100"
  },
  {
    "#FF71CE", "#B967FF", "#01CDFE", "#05FFA1", "#FFFB96", "#FF71CE"
  }
}

local function clone_colors(colors)
  local out = {}
  for i = 1, #colors do
    out[i] = colors[i]
  end
  return out
end

-- ============================================================================
-- Config/state
-- ============================================================================

local config = {
  -- Audio settings equivalent
  avgSize = 8,

  -- AudioBubbles settings
  preset = 0,
  colors = clone_colors(presets[1]),
  spawnMode = 0,
  trigger = 30,
  max_bubbles = 8,
  speed_mult = 1,
  max_expansion = 100,
  bubbles_thickness = 10,
}

-- Pre-parsed gradient colors (for fast sampling)
local gradient_colors = {}

-- Bubble list: { freq_id, amp, cx, cy, progress, speed }
local bubbles = {}

-- Shared zero FFT frame fallback
local zero_bins = {}
for i = 1, 256 do
  zero_bins[i] = 0.0
end

local function rebuild_gradient()
  gradient_colors = {}
  for i = 1, #config.colors do
    local r, g, b = hex_to_rgb(config.colors[i])
    gradient_colors[#gradient_colors + 1] = { r = r, g = g, b = b }
  end
  if #gradient_colors == 0 then
    gradient_colors[1] = { r = 255, g = 255, b = 255 }
  end
end

local function sample_gradient(freq_id)
  local count = #gradient_colors
  if count <= 1 then
    local c = gradient_colors[1]
    return c.r, c.g, c.b
  end

  local t = clamp((tonumber(freq_id) or 0) / 255.0, 0.0, 1.0)
  local segment = t * (count - 1)
  local i0 = math_floor(segment) + 1

  if i0 >= count then
    local c = gradient_colors[count]
    return c.r, c.g, c.b
  end

  local lt = segment - (i0 - 1)
  local c0 = gradient_colors[i0]
  local c1 = gradient_colors[i0 + 1]

  return math_floor(c0.r * (1.0 - lt) + c1.r * lt + 0.5),
         math_floor(c0.g * (1.0 - lt) + c1.g * lt + 0.5),
         math_floor(c0.b * (1.0 - lt) + c1.b * lt + 0.5)
end

local function apply_preset(idx)
  local preset = presets[idx + 1]
  if preset then
    config.colors = clone_colors(preset)
    rebuild_gradient()
  end
end

-- ============================================================================
-- Bubble simulation (ported from OpenRGB AudioBubbles)
-- ============================================================================

local function init_bubble(idx, amp)
  amp = clamp(tonumber(amp) or 0.0, 0.2, 0.8)

  local cx, cy
  local mode = config.spawnMode

  if mode == 1 then
    -- RANDOM_X
    cx = math_random()
    cy = 1.0 - (idx / 256.0)
  elseif mode == 2 then
    -- RANDOM_Y
    cx = idx / 256.0
    cy = math_random()
  elseif mode == 3 then
    -- CENTER
    cx, cy = 0.5, 0.5
  else
    -- RANDOM_XY
    cx = math_random()
    cy = math_random()
  end

  bubbles[#bubbles + 1] = {
    freq_id = idx,
    amp = amp,
    cx = cx,
    cy = cy,
    progress = 0.0,
    speed = 1.0 / amp,
  }
end

local function expand_bubbles()
  local speed_mult = config.speed_mult
  for i = 1, #bubbles do
    local bubble = bubbles[i]
    bubble.progress = bubble.progress + (0.1 * speed_mult * bubble.speed / FPS)
  end
end

local function trigger_bubbles(bins)
  local trigger_value = 0.01 * config.trigger
  local avg_size = config.avgSize

  local indexed_fft = {}

  for i = 0, 255, avg_size do
    local amp = tonumber(bins[i + 1]) or 0.0
    if amp >= trigger_value then
      indexed_fft[#indexed_fft + 1] = { amp = amp, idx = i }
    end
  end

  table.sort(indexed_fft, function(l, r)
    return l.amp > r.amp
  end)

  local occupied = {}
  for i = 1, #bubbles do
    occupied[bubbles[i].freq_id] = true
  end

  for i = 1, #indexed_fft do
    if #bubbles >= config.max_bubbles then
      break
    end

    local p = indexed_fft[i]
    if not occupied[p.idx] then
      init_bubble(p.idx, p.amp)
      occupied[p.idx] = true
    end
  end
end

local function cleanup_bubbles()
  local kept = {}
  for i = 1, #bubbles do
    local bubble = bubbles[i]
    if bubble.progress < (config.max_expansion * bubble.amp) then
      kept[#kept + 1] = bubble
    end
  end
  bubbles = kept
end

-- ============================================================================
-- Pixel shader (ported from OpenRGB AudioBubbles::GetColor)
-- ============================================================================

local function get_color(x, y, w, h)
  local pr, pg, pb = 0, 0, 0

  for i = 1, #bubbles do
    local bubble = bubbles[i]

    local dx = w * bubble.cx - x
    local dy = h * bubble.cy - y
    local distance = math_sqrt(dx * dx + dy * dy)

    local denom = 0.1 * config.bubbles_thickness * bubble.amp
    if denom > 0 then
      local shallow = math_abs(distance - bubble.progress) / denom

      local value
      if shallow <= 1e-9 then
        value = 255.0
      else
        value = math_min(255.0, 255.0 * (1.0 / math_pow(shallow, 3)))
      end

      local progress_norm = math_min(1.0, bubble.progress / (config.max_expansion * bubble.amp))

      if value > 0 and progress_norm > 0 then
        local gr, gg, gb = sample_gradient(bubble.freq_id)
        local bh, bs, _ = rgb_to_hsv(gr, gg, gb)
        local bv = (value / 255.0) * math_pow(1.0 - progress_norm, 0.5)

        local br, bg, bb = host.hsv_to_rgb(bh, bs, bv)

        pr = screen_blend_channel(pr, br)
        pg = screen_blend_channel(pg, bg)
        pb = screen_blend_channel(pb, bb)
      end
    end
  end

  return pr, pg, pb
end

local function render_linear(buffer, n)
  for led = 1, n do
    local r, g, b = get_color(led - 1, 0, n, 1)
    buffer:set(led, r, g, b)
  end
end

local function render_matrix(buffer, n, width, height)
  local led = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if led > n then return end
      local r, g, b = get_color(x, y, width, height)
      buffer:set(led, r, g, b)
      led = led + 1
    end
  end
end

-- ============================================================================
-- Plugin callbacks
-- ============================================================================

function plugin.on_init()
  math.randomseed(os.time())
  rebuild_gradient()
end

function plugin.on_params(p)
  if type(p) ~= "table" then return end

  local gradient_dirty = false

  -- Preset first (matches original: selecting preset rewrites color list)
  if p.preset ~= nil then
    local preset_idx = clamp(to_int(p.preset, config.preset), 0, #presets - 1)
    config.preset = preset_idx
    apply_preset(preset_idx)
    gradient_dirty = false -- apply_preset already rebuilt
  end

  if p.colors ~= nil and type(p.colors) == "table" then
    local next_colors = {}
    for i = 1, #p.colors do
      if type(p.colors[i]) == "string" then
        next_colors[#next_colors + 1] = p.colors[i]
      end
    end
    if #next_colors > 0 then
      config.colors = next_colors
      gradient_dirty = true
    end
  end

  if p.spawnMode ~= nil then
    config.spawnMode = clamp(to_int(p.spawnMode, config.spawnMode), 0, 3)
  end

  if p.trigger ~= nil then
    config.trigger = clamp(to_int(p.trigger, config.trigger), 1, 100)
  end

  if p.max_bubbles ~= nil then
    config.max_bubbles = clamp(to_int(p.max_bubbles, config.max_bubbles), 1, 32)
  end

  if p.speed_mult ~= nil then
    config.speed_mult = clamp(tonumber(p.speed_mult) or config.speed_mult, 1, 1000)
  end

  if p.max_expansion ~= nil then
    config.max_expansion = clamp(to_int(p.max_expansion, config.max_expansion), 1, 1000)
  end

  if p.bubbles_thickness ~= nil then
    config.bubbles_thickness = clamp(to_int(p.bubbles_thickness, config.bubbles_thickness), 1, 200)
  end

  if p.avgSize ~= nil then
    config.avgSize = clamp(to_int(p.avgSize, config.avgSize), 1, 256)
  end

  if gradient_dirty then
    rebuild_gradient()
  end
end

function plugin.on_tick(_, buffer, width, height)
  local n = buffer:len()
  if n <= 0 then return end

  if type(width) ~= "number" or width <= 0 then width = n end
  if type(height) ~= "number" or height <= 0 then height = 1 end

  local bins = zero_bins

  if audio and type(audio.capture) == "function" then
    local frame = audio.capture(config.avgSize)
    if frame and type(frame) == "table" and type(frame.bins) == "table" then
      bins = frame.bins
    end
  end

  -- Match original call order:
  -- 1) render current bubbles, 2) expand, 3) trigger, 4) cleanup
  if height == 1 or width == 1 then
    render_linear(buffer, n)
  else
    render_matrix(buffer, n, width, height)
  end

  expand_bubbles()
  trigger_bubbles(bins)
  cleanup_bubbles()
end

function plugin.on_shutdown()
  bubbles = {}
end

return plugin
