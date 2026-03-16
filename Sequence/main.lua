local plugin = {}

local math_floor = math.floor
local math_ceil = math.ceil
local math_max = math.max
local math_min = math.min

---------------------------------------------------------------------------
-- Parameters
---------------------------------------------------------------------------
local speed = 10      -- 1~20, reference default 10
local fade_time = 1   -- 1~100, reference Slider2Val "Fade time"

local default_colors = {
    { r = 255, g = 0,   b = 0   },
    { r = 0,   g = 255, b = 0   },
    { r = 0,   g = 0,   b = 255 },
    { r = 255, g = 255, b = 0   },
    { r = 0,   g = 255, b = 255 },
}

local function clone_palette(source)
    local out = {}
    for i = 1, #source do
        local c = source[i]
        out[i] = { r = c.r, g = c.g, b = c.b }
    end
    return out
end

local colors = clone_palette(default_colors)

---------------------------------------------------------------------------
-- Mutable state
--
-- Use delta-time accumulation (not pure elapsed time) because
-- the progress rate changes depending on whether we are in the
-- solid zone or the fade zone.
---------------------------------------------------------------------------
local progress = 0.0
local last_t = nil

---------------------------------------------------------------------------
-- Helpers
---------------------------------------------------------------------------

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
        return nil
    end
    local result = {}
    for i = 1, #raw_colors do
        local parsed = parse_hex_color(raw_colors[i])
        if parsed then
            result[#result + 1] = parsed
        end
    end
    if #result == 0 then
        return nil
    end
    return result
end

local function lerp(a, b, t)
    return math_floor(a + (b - a) * t + 0.5)
end

local function interpolate_color(c1, c2, t)
    return lerp(c1.r, c2.r, t),
           lerp(c1.g, c2.g, t),
           lerp(c1.b, c2.b, t)
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    progress = 0.0
    last_t = nil
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end
    if type(p.speed) == "number" then
        speed = math_max(1, math_min(20, math_floor(p.speed + 0.5)))
    end
    if type(p.fade_time) == "number" then
        fade_time = math_max(1, math_min(100, math_floor(p.fade_time + 0.5)))
    end
    if type(p.colors) == "table" then
        local palette = resolve_palette(p.colors)
        if palette then
            colors = palette
        end
    end
end

---------------------------------------------------------------------------
-- Render
--
-- Faithfully reproduces the reference C++ Sequence effect:
--
--   current_color_index = ceil(progress) % colors_count
--   frac = fractional part of progress
--
--   if frac >= 0.8:
--       blend from current color to next color
--       blend_factor = (frac - 0.8) * 5       (maps 0.8~1.0 → 0~1)
--       fade_mult = 1 / Slider2Val             (slows progress during fade)
--   else:
--       solid current color
--       fade_mult = 1.0
--
--   All LEDs ← computed color
--   progress += fade_mult * 0.1 * Speed / FPS
--
-- Time-based equivalent (dt = 1/FPS):
--   progress += fade_mult * 0.1 * Speed * dt
---------------------------------------------------------------------------

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then
        return
    end

    local colors_count = #colors
    if colors_count == 0 then
        return
    end

    -- Compute delta time
    local delta = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            delta = t
        else
            delta = t - last_t
        end
        last_t = t
    end

    -- Reference: current_color_index = ((int)ceil(progress)) % colors_count
    -- Lua arrays are 1-based, so we add 1
    local ceil_progress = math_ceil(progress)
    local current_color_index = (ceil_progress % colors_count) + 1

    -- Get fractional part of progress
    local whole = math_floor(progress)
    local frac = progress - whole

    local r, g, b
    local fade_mult

    if frac >= 0.8 then
        -- Fade zone: interpolate from current color to next color
        -- Reference: next_color_index = current < count-1 ? current+1 : 0
        local next_color_index = (current_color_index % colors_count) + 1
        local blend = (frac - 0.8) * 5.0
        r, g, b = interpolate_color(colors[current_color_index], colors[next_color_index], blend)
        fade_mult = 1.0 / fade_time
    else
        -- Solid zone: show current color
        local c = colors[current_color_index]
        r, g, b = c.r, c.g, c.b
        fade_mult = 1.0
    end

    -- Set all LEDs to the same color (reference: SetAllZoneLEDs)
    for i = 1, n do
        buffer:set(i, r, g, b)
    end

    -- Advance progress
    -- Reference: progress += fade_mult * 0.1 * Speed / FPS
    -- Time-based: progress += fade_mult * 0.1 * Speed * delta
    progress = progress + fade_mult * 0.1 * speed * delta
end

function plugin.on_shutdown()
    last_t = nil
end

return plugin
