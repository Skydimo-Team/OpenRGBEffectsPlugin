local plugin = {}

local math_floor = math.floor
local math_sin   = math.sin
local math_min   = math.min
local math_max   = math.max
local math_random     = math.random
local math_randomseed = math.randomseed
local os_clock = os.clock
local os_time  = os.time

local PI = math.pi

-------------------------------------------------------------------------------
-- Parameters (matching original C++ defaults)
-------------------------------------------------------------------------------
local wave_frequency    = 1    -- 1–20,  default 1
local wave_speed        = 50   -- 1–200, default 50
local oscillation_speed = 100  -- 1–200, default 100
local random_enabled    = false

-------------------------------------------------------------------------------
-- Internal state
-------------------------------------------------------------------------------
local dir           = true   -- oscillation direction (true = rising)
local sine_progress = 0.0    -- amplitude envelope, oscillates in [-1, 1]
local wave_progress = 0.0    -- scroll position, wraps in [0, 100)
local last_t        = nil    -- previous elapsed timestamp for delta calculation

-------------------------------------------------------------------------------
-- Colors
-------------------------------------------------------------------------------
local user_colors = {
    { r = 255, g = 0, b = 0 },
    { r = 0,   g = 0, b = 255 },
}

local random_colors = {
    { r = 255, g = 0, b = 0 },
    { r = 0,   g = 255, b = 0 },
}

-------------------------------------------------------------------------------
-- Helpers
-------------------------------------------------------------------------------
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

local function update_user_colors(colors)
    if type(colors) ~= "table" or #colors < 2 then
        return
    end

    local first  = parse_hex_color(colors[1])
    local second = parse_hex_color(colors[2])
    if not first or not second then
        return
    end

    user_colors[1] = first
    user_colors[2] = second
end

local function random_rgb_color()
    local r, g, b = host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
    return { r = r, g = g, b = b }
end

--- Generate a random color and its inverse (matches ColorUtils::Invert).
local function refresh_random_colors()
    local color = random_rgb_color()
    random_colors[1] = color
    random_colors[2] = { r = 255 - color.r, g = 255 - color.g, b = 255 - color.b }
end

--- Linear interpolation per channel — matches ColorUtils::InterpolateChanel.
--- (int)((b - a) * fraction + a)
local function interpolate_channel(a, b, fraction)
    return math_floor((b - a) * fraction + a)
end

--- Compute the color for position `i` out of `count` LEDs/columns.
--- Faithfully replicates Wavy::GetColor from the C++ reference.
local function get_color(i, count, colors)
    local pos         = (i + (count * wave_progress) / 100.0) / count
    local rad         = 2.0 * PI * pos
    local wave_height = sine_progress * math_sin(wave_frequency * rad)
    local h           = 0.5 + wave_height / 2.0

    local c1, c2 = colors[1], colors[2]

    return interpolate_channel(c1.r, c2.r, h),
           interpolate_channel(c1.g, c2.g, h),
           interpolate_channel(c1.b, c2.b, h)
end

-------------------------------------------------------------------------------
-- Plugin lifecycle
-------------------------------------------------------------------------------
function plugin.on_init()
    math_randomseed(os_time(), math_floor((os_clock() % 1) * 1000000))
    refresh_random_colors()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.wave_frequency) == "number" then
        wave_frequency = math_max(1, math_min(20, math_floor(p.wave_frequency + 0.5)))
    end

    if type(p.wave_speed) == "number" then
        wave_speed = math_max(1, math_min(200, math_floor(p.wave_speed + 0.5)))
    end

    if type(p.oscillation_speed) == "number" then
        oscillation_speed = math_max(1, math_min(200, math_floor(p.oscillation_speed + 0.5)))
    end

    if type(p.random) == "boolean" then
        local was_random = random_enabled
        random_enabled = p.random
        if random_enabled and not was_random then
            refresh_random_colors()
        end
    end

    if type(p.colors) == "table" then
        update_user_colors(p.colors)
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

    local colors = random_enabled and random_colors or user_colors

    ---------------------------------------------------------------------------
    -- 1. Render — matches the C++ StepEffect inner loops.
    --    Linear: GetColor(LedID, leds_count)       → width == n when height == 1
    --    Matrix: GetColor(col_id, cols)             → color per column, same for all rows
    ---------------------------------------------------------------------------
    local led = 1
    for _y = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                break
            end
            local r, g, b = get_color(x, width, colors)
            buffer:set(led, r, g, b)
            led = led + 1
        end
        if led > n then break end
    end

    ---------------------------------------------------------------------------
    -- 2. Compute delta time
    ---------------------------------------------------------------------------
    local delta = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            delta = t
        else
            delta = t - last_t
        end
        last_t = t
    end

    ---------------------------------------------------------------------------
    -- 3. Update sine oscillation
    --    C++ per-frame: 0.01 * OscillationSpeed / FPS
    --    Per-second:    0.01 * OscillationSpeed  (FPS cancels out)
    ---------------------------------------------------------------------------
    local sine_inc = delta * 0.01 * oscillation_speed

    if dir then
        if sine_progress < 1 then
            sine_progress = sine_progress + sine_inc
        else
            dir = false
            sine_progress = sine_progress - sine_inc
        end
    else
        if sine_progress > -1 then
            sine_progress = sine_progress - sine_inc
        else
            dir = true
            sine_progress = sine_progress + sine_inc
        end
    end

    -- Refresh random colors when oscillation crosses zero
    if random_enabled and sine_progress >= -0.01 and sine_progress <= 0.01 then
        refresh_random_colors()
    end

    -- Clamp to [-1, 1]
    sine_progress = math_min(sine_progress, 1.0)
    sine_progress = math_max(sine_progress, -1.0)

    ---------------------------------------------------------------------------
    -- 4. Update wave scroll
    --    C++ per-frame: 0.05 * WaveSpeed / FPS
    --    Per-second:    0.05 * WaveSpeed
    ---------------------------------------------------------------------------
    local wave_inc = delta * 0.05 * wave_speed

    if wave_progress < 100 then
        wave_progress = wave_progress + wave_inc
    else
        wave_progress = 0.0
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
