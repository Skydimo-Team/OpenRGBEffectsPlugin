local plugin = {}

local math_floor = math.floor
local math_max = math.max
local math_min = math.min

local speed = 40
local frequency = 10

local progress = 0.0
local last_t = nil

local function clamp_int(value, min_value, max_value)
  return math_max(min_value, math_min(max_value, math_floor(value + 0.5)))
end

function plugin.on_init()
  -- no-op
end

function plugin.on_params(p)
  if type(p) ~= "table" then
    return
  end

  if type(p.speed) == "number" then
    speed = clamp_int(p.speed, 1, 100)
  end

  if type(p.frequency) == "number" then
    frequency = clamp_int(p.frequency, 1, 50)
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

  local current_progress = progress

  local led = 1
  for _ = 0, height - 1 do
    for x = 0, width - 1 do
      if led <= n then
        local hue = math_floor((current_progress + x) * frequency)
        buffer:set_hsv(led, hue, 1.0, 1.0)
      end
      led = led + 1
    end
  end

  local delta = 0.0
  if type(t) == "number" and t >= 0 then
    if last_t == nil or t < last_t then
      delta = t
    else
      delta = t - last_t
    end
    last_t = t
  end

  -- Match the original effect's frame order: render the current progress,
  -- then advance it, and only reset after one frame above 360.
  if current_progress < 360.0 then
    progress = current_progress + speed * delta
  else
    progress = 0.0
  end
end

function plugin.on_shutdown()
  -- no-op
end

return plugin
