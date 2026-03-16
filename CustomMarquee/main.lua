local plugin = {}

local math_floor = math.floor

local default_colors = {
  { r = 0, g = 0, b = 0 },
}

local speed = 25
local colors = {
  { r = 0, g = 0, b = 0 },
}

local progress = 0.0
local last_t = 0.0

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
    return clone_palette(default_colors)
  end

  local next_colors = {}
  for i = 1, #raw_colors do
    local parsed = parse_hex_color(raw_colors[i])
    if parsed then
      next_colors[#next_colors + 1] = parsed
    end
  end

  if #next_colors == 0 then
    return clone_palette(default_colors)
  end

  return next_colors
end

function plugin.on_init()
  colors = clone_palette(default_colors)
  progress = 0.0
  last_t = 0.0
end

function plugin.on_params(p)
  if type(p) ~= "table" then
    return
  end

  if type(p.speed) == "number" then
    speed = p.speed
  end

  if type(p.colors) == "table" then
    colors = resolve_palette(p.colors)
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

  local palette = colors
  local count = #palette
  if count <= 0 then
    palette = default_colors
    count = #palette
  end

  local shift = math_floor(progress)
  local led = 1

  for _ = 0, height - 1 do
    for x = 0, width - 1 do
      if led > n then
        break
      end

      local color = palette[((x + shift) % count) + 1]
      buffer:set(led, color.r, color.g, color.b)
      led = led + 1
    end
  end

  -- Match the reference order exactly:
  -- render current frame first, then advance progress by Speed / FPS.
  local dt = t - last_t
  if dt < 0 then
    dt = 0
  end
  progress = progress + (speed * dt)
  last_t = t
end

function plugin.on_shutdown()
  -- no-op
end

return plugin
