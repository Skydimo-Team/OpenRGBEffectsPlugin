local plugin = {}

---------------------------------------------------------------------------
-- Localize hot-path math functions
---------------------------------------------------------------------------
local PI         = 3.14159265358979323846
local math_sin   = math.sin
local math_sqrt  = math.sqrt
local math_min   = math.min
local math_floor = math.floor
local math_random = math.random

---------------------------------------------------------------------------
-- Parameters (match manifest defaults)
---------------------------------------------------------------------------
local interval       = 2.0     -- Idle duration (seconds)
local pulses         = 2       -- Number of sine pulses
local pulse_duration = 0.5     -- Duration of each pulse (seconds)
local strength       = 0.5     -- Blink depth 0.0-1.0 (slider 0-100 → mapped)
local rendering      = 0       -- 0 = Solid, 1 = Circle
local cx_shift       = 50      -- Circle center X (0-100%)
local cy_shift       = 50      -- Circle center Y (0-100%)
local random_enabled = false

-- User colors (RGB 0-255)
local color1_r, color1_g, color1_b = 0, 0, 0        -- Blink-to (pulse peak)
local color2_r, color2_g, color2_b = 255, 0, 0      -- Base (idle)

---------------------------------------------------------------------------
-- Random color state (matches C++ constructor init)
---------------------------------------------------------------------------
local random_color1_r, random_color1_g, random_color1_b = 0, 0, 0
local random_color2_r, random_color2_g, random_color2_b = 0, 0, 0
local next_color1_r, next_color1_g, next_color1_b = 0, 0, 0
local next_color2_r, next_color2_g, next_color2_b = 0, 0, 0

---------------------------------------------------------------------------
-- Animation state
---------------------------------------------------------------------------
local last_cycle = -1

---------------------------------------------------------------------------
-- Helpers
---------------------------------------------------------------------------

local function hex_to_rgb(hex)
    if type(hex) ~= "string" then return 0, 0, 0 end
    hex = hex:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then hex = hex:sub(2) end
    if #hex == 3 then
        hex = hex:sub(1, 1):rep(2)
           .. hex:sub(2, 2):rep(2)
           .. hex:sub(3, 3):rep(2)
    end
    if #hex ~= 6 or hex:find("[^%x]") then return 0, 0, 0 end
    return tonumber(hex:sub(1, 2), 16) or 0,
           tonumber(hex:sub(3, 4), 16) or 0,
           tonumber(hex:sub(5, 6), 16) or 0
end

local function random_rgb()
    return host.hsv_to_rgb(math_random() * 360, 1.0, 1.0)
end

local function lerp(a, b, t)
    return math_floor(a + (b - a) * t + 0.5)
end

local function lerp_rgb(r1, g1, b1, r2, g2, b2, t)
    return lerp(r1, r2, t), lerp(g1, g2, t), lerp(b1, b2, t)
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    math.randomseed(os.time())
    -- Match C++ constructor: four independent random colors
    random_color1_r, random_color1_g, random_color1_b = random_rgb()
    random_color2_r, random_color2_g, random_color2_b = random_rgb()
    next_color1_r, next_color1_g, next_color1_b = random_rgb()
    next_color2_r, next_color2_g, next_color2_b = random_rgb()
    last_cycle = -1
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end
    if type(p.interval) == "number"       then interval       = p.interval end
    if type(p.pulses) == "number"         then pulses         = p.pulses end
    if type(p.pulse_duration) == "number" then pulse_duration = p.pulse_duration end
    if type(p.strength) == "number"       then strength       = p.strength / 100.0 end
    if type(p.rendering) == "number"      then rendering      = p.rendering end
    if type(p.cx) == "number"             then cx_shift       = p.cx end
    if type(p.cy) == "number"             then cy_shift       = p.cy end
    if type(p.random) == "boolean"        then random_enabled = p.random end
    if type(p.color1) == "string"         then
        color1_r, color1_g, color1_b = hex_to_rgb(p.color1)
    end
    if type(p.color2) == "string"         then
        color2_r, color2_g, color2_b = hex_to_rgb(p.color2)
    end
end

---------------------------------------------------------------------------
-- Render
--
-- Faithfully reproduces the reference C++ SmoothBlink effect:
--
--   total_duration = interval + pulses * pulse_duration
--   s = 0.5 + (1 - strength) * 0.5
--
--   if time < interval:
--       value = 1.0                                     (idle, shows color2)
--   else:
--       x = time - interval
--       y = 0.5 * (1 + sin(2*pulses/pulses_total * x * PI - PI/2))
--       value = y - (y - s) / s                         (pulsed blink)
--
--   Solid:  all LEDs = lerp(color1, color2, value)
--   Circle: per-pixel blend based on distance from (cx, cy)
--
-- Random color mode: crossfade to new random colors during first half
-- of each interval period, matching C++ random_fade_timer logic.
---------------------------------------------------------------------------

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    if type(width) ~= "number" or width <= 0 then width = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    -- Effect timing
    local pulses_total_duration = pulses * pulse_duration
    local total_effect_duration = interval + pulses_total_duration

    -- Current cycle and time within it
    local cycle = math_floor(t / total_effect_duration)
    local time_in_cycle = t - cycle * total_effect_duration

    -------------------------------------------------------------------
    -- Random color cycling
    -- On each new cycle: rotate next → current, pick fresh next pair.
    -- random_fade_timer ≡ time_in_cycle (both reset at cycle boundary).
    -------------------------------------------------------------------
    if cycle ~= last_cycle then
        last_cycle = cycle
        random_color1_r, random_color1_g, random_color1_b =
            next_color1_r, next_color1_g, next_color1_b
        random_color2_r, random_color2_g, random_color2_b =
            next_color2_r, next_color2_g, next_color2_b
        next_color1_r, next_color1_g, next_color1_b = random_rgb()
        next_color2_r, next_color2_g, next_color2_b = random_rgb()
    end

    -------------------------------------------------------------------
    -- Resolve current color pair
    -------------------------------------------------------------------
    local cur1_r, cur1_g, cur1_b
    local cur2_r, cur2_g, cur2_b

    if random_enabled then
        -- Crossfade during first half of interval (matches C++ random_fade_timer)
        local half_interval = 0.5 * interval
        if half_interval > 0 and time_in_cycle <= half_interval then
            local fade_t = time_in_cycle / half_interval
            cur1_r, cur1_g, cur1_b = lerp_rgb(
                random_color1_r, random_color1_g, random_color1_b,
                next_color1_r, next_color1_g, next_color1_b, fade_t)
            cur2_r, cur2_g, cur2_b = lerp_rgb(
                random_color2_r, random_color2_g, random_color2_b,
                next_color2_r, next_color2_g, next_color2_b, fade_t)
        else
            -- Snap to next colors (matches C++ else branch)
            cur1_r, cur1_g, cur1_b =
                next_color1_r, next_color1_g, next_color1_b
            cur2_r, cur2_g, cur2_b =
                next_color2_r, next_color2_g, next_color2_b
        end
    else
        cur1_r, cur1_g, cur1_b = color1_r, color1_g, color1_b
        cur2_r, cur2_g, cur2_b = color2_r, color2_g, color2_b
    end

    -------------------------------------------------------------------
    -- Compute blink value
    --   s maps strength: 0→1.0 (no blink), 1→0.5 (full blink)
    --   During interval: value = 1.0 (idle, shows color2)
    --   During pulses:   value oscillates via sine
    -------------------------------------------------------------------
    local s = 0.5 + (1 - strength) * 0.5
    local value

    if time_in_cycle < interval then
        value = 1.0
    else
        local x = time_in_cycle - interval
        local y = 0.5 * (1 + math_sin(
            2 * pulses / pulses_total_duration * x * PI - 0.5 * PI))
        value = y - (y - s) / s
    end

    -------------------------------------------------------------------
    -- Rendering
    -------------------------------------------------------------------
    if rendering == 1 then
        -- Circle rendering (matches C++ HandleCircleRendering)
        local cx_mult = cx_shift / 100.0
        local cy_mult = cy_shift / 100.0

        local max_distance
        if height <= 1 then
            -- Linear / Single: cx based on width, cy = 0
            local cx = (width - 1) * cx_mult
            local cy_val = 0
            max_distance = width

            local led = 1
            for col = 0, width - 1 do
                if led > n then break end
                local r, g, b
                if value >= 1.0 then
                    r, g, b = cur2_r, cur2_g, cur2_b
                else
                    local dx = cx - col
                    local distance = math_sqrt(dx * dx)
                    local distance_percent = (max_distance > 0)
                        and (distance / max_distance) or 0
                    local blend = math_min(1.0, value + distance_percent)
                    r, g, b = lerp_rgb(
                        cur1_r, cur1_g, cur1_b,
                        cur2_r, cur2_g, cur2_b, blend)
                end
                buffer:set(led, r, g, b)
                led = led + 1
            end
        else
            -- Matrix: cx/cy based on width/height
            local cx = (width - 1) * cx_mult
            local cy_val = (height - 1) * cy_mult
            max_distance = width + height

            local led = 1
            for row = 0, height - 1 do
                for col = 0, width - 1 do
                    if led > n then return end
                    local r, g, b
                    if value >= 1.0 then
                        r, g, b = cur2_r, cur2_g, cur2_b
                    else
                        local dx = cx - col
                        local dy = cy_val - row
                        local distance = math_sqrt(dx * dx + dy * dy)
                        local distance_percent = (max_distance > 0)
                            and (distance / max_distance) or 0
                        local blend = math_min(1.0, value + distance_percent)
                        r, g, b = lerp_rgb(
                            cur1_r, cur1_g, cur1_b,
                            cur2_r, cur2_g, cur2_b, blend)
                    end
                    buffer:set(led, r, g, b)
                    led = led + 1
                end
            end
        end
    else
        -- Solid rendering (matches C++ HandleSolidRendering)
        local r, g, b = lerp_rgb(
            cur1_r, cur1_g, cur1_b,
            cur2_r, cur2_g, cur2_b, value)
        for i = 1, n do
            buffer:set(i, r, g, b)
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
