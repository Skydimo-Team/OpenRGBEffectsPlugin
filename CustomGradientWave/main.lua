local plugin = {}

local math_abs = math.abs
local math_floor = math.floor
local math_sqrt = math.sqrt

local CUSTOM_PRESET = 0

local HORIZONTAL = 0
local VERTICAL = 1
local RADIAL_OUT = 2
local RADIAL_IN = 3

local GRADIENT_SAMPLES = 100

local function clone_palette(source)
  local out = {}
  for i = 1, #source do
    local color = source[i]
    out[i] = { r = color.r, g = color.g, b = color.b }
  end
  return out
end

local default_custom_colors = {
  { r = 0xFF, g = 0x00, b = 0x00 },
  { r = 0xFF, g = 0x00, b = 0xE6 },
  { r = 0x00, g = 0x00, b = 0xFF },
  { r = 0x00, g = 0xB3, b = 0xFF },
  { r = 0x00, g = 0xFF, b = 0x51 },
  { r = 0xEA, g = 0xFF, b = 0x00 },
  { r = 0xFF, g = 0xB3, b = 0x00 },
  { r = 0xFF, g = 0x00, b = 0x00 },
}

local preset_palettes = {
  [1] = clone_palette(default_custom_colors),
  [2] = {
    { r = 0x14, g = 0xE8, b = 0x1E },
    { r = 0x00, g = 0xEA, b = 0x8D },
    { r = 0x01, g = 0x7E, b = 0xD5 },
    { r = 0xB5, g = 0x3D, b = 0xFF },
    { r = 0x8D, g = 0x00, b = 0xC4 },
    { r = 0x14, g = 0xE8, b = 0x1E },
  },
  [3] = {
    { r = 0x00, g = 0x00, b = 0x7F },
    { r = 0x00, g = 0x00, b = 0xFF },
    { r = 0x00, g = 0xFF, b = 0xFF },
    { r = 0x00, g = 0xAA, b = 0xFF },
    { r = 0x00, g = 0x00, b = 0x7F },
  },
  [4] = {
    { r = 0xFE, g = 0x00, b = 0xC5 },
    { r = 0x00, g = 0xC5, b = 0xFF },
    { r = 0x00, g = 0xC5, b = 0xFF },
    { r = 0xFE, g = 0x00, b = 0xC5 },
  },
  [5] = {
    { r = 0xFE, g = 0xE0, b = 0x00 },
    { r = 0xFE, g = 0x00, b = 0xFE },
    { r = 0xFE, g = 0x00, b = 0xFE },
    { r = 0xFE, g = 0xE0, b = 0x00 },
  },
  [6] = {
    { r = 0xFF, g = 0x55, b = 0x00 },
    { r = 0x00, g = 0x00, b = 0x00 },
    { r = 0x00, g = 0x00, b = 0x00 },
    { r = 0x00, g = 0x00, b = 0x00 },
    { r = 0xFF, g = 0x55, b = 0x00 },
  },
  [7] = {
    { r = 0xFF, g = 0x21, b = 0x00 },
    { r = 0xAA, g = 0x00, b = 0xFF },
    { r = 0xAA, g = 0x00, b = 0xFF },
    { r = 0xFF, g = 0x21, b = 0x00 },
    { r = 0xFF, g = 0x21, b = 0x00 },
    { r = 0xFF, g = 0x21, b = 0x00 },
  },
  [8] = {
    { r = 0x03, g = 0xFF, b = 0xFA },
    { r = 0x55, g = 0x00, b = 0x7F },
    { r = 0x55, g = 0x00, b = 0x7F },
    { r = 0x03, g = 0xFF, b = 0xFA },
  },
  [9] = {
    { r = 0xFF, g = 0x00, b = 0x00 },
    { r = 0x00, g = 0x00, b = 0xFF },
    { r = 0x00, g = 0x00, b = 0xFF },
    { r = 0xFF, g = 0x00, b = 0x00 },
    { r = 0xFF, g = 0x00, b = 0x00 },
  },
  [10] = {
    { r = 0x00, g = 0xFF, b = 0x00 },
    { r = 0x00, g = 0x32, b = 0xFF },
    { r = 0x00, g = 0x32, b = 0xFF },
    { r = 0x00, g = 0xFF, b = 0x00 },
    { r = 0x00, g = 0xFF, b = 0x00 },
  },
  [11] = {
    { r = 0xFF, g = 0x21, b = 0x00 },
    { r = 0xAB, g = 0x00, b = 0x6D },
    { r = 0xC0, g = 0x1C, b = 0x52 },
    { r = 0xD5, g = 0x37, b = 0x37 },
    { r = 0xEA, g = 0x53, b = 0x1B },
    { r = 0xFF, g = 0x6E, b = 0x00 },
    { r = 0xFF, g = 0x00, b = 0x00 },
    { r = 0xFF, g = 0x21, b = 0x00 },
  },
  [12] = {
    { r = 0xFF, g = 0x71, b = 0xCE },
    { r = 0xB9, g = 0x67, b = 0xFF },
    { r = 0x01, g = 0xCD, b = 0xFE },
    { r = 0x05, g = 0xFF, b = 0xA1 },
    { r = 0xFF, g = 0xFB, b = 0x96 },
    { r = 0xFF, g = 0x71, b = 0xCE },
  },
}

local speed = 25
local preset = 1
local spread = 100
local direction = HORIZONTAL
local center_y_percent = 50
local center_x_percent = 50

local custom_colors = clone_palette(default_custom_colors)
local gradient_strip = {}
local gradient_dirty = true

local function parse_hex_color(value)
  if type(value) ~= "string" then
    return nil
  end

  local hex = value:gsub("%s+", "")
  if hex:sub(1, 1) == "#" then
    hex = hex:sub(2)
  end

  if #hex == 3 then
    hex = hex:sub(1, 1):rep(2)
      .. hex:sub(2, 2):rep(2)
      .. hex:sub(3, 3):rep(2)
  end

  if #hex ~= 6 or hex:find("[^%x]") then
    return nil
  end

  return {
    r = tonumber(hex:sub(1, 2), 16) or 255,
    g = tonumber(hex:sub(3, 4), 16) or 255,
    b = tonumber(hex:sub(5, 6), 16) or 255,
  }
end

local function resolve_palette(raw_colors)
  if type(raw_colors) ~= "table" then
    return clone_palette(default_custom_colors)
  end

  local resolved = {}
  for i = 1, #raw_colors do
    local parsed = parse_hex_color(raw_colors[i])
    if parsed then
      resolved[#resolved + 1] = parsed
    end
  end

  if #resolved < 2 then
    return clone_palette(default_custom_colors)
  end

  return resolved
end

local function resolve_active_palette()
  return preset_palettes[preset] or custom_colors
end

local function lerp_channel(a, b, t)
  return math_floor(a + (b - a) * t + 0.5)
end

local function rebuild_gradient()
  local palette = resolve_active_palette()
  local count = #palette
  gradient_strip = {}

  if count <= 0 then
    return
  end

  if count == 1 then
    local color = palette[1]
    for sample_index = 1, GRADIENT_SAMPLES do
      gradient_strip[sample_index] = { r = color.r, g = color.g, b = color.b }
    end
    gradient_dirty = false
    return
  end

  local segment_count = count - 1
  for sample_index = 0, GRADIENT_SAMPLES - 1 do
    -- The original Qt effect rasterizes into a 100x1 image and then samples
    -- integer pixels with `pixelColor(100 * i, 0)`. Using pixel centers keeps
    -- the Lua strip closer to how QPainter actually shades that image.
    local position = (sample_index + 0.5) / GRADIENT_SAMPLES
    local scaled = position * segment_count
    local left_index = math_floor(scaled) + 1
    local blend = scaled - math_floor(scaled)

    if left_index >= count then
      left_index = count - 1
      blend = 1.0
    end

    local right_index = left_index + 1
    local left = palette[left_index]
    local right = palette[right_index]

    gradient_strip[sample_index + 1] = {
      r = lerp_channel(left.r, right.r, blend),
      g = lerp_channel(left.g, right.g, blend),
      b = lerp_channel(left.b, right.b, blend),
    }
  end

  gradient_dirty = false
end

local function sample_gradient(position)
  if gradient_dirty then
    rebuild_gradient()
  end

  local wrapped = position - math_floor(position)
  local index = math_floor(GRADIENT_SAMPLES * wrapped) + 1

  if index < 1 then
    index = 1
  elseif index > GRADIENT_SAMPLES then
    index = GRADIENT_SAMPLES
  end

  local color = gradient_strip[index]
  if not color then
    return 0, 0, 0
  end

  return color.r, color.g, color.b
end

function plugin.on_init()
  rebuild_gradient()
end

function plugin.on_params(p)
  if type(p) ~= "table" then
    return
  end

  local needs_gradient_refresh = false

  if type(p.speed) == "number" then
    speed = p.speed
  end

  if type(p.preset) == "number" then
    local next_preset = math_floor(p.preset + 0.5)
    if next_preset == CUSTOM_PRESET or preset_palettes[next_preset] then
      if preset ~= next_preset then
        preset = next_preset
        needs_gradient_refresh = true
      end
    end
  end

  if type(p.colors) == "table" then
    custom_colors = resolve_palette(p.colors)
    if preset == CUSTOM_PRESET then
      needs_gradient_refresh = true
    end
  end

  if type(p.direction) == "number" then
    local next_direction = math_floor(p.direction + 0.5)
    if next_direction >= HORIZONTAL and next_direction <= RADIAL_IN then
      direction = next_direction
    end
  end

  if type(p.height) == "number" then
    center_y_percent = p.height
  end

  if type(p.width) == "number" then
    center_x_percent = p.width
  end

  if type(p.spread) == "number" then
    spread = p.spread
  end

  if needs_gradient_refresh then
    rebuild_gradient()
  end
end

function plugin.on_tick(t, buffer, width, height)
  local n = buffer:len()
  if n <= 0 then
    return
  end

  if type(width) ~= "number" or width <= 0 then
    width = n
  end
  if type(height) ~= "number" or height <= 0 then
    height = 1
  end

  if gradient_dirty then
    rebuild_gradient()
  end

  local progress = 0.01 * speed * t
  local spread_factor = spread / 100.0
  local center_y = (height - 1) * (0.01 * center_y_percent)
  local center_x = (width - 1) * (0.01 * center_x_percent)

  local led = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if led > n then
        return
      end

      local gradient_index
      if direction == HORIZONTAL then
        gradient_index = spread_factor * x / width + progress
      elseif direction == VERTICAL then
        gradient_index = spread_factor * y / height + progress
      else
        local dx = x - center_x
        local dy = y - center_y
        local distance = math_sqrt(dx * dx + dy * dy)

        if direction == RADIAL_IN then
          gradient_index = math_abs(spread_factor * distance / width + progress)
        else
          gradient_index = math_abs(spread_factor * distance / width - progress)
        end
      end

      local r, g, b = sample_gradient(gradient_index)
      buffer:set(led, r, g, b)
      led = led + 1
    end
  end
end

function plugin.on_shutdown()
  -- no-op
end

return plugin
