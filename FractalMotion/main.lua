local plugin = {}

local FRAME_DT = 1.0 / 60.0
local VALUE_SCALE = 0.01

local math_abs = math.abs
local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_random = math.random
local math_sin = math.sin

local params = {
    background = "#000000",
    thickness = 2,
    speed = 50,
    amplitude = 100,
    frequency = 100,
    freq_m1 = 210,
    freq_m2 = 450,
    freq_m3 = 172,
    freq_m4 = 112,
    freq_m5 = 400,
    freq_m6 = 222,
    freq_m7 = 43,
    freq_m8 = 500,
    freq_m9 = 211,
    freq_m10 = 150,
    freq_m11 = 172,
    freq_m12 = 6,
    random = false,
    color = "#FF0000",
}

local background_r, background_g, background_b = 0, 0, 0
local user_r, user_g, user_b = 255, 0, 0

local progress = 0.0
local random_tick = 0.0
local last_time = nil
local time_carry = 0.0

local current_random = { r = 255, g = 0, b = 0 }
local next_random = { r = 255, g = 0, b = 0 }

local function clamp(value, min_value, max_value)
    if value < min_value then
        return min_value
    end
    if value > max_value then
        return max_value
    end
    return value
end

local function round_byte(value)
    return clamp(math_floor(value + 0.5), 0, 255)
end

local function hex_to_rgb(hex, default_r, default_g, default_b)
    if type(hex) ~= "string" then
        return default_r, default_g, default_b
    end

    local normalized = hex:gsub("%s+", "")
    if normalized:sub(1, 1) == "#" then
        normalized = normalized:sub(2)
    end

    if #normalized == 8 then
        normalized = normalized:sub(1, 6)
    elseif #normalized == 3 then
        normalized =
            normalized:sub(1, 1):rep(2)
            .. normalized:sub(2, 2):rep(2)
            .. normalized:sub(3, 3):rep(2)
    end

    if #normalized ~= 6 or normalized:find("[^%x]") then
        return default_r, default_g, default_b
    end

    return tonumber(normalized:sub(1, 2), 16) or default_r,
        tonumber(normalized:sub(3, 4), 16) or default_g,
        tonumber(normalized:sub(5, 6), 16) or default_b
end

local function make_random_color()
    local r, g, b = host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
    return { r = r or 255, g = g or 0, b = b or 0 }
end

local function lerp_rgb(r1, g1, b1, r2, g2, b2, t)
    t = clamp(t, 0.0, 1.0)
    return round_byte(r1 + (r2 - r1) * t),
        round_byte(g1 + (g2 - g1) * t),
        round_byte(b1 + (b2 - b1) * t)
end

local function scaled(raw_value)
    return raw_value * VALUE_SCALE
end

local function coerce_int(value, min_value, max_value)
    if type(value) ~= "number" then
        return nil
    end
    return clamp(math_floor(value + 0.5), min_value, max_value)
end

local function apply_colors()
    background_r, background_g, background_b = hex_to_rgb(params.background, 0, 0, 0)
    user_r, user_g, user_b = hex_to_rgb(params.color, 255, 0, 0)
end

local function reset_runtime_state()
    progress = 0.0
    random_tick = 0.0
    last_time = nil
    time_carry = 0.0
    current_random = make_random_color()
    next_random = make_random_color()
end

local function step_reference_frame()
    local delta = params.speed * FRAME_DT

    if random_tick >= 1.0 then
        current_random = next_random
        next_random = make_random_color()
        random_tick = 0.0
    end

    random_tick = random_tick + (0.005 * delta)
    progress = progress + (0.1 * delta)
end

function plugin.on_init()
    local seed = os.time() + math_floor((os.clock() % 1) * 1000000)
    math.randomseed(seed)
    math.random()
    math.random()
    math.random()

    apply_colors()
    reset_runtime_state()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    local thickness = coerce_int(p.thickness, 2, 20)
    if thickness ~= nil then
        params.thickness = thickness
    end

    local speed = coerce_int(p.speed, 20, 200)
    if speed ~= nil then
        params.speed = speed
    end

    local amplitude = coerce_int(p.amplitude, 1, 10000)
    if amplitude ~= nil then
        params.amplitude = amplitude
    end

    local frequency = coerce_int(p.frequency, 1, 10000)
    if frequency ~= nil then
        params.frequency = frequency
    end

    local freq_m1 = coerce_int(p.freq_m1, 1, 10000)
    if freq_m1 ~= nil then
        params.freq_m1 = freq_m1
    end

    local freq_m2 = coerce_int(p.freq_m2, 1, 10000)
    if freq_m2 ~= nil then
        params.freq_m2 = freq_m2
    end

    local freq_m3 = coerce_int(p.freq_m3, 1, 10000)
    if freq_m3 ~= nil then
        params.freq_m3 = freq_m3
    end

    local freq_m4 = coerce_int(p.freq_m4, 1, 1000)
    if freq_m4 ~= nil then
        params.freq_m4 = freq_m4
    end

    local freq_m5 = coerce_int(p.freq_m5, 1, 10000)
    if freq_m5 ~= nil then
        params.freq_m5 = freq_m5
    end

    local freq_m6 = coerce_int(p.freq_m6, 1, 10000)
    if freq_m6 ~= nil then
        params.freq_m6 = freq_m6
    end

    local freq_m7 = coerce_int(p.freq_m7, 1, 100)
    if freq_m7 ~= nil then
        params.freq_m7 = freq_m7
    end

    local freq_m8 = coerce_int(p.freq_m8, 1, 10000)
    if freq_m8 ~= nil then
        params.freq_m8 = freq_m8
    end

    local freq_m9 = coerce_int(p.freq_m9, 1, 10000)
    if freq_m9 ~= nil then
        params.freq_m9 = freq_m9
    end

    local freq_m10 = coerce_int(p.freq_m10, 1, 10000)
    if freq_m10 ~= nil then
        params.freq_m10 = freq_m10
    end

    local freq_m11 = coerce_int(p.freq_m11, 1, 10000)
    if freq_m11 ~= nil then
        params.freq_m11 = freq_m11
    end

    local freq_m12 = coerce_int(p.freq_m12, 1, 100)
    if freq_m12 ~= nil then
        params.freq_m12 = freq_m12
    end

    if type(p.random) == "boolean" then
        params.random = p.random
    end

    if type(p.background) == "string" then
        params.background = p.background
    end

    if type(p.color) == "string" then
        params.color = p.color
    end

    apply_colors()
end

function plugin.on_tick(t, buffer, width, height)
    local led_count = buffer:len()
    if led_count <= 0 then
        return
    end

    if type(width) ~= "number" or width <= 0 then
        width = led_count
    end
    if type(height) ~= "number" or height <= 0 then
        height = 1
    end

    local frequency = scaled(params.frequency)
    local amplitude = scaled(params.amplitude)
    local freq_m1 = scaled(params.freq_m1)
    local freq_m2 = scaled(params.freq_m2)
    local freq_m3 = scaled(params.freq_m3)
    local freq_m4 = scaled(params.freq_m4)
    local freq_m5 = scaled(params.freq_m5)
    local freq_m6 = scaled(params.freq_m6)
    local freq_m7 = scaled(params.freq_m7)
    local freq_m8 = scaled(params.freq_m8)
    local freq_m9 = scaled(params.freq_m9)
    local freq_m10 = scaled(params.freq_m10)
    local freq_m11 = scaled(params.freq_m11)
    local freq_m12 = scaled(params.freq_m12)

    local f = frequency * 0.01
    local t_term = 0.01 * (-progress * params.speed)

    local color_r, color_g, color_b
    if params.random then
        color_r, color_g, color_b = lerp_rgb(
            current_random.r,
            current_random.g,
            current_random.b,
            next_random.r,
            next_random.g,
            next_random.b,
            math_min(1.0, random_tick)
        )
    else
        color_r, color_g, color_b = user_r, user_g, user_b
    end

    local led = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if led > led_count then
                return
            end

            local wave = math_sin(x * f)
            wave = wave + (math_sin((x * f * freq_m1) + t_term) * freq_m2)
            wave = wave + (math_sin((x * f * freq_m3) + (t_term * freq_m4)) * freq_m5)
            wave = wave + (math_sin((x * f * freq_m6) + (t_term * freq_m7)) * freq_m8)
            wave = wave + (math_sin((x * f * freq_m9) + (t_term * freq_m10)) * freq_m11)
            wave = wave * (0.1 * amplitude * freq_m12)

            local curve_y = (1.0 + wave) * 0.5 * height
            local distance = math_abs(curve_y - y)

            if distance > params.thickness then
                buffer:set(led, background_r, background_g, background_b)
            else
                local out_r, out_g, out_b = lerp_rgb(
                    color_r,
                    color_g,
                    color_b,
                    background_r,
                    background_g,
                    background_b,
                    distance / params.thickness
                )
                buffer:set(led, out_r, out_g, out_b)
            end

            led = led + 1
        end
    end

    local dt = FRAME_DT
    if last_time ~= nil and type(t) == "number" then
        dt = math_max(0.0, t - last_time)
    end
    if type(t) == "number" then
        last_time = t
    end

    time_carry = time_carry + dt
    while time_carry >= FRAME_DT do
        step_reference_frame()
        time_carry = time_carry - FRAME_DT
    end
end

function plugin.on_shutdown()
    reset_runtime_state()
end

return plugin
