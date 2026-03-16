local plugin = {}

local FPS = 60.0
local UINT32 = 4294967296.0

local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_random = math.random

local speed = 50
local random_enabled = false
local user_r, user_g, user_b = 255, 0, 0

-- Match the reference effect's initial runtime state exactly.
local progress = 0.0
local last_progress = 0
local spacing = 1
local speed_mult = 0.5
local progress_mult = 0.5
local dir = false
local random_hue = 0

local last_t = nil
local frame_accumulator = 0.0

local function clamp(value, min_value, max_value)
    return math_max(min_value, math_min(max_value, value))
end

local function trunc_to_int(value)
    if value >= 0 then
        return math_floor(value)
    end
    return -math_floor(-value)
end

local function wrap_u32(value)
    local wrapped = value % UINT32
    if wrapped < 0 then
        wrapped = wrapped + UINT32
    end
    return wrapped
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

    return tonumber(hex:sub(1, 2), 16) or 255,
        tonumber(hex:sub(3, 4), 16) or 0,
        tonumber(hex:sub(5, 6), 16) or 0
end

local function custom_rand(min_value, max_value)
    return min_value + math_random() * (max_value - min_value)
end

local function reset_state()
    progress = 0.0
    last_progress = 0
    spacing = 1
    speed_mult = 0.5
    progress_mult = 0.5
    dir = false
    random_hue = 0
    last_t = nil
    frame_accumulator = 0.0
end

local function step_state()
    progress = progress + (0.005 * speed * progress_mult / FPS)

    local current_progress = math_floor(progress)
    if last_progress ~= current_progress then
        last_progress = current_progress
        speed_mult = custom_rand(0.5, 1.5)
        progress_mult = custom_rand(0.5, 1.5)
        dir = math_random(0, 1) == 0
        spacing = 1 + math_random(0, 2)

        if random_enabled then
            random_hue = math_random(0, 359)
        end
    end
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

local function active_color()
    if random_enabled then
        return host.hsv_to_rgb(random_hue, 1.0, 1.0)
    end

    return user_r, user_g, user_b
end

local function is_lit_column(x)
    local direction = dir and -1 or 1
    local shift = trunc_to_int(direction * 20.0 * progress * speed_mult)
    local modulus = 2 * spacing

    -- The reference implementation mixes `unsigned int` and `int`.
    -- Reproduce the 32-bit wrap so reverse motion aligns exactly.
    local sum = (wrap_u32(x) + wrap_u32(shift)) % UINT32
    return (sum % modulus) == 0
end

function plugin.on_init()
    math.randomseed(os.time())
    reset_state()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = clamp(p.speed, 1, 200)
    end

    if type(p.random) == "boolean" then
        random_enabled = p.random
    end

    if type(p.color) == "string" then
        local r, g, b = parse_hex_color(p.color)
        if r ~= nil then
            user_r, user_g, user_b = r, g, b
        end
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

    local lit_r, lit_g, lit_b = active_color()
    local led = 1

    -- Match the reference effect: X drives the marquee and each row repeats it.
    for _ = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                advance_state(t)
                return
            end

            if is_lit_column(x) then
                buffer:set(led, lit_r, lit_g, lit_b)
            else
                buffer:set(led, 0, 0, 0)
            end

            led = led + 1
        end
    end

    -- Keep the same order as the C++ effect: render first, then advance.
    advance_state(t)
end

function plugin.on_shutdown()
    last_t = nil
    frame_accumulator = 0.0
end

return plugin
