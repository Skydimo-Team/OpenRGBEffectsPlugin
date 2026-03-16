-- SparkleFade effect
-- Port of OpenRGBEffectsPlugin SparkleFade
--
-- State machine:
--   OFF → FADE_IN → ON (instant) → FADE_OUT → OFF …
--
-- FADE_IN : all LEDs linearly fade from black to base color
-- ON      : transition frame — assigns each LED a random fade-start delay
--           and random fade duration, then immediately enters FADE_OUT
-- FADE_OUT: each LED independently fades from base color to black at its
--           own random pace, creating the "sparkle" appearance
-- OFF     : all LEDs stay black for off_time seconds before the next cycle

local plugin = {}

local math_floor = math.floor
local math_random = math.random
local math_max = math.max

-- States
local STATE_OFF = 0
local STATE_FADE_IN = 1
local STATE_ON = 2
local STATE_FADE_OUT = 3

-- Parameters (seconds)
local off_time = 0.5
local fade_in_time = 3.0
local fade_out_time = 10.0
local random_enabled = false
local user_r, user_g, user_b = 255, 0, 0

-- Runtime state
local current_state = STATE_OFF
local state_start_time = 0.0
local base_r, base_g, base_b = 255, 0, 0

-- Per-LED fade-out scheduling (flat arrays, 1-indexed)
local led_fade_start = {}   -- absolute time when this LED begins fading
local led_fade_period = {}  -- duration of this LED's individual fade
local led_count = 0

local seeded = false

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
    return tonumber(hex:sub(1, 2), 16) or 255,
        tonumber(hex:sub(3, 4), 16) or 0,
        tonumber(hex:sub(5, 6), 16) or 0
end

local function pick_random_color()
    return host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    if not seeded then
        math.randomseed(math_floor(os.clock() * 1000000))
        seeded = true
    end
    current_state = STATE_OFF
    state_start_time = 0.0
    led_count = 0
    led_fade_start = {}
    led_fade_period = {}
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end
    if type(p.off_time) == "number" then
        off_time = p.off_time
    end
    if type(p.fade_in_time) == "number" then
        fade_in_time = p.fade_in_time
    end
    if type(p.fade_out_time) == "number" then
        fade_out_time = p.fade_out_time
    end
    if type(p.random) == "boolean" then
        random_enabled = p.random
    end
    if type(p.color) == "string" then
        local r, g, b = parse_hex_color(p.color)
        if r then
            user_r, user_g, user_b = r, g, b
        end
    end
end

---------------------------------------------------------------------------
-- Render
--
-- Faithfully reproduces the C++ SparkleFade state machine.
--
-- Reference fade-out scheduling (original C++ with fadeOutTime in ms):
--   distribution range = [0, fadeOutTime / 20000]
--   startDelay  = distribution_sample * 10000  →  effective [0, fadeOutTime/2] ms
--   fadePeriod  = distribution_sample * 10000  →  effective [0, fadeOutTime/2] ms
--   ⇒ worst-case total = fadeOutTime/2 + fadeOutTime/2 = fadeOutTime
--
-- Translated here with all times in seconds:
--   max_half = fade_out_time / 2
--   led_fade_start[i]  = now + random() * max_half
--   led_fade_period[i] = random() * max_half   (clamped ≥ 1 ms)
---------------------------------------------------------------------------

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then
        return
    end

    -- Resize per-LED arrays when LED count changes
    if n ~= led_count then
        led_count = n
        led_fade_start = {}
        led_fade_period = {}
        for i = 1, n do
            led_fade_start[i] = 0.0
            led_fade_period[i] = 0.5
        end
    end

    local elapsed_in_state = t - state_start_time

    -----------------------------------------------------------------------
    -- STATE: OFF
    -----------------------------------------------------------------------
    if current_state == STATE_OFF then
        for i = 1, n do
            buffer:set(i, 0, 0, 0)
        end

        if elapsed_in_state >= off_time then
            current_state = STATE_FADE_IN
            state_start_time = t

            -- Choose base color for the upcoming cycle
            if random_enabled then
                base_r, base_g, base_b = pick_random_color()
            else
                base_r, base_g, base_b = user_r, user_g, user_b
            end
        end

    -----------------------------------------------------------------------
    -- STATE: FADE_IN  (all LEDs fade from black → base color)
    -----------------------------------------------------------------------
    elseif current_state == STATE_FADE_IN then
        if elapsed_in_state >= fade_in_time then
            -- Fade in complete → ON (transition)
            current_state = STATE_ON
            state_start_time = t
        else
            -- Linear interpolation: OFF → baseColor
            local mult = elapsed_in_state / fade_in_time
            local r = math_floor(base_r * mult + 0.5)
            local g = math_floor(base_g * mult + 0.5)
            local b = math_floor(base_b * mult + 0.5)
            for i = 1, n do
                buffer:set(i, r, g, b)
            end
        end

    -----------------------------------------------------------------------
    -- STATE: ON  (instant transition — schedule per-LED fade-out)
    -----------------------------------------------------------------------
    elseif current_state == STATE_ON then
        local max_half = math_max(fade_out_time / 2.0, 0.001)

        for i = 1, n do
            led_fade_start[i] = t + math_random() * max_half
            local period = math_random() * max_half
            -- Clamp to avoid zero-duration fades
            if period < 0.001 then
                period = 0.001
            end
            led_fade_period[i] = period
        end

        current_state = STATE_FADE_OUT
        state_start_time = t

        -- This frame: all LEDs at full base color
        for i = 1, n do
            buffer:set(i, base_r, base_g, base_b)
        end

    -----------------------------------------------------------------------
    -- STATE: FADE_OUT  (each LED fades independently → sparkle)
    -----------------------------------------------------------------------
    elseif current_state == STATE_FADE_OUT then
        local all_faded = true

        for i = 1, n do
            local fs = led_fade_start[i]
            local fp = led_fade_period[i]

            if t >= fs + fp then
                -- This LED has fully faded out
                buffer:set(i, 0, 0, 0)
            else
                all_faded = false

                if t >= fs then
                    -- Currently fading: linear from baseColor → black
                    local progress = (t - fs) / fp
                    local inv = 1.0 - progress
                    buffer:set(
                        i,
                        math_floor(base_r * inv + 0.5),
                        math_floor(base_g * inv + 0.5),
                        math_floor(base_b * inv + 0.5)
                    )
                else
                    -- Hasn't started fading yet — still at base color
                    buffer:set(i, base_r, base_g, base_b)
                end
            end
        end

        if all_faded then
            current_state = STATE_OFF
            state_start_time = t
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
