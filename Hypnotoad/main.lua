local plugin = {}

local math_abs = math.abs
local math_atan = math.atan
local math_cos = math.cos
local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_modf = math.modf
local math_sqrt = math.sqrt
local PI = math.pi

local COLOR_MODE_RAINBOW = 0
local COLOR_MODE_CUSTOM = 1

local ANIMATION_DIRECTION_INSIDE = 0
local ANIMATION_DIRECTION_OUTSIDE = 1

local COLOR_ROTATION_CLOCKWISE = 0
local COLOR_ROTATION_COUNTER_CLOCKWISE = 1

local GRADIENT_SAMPLES = 100

local speed = 50
local color_mode = COLOR_MODE_RAINBOW
local animation_speed = 10
local color_rotation_speed = 10
local animation_direction = ANIMATION_DIRECTION_INSIDE
local color_rotation_direction = COLOR_ROTATION_CLOCKWISE
local spacing = 1
local thickness = 1
local cx_shift = 50
local cy_shift = 50

local default_custom_colors = {
  { r = 0, g = 0, b = 0 },
}

local custom_colors = {}
local gradient_strip = {}
local gradient_dirty = true

local function clamp_number(value, min_value, max_value)
  if value < min_value then
    return min_value
  end
  if value > max_value then
    return max_value
  end
  return value
end

local function clone_palette(source)
  local out = {}
  for i = 1, #source do
    local color = source[i]
    out[i] = { r = color.r, g = color.g, b = color.b }
  end
  return out
end

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
    r = tonumber(hex:sub(1, 2), 16) or 0,
    g = tonumber(hex:sub(3, 4), 16) or 0,
    b = tonumber(hex:sub(5, 6), 16) or 0,
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

  if #resolved == 0 then
    return clone_palette(default_custom_colors)
  end

  return resolved
end

local function c_trunc(value)
  local integer = math_modf(value)
  return integer
end

local function c_remainder(value, divisor)
  local quotient = c_trunc(value / divisor)
  return value - quotient * divisor
end

local function lerp_channel(a, b, t)
  return math_floor(a + (b - a) * t + 0.5)
end

local function rebuild_gradient()
  local palette = custom_colors
  local count = #palette
  gradient_strip = {}

  if count <= 0 then
    for sample_index = 1, GRADIENT_SAMPLES do
      gradient_strip[sample_index] = { r = 0, g = 0, b = 0 }
    end
    gradient_dirty = false
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

  -- Match the reference's QImage(100, 1) gradient rasterization as closely as possible.
  local segment_count = count - 1
  for sample_index = 0, GRADIENT_SAMPLES - 1 do
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

  local index = math_floor(position * GRADIENT_SAMPLES) + 1
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

local function rgb_to_hsv255(r, g, b)
  local maxc = math_max(r, g, b)
  local minc = math_min(r, g, b)
  local delta = maxc - minc
  local hue = 0.0

  if delta > 0 then
    if maxc == r then
      hue = 60.0 * (((g - b) / delta) % 6.0)
    elseif maxc == g then
      hue = 60.0 * (((b - r) / delta) + 2.0)
    else
      hue = 60.0 * (((r - g) / delta) + 4.0)
    end
  end

  if hue < 0.0 then
    hue = hue + 360.0
  end

  local saturation = 0
  if maxc > 0 then
    saturation = math_floor((delta / maxc) * 255.0)
  end

  return hue, saturation, maxc
end

local function enlight_color(r, g, b, factor)
  if factor <= 0.0 then
    return 0, 0, 0
  end

  local hue, saturation255, value255 = rgb_to_hsv255(r, g, b)
  local scaled_value = math_floor(value255 * factor)
  if scaled_value <= 0 then
    return 0, 0, 0
  end

  return host.hsv_to_rgb(
    hue,
    saturation255 / 255.0,
    clamp_number(scaled_value, 0, 255) / 255.0
  )
end

local function phase_degrees(angle, distance, progress, color_mult)
  -- The reference relies on C++ int casts and modulo semantics, which differ from Lua.
  local raw_phase = c_trunc(angle + distance + progress * color_mult * color_rotation_speed)
  return math_abs(c_remainder(raw_phase, 360))
end

function plugin.on_init()
  custom_colors = clone_palette(default_custom_colors)
  rebuild_gradient()
end

function plugin.on_params(p)
  if type(p) ~= "table" then
    return
  end

  if type(p.speed) == "number" then
    speed = clamp_number(p.speed, 1, 100)
  end

  if type(p.color_mode) == "number" then
    local next_mode = c_trunc(p.color_mode)
    if next_mode == COLOR_MODE_RAINBOW or next_mode == COLOR_MODE_CUSTOM then
      color_mode = next_mode
    end
  end

  if type(p.colors) == "table" then
    custom_colors = resolve_palette(p.colors)
    gradient_dirty = true
  end

  if type(p.animation_speed) == "number" then
    animation_speed = c_trunc(clamp_number(p.animation_speed, 10, 99))
  end

  if type(p.animation_direction) == "number" then
    local next_direction = c_trunc(p.animation_direction)
    if next_direction == ANIMATION_DIRECTION_INSIDE or next_direction == ANIMATION_DIRECTION_OUTSIDE then
      animation_direction = next_direction
    end
  end

  if type(p.color_rotation_speed) == "number" then
    color_rotation_speed = c_trunc(clamp_number(p.color_rotation_speed, 10, 99))
  end

  if type(p.color_rotation_direction) == "number" then
    local next_direction = c_trunc(p.color_rotation_direction)
    if next_direction == COLOR_ROTATION_CLOCKWISE or next_direction == COLOR_ROTATION_COUNTER_CLOCKWISE then
      color_rotation_direction = next_direction
    end
  end

  if type(p.spacing) == "number" then
    spacing = c_trunc(clamp_number(p.spacing, 1, 10))
  end

  if type(p.thickness) == "number" then
    thickness = c_trunc(clamp_number(p.thickness, 1, 10))
  end

  if type(p.cx) == "number" then
    cx_shift = c_trunc(clamp_number(p.cx, 0, 100))
  end

  if type(p.cy) == "number" then
    cy_shift = c_trunc(clamp_number(p.cy, 0, 100))
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

  local cx_shift_mult = 0.01 * cx_shift
  local cy_shift_mult = 0.01 * cy_shift
  local cx = (width - 1) * cx_shift_mult
  local cy = (height > 1) and ((height - 1) * cy_shift_mult) or cy_shift_mult

  local animation_mult = 0.01 * animation_speed * ((animation_direction == ANIMATION_DIRECTION_INSIDE) and 1.0 or -1.0)
  local color_mult = 0.01 * color_rotation_speed * ((color_rotation_direction == COLOR_ROTATION_CLOCKWISE) and -1.0 or 1.0)

  local progress = 1000.0 + 0.1 * speed * t
  local exponent = 11 - thickness

  local led = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if led > n then
        return
      end

      local angle = math_atan(y - cy, x - cx) * 180.0 / PI
      local dx = cx - x
      local dy = cy - y
      local distance = math_sqrt(dx * dx + dy * dy)
      local wave = math_cos(animation_mult * distance / (0.1 * spacing) + progress)
      local value_factor = ((wave + 1.0) * 0.5) ^ exponent
      local hue = phase_degrees(angle, distance, progress, color_mult)

      if color_mode == COLOR_MODE_RAINBOW then
        local value255 = math_floor(value_factor * 255.0)
        buffer:set_hsv(led, hue, 1.0, clamp_number(value255, 0, 255) / 255.0)
      else
        local gradient_position = hue / 360.0
        local r, g, b = sample_gradient(gradient_position)
        local out_r, out_g, out_b = enlight_color(r, g, b, value_factor)
        buffer:set(led, out_r, out_g, out_b)
      end

      led = led + 1
    end
  end
end

function plugin.on_shutdown()
  -- no-op
end

return plugin
