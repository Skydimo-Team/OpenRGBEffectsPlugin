local plugin = {}

local math_floor = math.floor
local math_pow   = math.pow or function(b, e) return b ^ e end

-- Parameters with defaults (matching original C++ ranges)
-- Original: Speed 1-50 default 25, Slider2 (comet_size) 1-100 default 50
-- Mapped:   speed 1-100 default 50, comet_size 1-100 default 50
local speed      = 50
local comet_size = 50
local color_mode = 0   -- 0 = rainbow, 1 = custom
local user_color = { r = 0, g = 170, b = 255 }  -- #00AAFF

-- Internal state
local time_acc = 0.0
local progress = 0.0

local function parse_hex_color(value)
  if type(value) ~= "string" then
    return nil
  end
  local hex = value:gsub("%s+", "")
  if hex:sub(1, 1) == "#" then
    hex = hex:sub(2)
  end
  if #hex ~= 6 or hex:find("[^%x]") then
    return nil
  end
  return {
    r = tonumber(hex:sub(1, 2), 16) or 0,
    g = tonumber(hex:sub(3, 4), 16) or 170,
    b = tonumber(hex:sub(5, 6), 16) or 255,
  }
end

-- Convert RGB (0-255) to HSV (h: 0-360, s: 0-255, v: 0-255)
-- Matches the C++ hsv_t struct used in the reference
local function rgb_to_hsv(r, g, b)
  local r_n, g_n, b_n = r / 255.0, g / 255.0, b / 255.0
  local max = math.max(r_n, g_n, b_n)
  local min = math.min(r_n, g_n, b_n)
  local delta = max - min

  local h, s, v
  v = max

  if max == 0 then
    s = 0
    h = 0
  else
    s = delta / max
    if delta == 0 then
      h = 0
    elseif max == r_n then
      h = 60 * (((g_n - b_n) / delta) % 6)
    elseif max == g_n then
      h = 60 * (((b_n - r_n) / delta) + 2)
    else
      h = 60 * (((r_n - g_n) / delta) + 4)
    end
  end

  if h < 0 then h = h + 360 end

  return h, s * 255, v * 255
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
  if type(p.comet_size) == "number" then
    comet_size = p.comet_size
  end
  if type(p.color_mode) == "number" then
    local m = math_floor(p.color_mode + 0.5)
    if m == 0 or m == 1 then
      color_mode = m
    end
  end
  if p.color ~= nil then
    local parsed = parse_hex_color(p.color)
    if parsed then
      user_color = parsed
    end
  end
end

-- Compute color for a single LED in the comet trail
-- i: LED index (0-based), width: total strip length
-- Returns r, g, b (0-255)
local function get_color(i, width)
  -- Comet tail size as fraction of strip width (1-100% → 0.01-1.0)
  local tail_len = 0.01 * comet_size * width

  -- Head position sweeps from 0 to 2*width so the comet fully enters and exits
  local position = progress * 2 * width

  -- LEDs ahead of the head are OFF
  if i > position then
    return 0, 0, 0
  end

  -- Distance from the comet head
  local distance = position - i

  -- Brightness envelope: 1 at head, fading to 0 at tail end
  local value
  if distance > tail_len then
    value = 0
  elseif distance == 0 then
    value = 1
  else
    value = 1.0 - (distance / tail_len)
  end

  -- Apply the same power curves as the original:
  -- saturation uses pow(value, 0.2) — gentle falloff, stays colorful
  -- brightness uses pow(value, 3)  — sharp falloff, dramatic trail
  local sat_factor = math_pow(value, 0.2)
  local val_factor = math_pow(value, 3)

  if color_mode == 0 then
    -- Rainbow mode: hue rotates with time and shifts along the trail
    local hue = (1000 * time_acc + (distance / math.max(tail_len, 1)) * 360) % 360
    local sat = sat_factor
    local val = val_factor
    -- host.hsv_to_rgb expects h:0-360, s:0-1, v:0-1
    return host.hsv_to_rgb(hue, sat, val)
  else
    -- Custom color mode: use user color with original power-curve fading
    local uh, us, uv = rgb_to_hsv(user_color.r, user_color.g, user_color.b)
    -- Scale saturation and value by the power-curve factors
    -- us and uv are 0-255 range, convert to 0-1 for host.hsv_to_rgb
    local sat = sat_factor * (us / 255.0)
    local val = val_factor * (uv / 255.0)
    return host.hsv_to_rgb(uh, sat, val)
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

  -- For matrix layouts, render the comet along each row
  local idx = 1
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      if idx > n then
        return
      end
      local r, g, b = get_color(x, width)
      buffer:set(idx, r, g, b)
      idx = idx + 1
    end
  end

  -- Advance time accumulator
  -- Original: time += 0.01 * Speed / FPS with Speed 1-50 default 25
  -- We map our speed (1-100, default 50) to the same range:
  -- speed 50 → original Speed 25 → 0.01 * 25 = 0.25 per second at 60fps
  -- So: 0.01 * (speed / 2) = 0.005 * speed
  -- Per-tick: divided by ~60fps equivalent, but we use elapsed time differences
  -- The reference increments per-frame, so we convert:
  -- Original per-frame increment = 0.01 * Speed / FPS ≈ 0.01 * 25 / 60 ≈ 0.00417
  -- With t as absolute time, we use a delta approach via progress wrapping
  time_acc = time_acc + 0.005 * speed / 60.0
  progress = time_acc % 1.0
end

function plugin.on_shutdown()
  -- no-op
end

return plugin
