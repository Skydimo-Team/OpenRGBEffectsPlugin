local plugin = {}

local math_floor = math.floor

local CUSTOM_PRESET = 0
local RAINBOW_PRESET = 1
local SUNSET_PRESET = 2
local OCEAN_PRESET = 3
local SYNTHWAVE_PRESET = 4

local speed = 2.5
local default_custom_colors = {
  { r = 255, g = 0, b = 0 },
  { r = 255, g = 153, b = 0 },
  { r = 255, g = 255, b = 0 },
  { r = 0, g = 255, b = 136 },
  { r = 0, g = 170, b = 255 },
  { r = 170, g = 0, b = 255 },
}
local palette_presets = {
  [RAINBOW_PRESET] = default_custom_colors,
  [SUNSET_PRESET] = {
    { r = 255, g = 94, b = 77 },
    { r = 255, g = 154, b = 0 },
    { r = 255, g = 206, b = 84 },
    { r = 255, g = 111, b = 145 },
  },
  [OCEAN_PRESET] = {
    { r = 0, g = 88, b = 255 },
    { r = 0, g = 170, b = 255 },
    { r = 0, g = 255, b = 204 },
    { r = 126, g = 255, b = 245 },
  },
  [SYNTHWAVE_PRESET] = {
    { r = 255, g = 0, b = 128 },
    { r = 255, g = 71, b = 195 },
    { r = 125, g = 65, b = 255 },
    { r = 0, g = 217, b = 255 },
  },
}

local function clone_palette(source)
  local out = {}
  for i = 1, #source do
    local color = source[i]
    out[i] = { r = color.r, g = color.g, b = color.b }
  end
  return out
end

local custom_colors = clone_palette(default_custom_colors)
local palette_preset = CUSTOM_PRESET

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

  local next_colors = {}
  for i = 1, #raw_colors do
    local parsed = parse_hex_color(raw_colors[i])
    if parsed then
      next_colors[#next_colors + 1] = parsed
    end
  end

  if #next_colors == 0 then
    return clone_palette(default_custom_colors)
  end

  return next_colors
end

local function resolve_active_palette()
  return palette_presets[palette_preset] or custom_colors
end

local function lerp(a, b, t)
  return math_floor(a + (b - a) * t + 0.5)
end

local function sample_gradient(palette, position)
  local count = #palette
  if count <= 0 then
    return 255, 255, 255
  end

  if count == 1 then
    local color = palette[1]
    return color.r, color.g, color.b
  end

  local wrapped = position % 1.0
  local scaled = wrapped * count
  local left_index = math_floor(scaled) + 1
  local blend = scaled - math_floor(scaled)

  if left_index > count then
    left_index = 1
  end

  local right_index = (left_index % count) + 1
  local left = palette[left_index]
  local right = palette[right_index]

  return lerp(left.r, right.r, blend),
         lerp(left.g, right.g, blend),
         lerp(left.b, right.b, blend)
end

function plugin.on_init()
  -- no-op
end


function plugin.on_params(p)
  if type(p) ~= "table" then
    return
  end
  if type(p.speed) == "number" then
    speed = p.speed
  end
  if type(p.preset) == "number" then
    local next_preset = math_floor(p.preset + 0.5)
    if next_preset == CUSTOM_PRESET or palette_presets[next_preset] then
      palette_preset = next_preset
    else
      palette_preset = CUSTOM_PRESET
    end
  end
  if type(p.colors) == "table" then
    custom_colors = resolve_palette(p.colors)
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

  local palette = resolve_active_palette()
  local offset = (t * speed * 0.12) % 1.0
  local row_shift = height > 1 and (0.16 / height) or 0.0

  local i = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if i > n then
        return
      end

      local position = offset + (x / width) + (y * row_shift)
      local r, g, b = sample_gradient(palette, position)
      buffer:set(i, r, g, b)

      i = i + 1
    end
  end
end

function plugin.on_shutdown()
  -- no-op
end

return plugin
