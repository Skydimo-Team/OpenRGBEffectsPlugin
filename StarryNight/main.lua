local plugin = {}

-- Localize hot-path math functions
local math_floor = math.floor
local math_ceil  = math.ceil
local math_min   = math.min
local math_max   = math.max
local math_random = math.random

----------------------------------------------------------------------------
-- Star states (matching original C++ StarState enum)
----------------------------------------------------------------------------
local OFF      = 0
local DELAYED  = 1
local FADE_IN  = 2
local ON       = 3
local FADE_OUT = 4

----------------------------------------------------------------------------
-- Constants (exact values from original C++ StarryNight.h)
----------------------------------------------------------------------------
local MIN_DELAY_TIME = 0.0   -- seconds
local MAX_DELAY_TIME = 1.0   -- seconds
local MIN_ON_TIME    = 1.0   -- seconds
local MAX_ON_TIME    = 3.0   -- seconds
local ON_RANGE_SEL   = 50.0  -- reference slider value for on-time range
local MIN_FADE_TIME  = 1.0   -- seconds
local MAX_FADE_TIME  = 3.0   -- seconds
local FADE_RANGE_SEL = 50.0  -- reference slider value for fade range

----------------------------------------------------------------------------
-- Configuration (matches manifest defaults)
----------------------------------------------------------------------------
local bg_r, bg_g, bg_b = 0, 0, 0
local bg_brightness    = 50
local random_enabled   = false
local density          = 50
local fade_in_speed    = 50
local fade_out_speed   = 50
local star_on_time     = 50

local palette = {
    { r = 255, g = 255, b = 255 },  -- White
    { r = 136, g = 204, b = 255 },  -- Light blue
    { r = 255, g = 204, b = 68  },  -- Warm yellow
}

----------------------------------------------------------------------------
-- State
----------------------------------------------------------------------------
local stars      = {}   -- array of star tables
local occupied   = {}   -- set: led_idx -> true (prevent duplicate stars)
local total_leds = 0
local seeded     = false

----------------------------------------------------------------------------
-- Color helpers
----------------------------------------------------------------------------

--- Parse "#RRGGBB" hex string to {r, g, b} table (0-255)
local function hex_to_rgb(hex)
    if type(hex) ~= "string" then return nil end
    hex = hex:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then hex = hex:sub(2) end
    if #hex == 3 then
        hex = hex:sub(1, 1):rep(2) .. hex:sub(2, 2):rep(2) .. hex:sub(3, 3):rep(2)
    end
    if #hex ~= 6 or hex:find("[^%x]") then return nil end
    return {
        r = tonumber(hex:sub(1, 2), 16) or 0,
        g = tonumber(hex:sub(3, 4), 16) or 0,
        b = tonumber(hex:sub(5, 6), 16) or 0,
    }
end

--- Background color with brightness applied
--- Matches: ColorUtils::apply_brightness(backColor, backColorBrightness / 100)
local function get_bg()
    local f = bg_brightness / 100
    return math_floor(bg_r * f + 0.5),
           math_floor(bg_g * f + 0.5),
           math_floor(bg_b * f + 0.5)
end

--- Linear interpolation of two RGB colors
--- Matches: ColorUtils::Interpolate(colorA, colorB, t)
local function lerp_rgb(r1, g1, b1, r2, g2, b2, t)
    t = math_max(0, math_min(1, t))
    return math_floor(r1 + (r2 - r1) * t + 0.5),
           math_floor(g1 + (g2 - g1) * t + 0.5),
           math_floor(b1 + (b2 - b1) * t + 0.5)
end

--- Pick a star color based on the current color mode
local function pick_star_color()
    if random_enabled then
        return host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
    end
    if #palette == 0 then return 255, 255, 255 end
    local c = palette[math_random(1, #palette)]
    return c.r, c.g, c.b
end

----------------------------------------------------------------------------
-- Timing helpers (reproduce original C++ random distributions exactly)
--
-- Original formulas (C++):
--   delay:   uniform_real(minDelayTime/10, maxDelayTime/10) * 10000  (in ms)
--            = uniform(0.0, 0.1) * 10000 = [0, 1000] ms = [0, 1] s
--
--   fadeIn:  uniform_real(minFadeTime/10, maxFadeTime/10)
--            * fadeInSpeed / fadeRangeSelector * 10000  (in ms)
--            = uniform(0.1, 0.3) * speed/50 * 10000
--            → at speed=50: [1000, 3000] ms = [1, 3] s
--
--   onTime:  uniform_real(minOnTime/10, maxOnTime/10)
--            * starOnTimeSpeed / onRangeSelector * 10000  (in ms)
--            = uniform(0.1, 0.3) * speed/50 * 10000
--            → at speed=50: [1000, 3000] ms = [1, 3] s
--
--   fadeOut:  same formula as fadeIn but with fadeOutSpeed
--
-- Simplified to seconds:
--   delay  = random[0, 1]
--   fadeIn = random[1, 3] * fadeInSpeed / 50
--   onTime = random[1, 3] * starOnTimeSpeed / 50
--   fadeOut = random[1, 3] * fadeOutSpeed / 50
----------------------------------------------------------------------------

local function random_delay()
    return MIN_DELAY_TIME + math_random() * (MAX_DELAY_TIME - MIN_DELAY_TIME)
end

local function random_fade_period(speed_value)
    local base = MIN_FADE_TIME + math_random() * (MAX_FADE_TIME - MIN_FADE_TIME)
    return base * speed_value / FADE_RANGE_SEL
end

local function random_on_period()
    local base = MIN_ON_TIME + math_random() * (MAX_ON_TIME - MIN_ON_TIME)
    return base * star_on_time / ON_RANGE_SEL
end

----------------------------------------------------------------------------
-- Star management
----------------------------------------------------------------------------

--- Find a random unoccupied LED index (1-based)
--- Matches original: loop generating random indices, check against existing stars
local function find_free_led(n)
    -- Quick path: try random indices (same strategy as original's while loop)
    for _ = 1, n do
        local idx = math_random(1, n)
        if not occupied[idx] then
            return idx
        end
    end
    -- Fallback: linear scan for a free slot
    for i = 1, n do
        if not occupied[i] then
            return i
        end
    end
    return nil
end

--- Activate a star at stars[i] with a new random LED
local function activate_star(i, n, elapsed)
    local led = find_free_led(n)
    if not led then return end

    -- Release previous LED if any
    local old = stars[i]
    if old and old.led then
        occupied[old.led] = nil
    end

    local cr, cg, cb = pick_star_color()
    local bgr, bgg, bgb = get_bg()

    stars[i] = {
        -- Target color when the star is fully "on"
        color_r = cr, color_g = cg, color_b = cb,
        -- Intermediate displayed color (starts as background)
        int_r = bgr, int_g = bgg, int_b = bgb,
        -- State machine
        state       = DELAYED,
        state_start = elapsed,
        period      = random_delay(),
        -- LED mapping (1-based index)
        led = led,
    }
    occupied[led] = true
end

--- Advance one star through its state machine
--- Faithfully reproduces the original UpdateStarInfo() logic
local function update_star(star, elapsed)
    if not star then return end

    local dt = elapsed - star.state_start
    local bgr, bgg, bgb = get_bg()

    if star.state == DELAYED then
        -- Waiting for random delay to expire before fading in
        -- During delay, display background color
        star.int_r, star.int_g, star.int_b = bgr, bgg, bgb
        if dt >= star.period then
            star.state       = FADE_IN
            star.state_start = elapsed
            star.period      = random_fade_period(fade_in_speed)
        end

    elseif star.state == FADE_IN then
        if dt >= star.period then
            -- Fade in complete → transition to ON
            star.state       = ON
            star.state_start = elapsed
            star.period      = random_on_period()
            star.int_r = star.color_r
            star.int_g = star.color_g
            star.int_b = star.color_b
        else
            -- Interpolate: background → star color
            local t = dt / star.period
            star.int_r, star.int_g, star.int_b = lerp_rgb(
                bgr, bgg, bgb,
                star.color_r, star.color_g, star.color_b,
                t
            )
        end

    elseif star.state == ON then
        -- Star at full brightness
        star.int_r = star.color_r
        star.int_g = star.color_g
        star.int_b = star.color_b
        if dt >= star.period then
            -- On period elapsed → transition to FADE_OUT
            star.state       = FADE_OUT
            star.state_start = elapsed
            star.period      = random_fade_period(fade_out_speed)
        end

    elseif star.state == FADE_OUT then
        if dt >= star.period then
            -- Fade out complete → transition to OFF
            star.state = OFF
            star.int_r, star.int_g, star.int_b = bgr, bgg, bgb
        else
            -- Interpolate: star color → background
            local t = dt / star.period
            star.int_r, star.int_g, star.int_b = lerp_rgb(
                star.color_r, star.color_g, star.color_b,
                bgr, bgg, bgb,
                t
            )
        end
    end
end

----------------------------------------------------------------------------
-- Plugin lifecycle
----------------------------------------------------------------------------

function plugin.on_init()
    if not seeded then
        math.randomseed(os.clock() * 1000000)
        seeded = true
    end
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end

    if type(p.background) == "string" then
        local c = hex_to_rgb(p.background)
        if c then
            bg_r, bg_g, bg_b = c.r, c.g, c.b
        end
    end
    if type(p.bg_brightness) == "number" then
        bg_brightness = p.bg_brightness
    end
    if type(p.random) == "boolean" then
        random_enabled = p.random
    end
    if type(p.density) == "number" then
        density = p.density
    end
    if type(p.fade_in_speed) == "number" then
        fade_in_speed = p.fade_in_speed
    end
    if type(p.fade_out_speed) == "number" then
        fade_out_speed = p.fade_out_speed
    end
    if type(p.star_on_time) == "number" then
        star_on_time = p.star_on_time
    end
    if type(p.colors) == "table" then
        local new_palette = {}
        for i = 1, #p.colors do
            local c = hex_to_rgb(p.colors[i])
            if c then
                new_palette[#new_palette + 1] = c
            end
        end
        if #new_palette > 0 then
            palette = new_palette
        end
    end
end

----------------------------------------------------------------------------
-- Render
--
-- Original StepEffect loop:
--   1. If background changed, set ALL LEDs to background color
--   2. UpdateStarInfo() — advance every star's state machine
--   3. For each star, set its LED to star.intColor
--
-- Our approach (buffer is fully rewritten each frame):
--   1. Fill entire buffer with background color
--   2. Advance star state machines
--   3. Overlay star colors onto their assigned LEDs
----------------------------------------------------------------------------

function plugin.on_tick(elapsed, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    -- Reset if LED count changed (mirrors OnControllerZonesListChanged)
    if total_leds ~= n then
        stars      = {}
        occupied   = {}
        total_leds = n
    end

    -- Target star count: ceil(totalLEDs * density / 100)
    -- Matches original: ceil(double(totalLEDs) * double(density) / 100)
    local target_count = math_ceil(n * density / 100)
    target_count = math_min(target_count, n)
    local current_count = #stars

    -- Adjust star array size
    if target_count > current_count then
        -- Grow: activate new stars
        for i = current_count + 1, target_count do
            activate_star(i, n, elapsed)
        end
    elseif target_count < current_count then
        -- Shrink: remove excess stars (release their LEDs)
        -- Original marks them "inactive" and waits a frame; since we write
        -- the full buffer each frame, immediate removal is visually identical.
        for i = current_count, target_count + 1, -1 do
            if stars[i] and stars[i].led then
                occupied[stars[i].led] = nil
            end
            stars[i] = nil
        end
    end

    -- Update all stars and handle OFF → reactivate cycle
    for i = 1, #stars do
        local star = stars[i]
        if star then
            update_star(star, elapsed)

            -- Star completed full cycle (OFF) → reactivate with new random LED
            if star.state == OFF then
                if star.led then
                    occupied[star.led] = nil
                end
                activate_star(i, n, elapsed)
            end
        end
    end

    -- 1) Fill entire buffer with background color
    local bgr, bgg, bgb = get_bg()
    for i = 1, n do
        buffer:set(i, bgr, bgg, bgb)
    end

    -- 2) Overlay star intermediate colors
    for i = 1, #stars do
        local star = stars[i]
        if star and star.led and star.led >= 1 and star.led <= n then
            buffer:set(star.led, star.int_r, star.int_g, star.int_b)
        end
    end
end

function plugin.on_shutdown()
    stars      = {}
    occupied   = {}
    total_leds = 0
end

return plugin
