local plugin = {}

local PI = math.pi
local math_floor = math.floor
local math_sin = math.sin
local math_max = math.max
local math_min = math.min
local math_random = math.random

-- Parameters
local speed = 50
local random_enabled = false

-- Colors stored as HSV { h = 0-360, s = 0-1 }
-- (v is always overridden by the breathing curve)
local default_hsv_colors = {
    { h = 0,   s = 1.0 },   -- Red
    { h = 200, s = 1.0 },   -- Sky blue
    { h = 24,  s = 1.0 },   -- Orange
}
local hsv_colors = {}
for i = 1, #default_hsv_colors do
    hsv_colors[i] = { h = default_hsv_colors[i].h, s = default_hsv_colors[i].s }
end

-- State for random color cycling
local last_cycle = -1
local random_hue = 0

---------------------------------------------------------------------------
-- RGB → HSV conversion
-- r, g, b: 0-255  →  h: 0-360, s: 0-1, v: 0-1
---------------------------------------------------------------------------
local function rgb_to_hsv(r, g, b)
    r, g, b = r / 255, g / 255, b / 255
    local max_c = math_max(r, g, b)
    local min_c = math_min(r, g, b)
    local d = max_c - min_c

    local h, s
    if d == 0 then
        h = 0
    elseif max_c == r then
        h = ((g - b) / d) % 6
    elseif max_c == g then
        h = (b - r) / d + 2
    else
        h = (r - g) / d + 4
    end
    h = h * 60
    if h < 0 then h = h + 360 end

    s = (max_c == 0) and 0 or (d / max_c)

    return h, s, max_c
end

---------------------------------------------------------------------------
-- Parse "#RRGGBB" hex string → { h, s } (HSV, ignoring original brightness)
---------------------------------------------------------------------------
local function parse_hex_to_hsv(value)
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

    local r = tonumber(hex:sub(1, 2), 16) or 0
    local g = tonumber(hex:sub(3, 4), 16) or 0
    local b = tonumber(hex:sub(5, 6), 16) or 0

    local h, s, _ = rgb_to_hsv(r, g, b)
    return { h = h, s = s }
end

---------------------------------------------------------------------------
-- Resolve raw color array from params into HSV palette
---------------------------------------------------------------------------
local function resolve_palette(raw_colors)
    if type(raw_colors) ~= "table" then
        return nil
    end
    local result = {}
    for i = 1, #raw_colors do
        local parsed = parse_hex_to_hsv(raw_colors[i])
        if parsed then
            result[#result + 1] = parsed
        end
    end
    if #result == 0 then
        return nil
    end
    return result
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

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
    if type(p.random) == "boolean" then
        random_enabled = p.random
    end
    if type(p.colors) == "table" then
        local palette = resolve_palette(p.colors)
        if palette then
            hsv_colors = palette
        end
    end
end

---------------------------------------------------------------------------
-- Render
--
-- Faithfully reproduces the reference C++ Breathing effect:
--   Progress += (Speed / 100.0) / FPS          (per frame)
--   if Progress >= PI then next color end
--   CurrentColor.value = pow(sin(Progress), 3) * 255
--   All LEDs ← hsv2rgb(CurrentColor)
--
-- Mapped to total-elapsed-time:
--   rate       = speed / 50.0                   (rad/s, speed=50 ≈ ref Speed=100)
--   progress   = t * rate
--   cycle      = floor(progress / PI)
--   phase      = progress mod PI                (0 → PI within one breath)
--   brightness = sin(phase)^3                   (0 → 1 → 0, sharper tails)
---------------------------------------------------------------------------

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then
        return
    end

    -- Speed mapping: slider 50 ≈ reference default (1.0 rad/s, ~3.14s per breath)
    local rate = speed / 50.0
    local raw_progress = t * rate

    -- Determine which breathing cycle we are in and the phase within it
    local cycle = math_floor(raw_progress / PI)
    local phase = raw_progress - cycle * PI  -- 0 … PI

    -- Breathing curve: sin^3 (identical to reference pow(sin(x), 3))
    local sin_val = math_sin(phase)
    local brightness = sin_val * sin_val * sin_val

    local h, s

    if random_enabled then
        -- Pick a new random hue each breathing cycle (ref: ColorUtils::RandomHSVColor)
        if cycle ~= last_cycle then
            last_cycle = cycle
            random_hue = math_random() * 360.0
        end
        h = random_hue
        s = 1.0
    else
        -- Cycle through colors sequentially (ref: colorLoopIndex++)
        local num = #hsv_colors
        if num == 0 then
            for i = 1, n do
                buffer:set_hsv(i, 0, 0, 0)
            end
            return
        end
        local idx = (cycle % num) + 1
        h = hsv_colors[idx].h
        s = hsv_colors[idx].s
    end

    -- Set all LEDs to the same color (ref: SetAllZoneLEDs)
    for i = 1, n do
        buffer:set_hsv(i, h, s, brightness)
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
