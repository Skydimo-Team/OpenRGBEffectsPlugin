local plugin = {}

-- ---------------------------------------------------------------------------
-- RandomSpin
--
-- Each plugin instance corresponds to one zone.  The effect randomly
-- alternates between spinning (with random direction & speed) and pausing.
-- A two-colour gradient pattern scrolls through the LEDs.
-- ---------------------------------------------------------------------------

local FPS = 60.0

local math_abs   = math.abs
local math_floor = math.floor
local math_max   = math.max
local math_min   = math.min
local math_random = math.random

-- ── Parameters ──────────────────────────────────────────────────────────────

local speed = 50

-- User colours (RGB 0-255)
local color1_r, color1_g, color1_b = 255, 0, 0
local color2_r, color2_g, color2_b = 0, 0, 255

-- ── Pre-computed gradient (100 entries, indexed 0 .. 99) ────────────────────
-- Matches the Qt QLinearGradient in the original C++ code exactly.
-- Sorted stop positions after Qt's setColorAt de-duplication:
--   0.00 → C1,  0.15 → C1,  0.25 → C2,  0.35 → C1,
--   0.65 → C1,  0.75 → C2,  0.80 → C1,  0.85 → C1
-- PadSpread: colours beyond [0, 0.85] pad with the nearest stop colour.

local gradient = {}

local function lerp(a, b, t)
    return a + (b - a) * t
end

local function generate_gradient()
    -- Each stop: { position, r, g, b }
    local stops = {
        { 0.00, color1_r, color1_g, color1_b },
        { 0.15, color1_r, color1_g, color1_b },
        { 0.25, color2_r, color2_g, color2_b },
        { 0.35, color1_r, color1_g, color1_b },
        { 0.65, color1_r, color1_g, color1_b },
        { 0.75, color2_r, color2_g, color2_b },
        { 0.80, color1_r, color1_g, color1_b },
        { 0.85, color1_r, color1_g, color1_b },
    }

    local num_stops = #stops

    for x = 0, 99 do
        local t = x / 100.0
        local r, g, b

        if t <= stops[1][1] then
            -- Before first stop – pad
            r, g, b = stops[1][2], stops[1][3], stops[1][4]
        else
            local found = false
            for i = 2, num_stops do
                if t <= stops[i][1] then
                    local t0 = stops[i - 1][1]
                    local t1 = stops[i][1]
                    local frac = (t - t0) / (t1 - t0)
                    r = lerp(stops[i - 1][2], stops[i][2], frac)
                    g = lerp(stops[i - 1][3], stops[i][3], frac)
                    b = lerp(stops[i - 1][4], stops[i][4], frac)
                    found = true
                    break
                end
            end
            if not found then
                -- After last stop – pad
                local last = stops[num_stops]
                r, g, b = last[2], last[3], last[4]
            end
        end

        gradient[x] = {
            math_floor(r + 0.5),
            math_floor(g + 0.5),
            math_floor(b + 0.5),
        }
    end
end

-- ── Per-instance spin state (mirrors one RandomSpinEntry) ───────────────────

local entry_progress        = 0.0
local entry_stop_progress   = 0.0
local entry_speed_mult      = 1.0
local entry_dir             = true   -- true → dir flag set in C++ (velocity = -1)
local entry_stop            = true
local entry_next_time_point = 0.0

-- Global progress – serves as a monotonic clock for state transitions.
local progress = 0.0

-- ── Timing (fixed-step accumulation, same as random_marquee) ────────────────

local last_t = nil
local frame_accumulator = 0.0

-- ── Helpers ─────────────────────────────────────────────────────────────────

local function clamp(v, lo, hi)
    return math_max(lo, math_min(hi, v))
end

local function custom_rand(min_val, max_val)
    return min_val + math_random() * (max_val - min_val)
end

local function parse_hex_color(hex)
    if type(hex) ~= "string" then
        return nil
    end

    hex = hex:gsub("%s+", "")
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

-- ── Gradient colour lookup (mirrors RandomSpin::GetColor) ───────────────────

local function get_color(i, w)
    local ep = entry_stop and entry_stop_progress or entry_progress

    local percent = i / w
    percent = percent + math_abs(ep)
    percent = percent - math_floor(percent) -- wrap to [0, 1)

    local px = math_floor(percent * 100)
    if px > 99 then px = 99 end
    if px < 0  then px = 0  end

    local c = gradient[px]
    return c[1], c[2], c[3]
end

-- ── Fixed-step simulation (mirrors StepEffect per frame) ────────────────────

local function step_state()
    -- Check whether it is time to toggle between spinning and pausing.
    if entry_next_time_point < progress then
        entry_stop = not entry_stop
        entry_next_time_point = progress + custom_rand(1, entry_stop and 1.5 or 3.5)
        entry_speed_mult      = custom_rand(1.0, 5.0)
        entry_dir             = math_random(0, 1) == 0
        entry_stop_progress   = entry_progress
    else
        -- Advance the spin when not stopped.
        local dir_sign = entry_dir and -1 or 1
        entry_progress = entry_progress
            + dir_sign * entry_speed_mult * 0.01 * speed / FPS
    end

    -- Advance the global clock.
    progress = progress + 0.01 * speed / FPS
end

local function advance_state(t)
    local delta = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t == nil then
            delta = math_max(t, 1.0 / FPS)
        elseif t < last_t then
            delta = math_max(t, 1.0 / FPS)
        else
            delta = t - last_t
        end
        last_t = t
    end

    if delta <= 0 then
        return
    end

    frame_accumulator = frame_accumulator + (delta * FPS)
    while frame_accumulator >= 1.0 do
        step_state()
        frame_accumulator = frame_accumulator - 1.0
    end
end

-- ── Plugin lifecycle ────────────────────────────────────────────────────────

function plugin.on_init()
    math.randomseed(os.time())

    progress              = 0.0
    entry_progress        = 0.0
    entry_stop_progress   = 0.0
    entry_speed_mult      = 1.0
    entry_dir             = true
    entry_stop            = true
    entry_next_time_point = 0.0
    last_t                = nil
    frame_accumulator     = 0.0

    generate_gradient()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = clamp(p.speed, 1, 100)
    end

    local colors_changed = false

    if type(p.colors) == "table" then
        if type(p.colors[1]) == "string" then
            local r, g, b = parse_hex_color(p.colors[1])
            if r then
                color1_r, color1_g, color1_b = r, g, b
                colors_changed = true
            end
        end
        if type(p.colors[2]) == "string" then
            local r, g, b = parse_hex_color(p.colors[2])
            if r then
                color2_r, color2_g, color2_b = r, g, b
                colors_changed = true
            end
        end
    end

    if colors_changed then
        generate_gradient()
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

    -- Render – same order as C++: paint first, then advance state.
    local led = 1
    for _ = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                advance_state(t)
                return
            end

            local r, g, b = get_color(x, width)
            buffer:set(led, r, g, b)
            led = led + 1
        end
    end

    advance_state(t)
end

function plugin.on_shutdown()
    last_t = nil
    frame_accumulator = 0.0
end

return plugin
