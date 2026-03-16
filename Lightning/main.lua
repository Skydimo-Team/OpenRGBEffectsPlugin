local plugin = {}

local FPS = 60
local MODE_WHOLE_STRIP = 0
local MODE_PER_LED = 1

local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_random = math.random

local speed = 20
local decay = 10
local mode = MODE_WHOLE_STRIP
local random_enabled = false

local user_hue = 0
local user_saturation = 255
local user_value = 255

local lightnings = {}
local lightning_count = 0
local last_tick_time = nil
local frame_remainder = 0.0
local first_tick = true

local function clamp(value, lo, hi)
    if value < lo then
        return lo
    end
    if value > hi then
        return hi
    end
    return value
end

local function parse_hex_color(value)
    if type(value) ~= "string" then
        return 255, 0, 0
    end

    local hex = value:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then
        hex = hex:sub(2)
    end

    if #hex == 3 then
        hex = hex:sub(1, 1):rep(2) .. hex:sub(2, 2):rep(2) .. hex:sub(3, 3):rep(2)
    end

    if #hex ~= 6 or hex:find("[^%x]") then
        return 255, 0, 0
    end

    return tonumber(hex:sub(1, 2), 16) or 255,
        tonumber(hex:sub(3, 4), 16) or 0,
        tonumber(hex:sub(5, 6), 16) or 0
end

local function rgb_to_hsv_255(r, g, b)
    local rf = r / 255.0
    local gf = g / 255.0
    local bf = b / 255.0

    local maxc = math_max(rf, gf, bf)
    local minc = math_min(rf, gf, bf)
    local delta = maxc - minc

    local hue = 0.0
    if delta > 0.0 then
        if maxc == rf then
            hue = 60.0 * (((gf - bf) / delta) % 6.0)
        elseif maxc == gf then
            hue = 60.0 * (((bf - rf) / delta) + 2.0)
        else
            hue = 60.0 * (((rf - gf) / delta) + 4.0)
        end
    end

    if hue < 0.0 then
        hue = hue + 360.0
    end

    local saturation = 0.0
    if maxc > 0.0 then
        saturation = (delta / maxc) * 255.0
    end

    local value = maxc * 255.0

    return math_floor(hue + 0.5) % 360,
        clamp(math_floor(saturation + 0.5), 0, 255),
        clamp(math_floor(value + 0.5), 0, 255)
end

local function set_user_color(hex)
    local r, g, b = parse_hex_color(hex)
    user_hue, user_saturation, user_value = rgb_to_hsv_255(r, g, b)
end

local function make_lightning_state()
    return {
        hue = user_hue,
        saturation = user_saturation,
        value = 0,
    }
end

local function sync_lightning_count(count)
    if lightning_count > count then
        for i = count + 1, lightning_count do
            lightnings[i] = nil
        end
    elseif lightning_count < count then
        for i = lightning_count + 1, count do
            lightnings[i] = make_lightning_state()
        end
    end

    lightning_count = count
end

local function advance_lightning(state, trigger_mod)
    local decrease = 1.0 + (decay / FPS)
    local triggered = math_random(trigger_mod) <= speed

    if triggered then
        state.value = random_enabled and 255 or user_value
    elseif state.value > 0 then
        state.value = math_floor(state.value / decrease)
    else
        state.value = 0
    end

    if random_enabled then
        if state.value == 0 then
            state.hue = math_random(0, 359)
            state.saturation = math_random(0, 254)
        end
    else
        state.hue = user_hue
        state.saturation = user_saturation
    end
end

local function step_simulation(led_count, elapsed)
    local steps = 0

    if first_tick then
        first_tick = false
        last_tick_time = elapsed
        frame_remainder = 0.0
        steps = 1
    else
        local dt = elapsed - (last_tick_time or elapsed)
        if dt < 0.0 then
            dt = 0.0
        end

        last_tick_time = elapsed

        local frames = dt * FPS + frame_remainder
        steps = math_floor(frames)
        frame_remainder = frames - steps
    end

    if steps <= 0 or led_count <= 0 then
        return
    end

    local trigger_mod = (mode == MODE_WHOLE_STRIP) and 1000 or (1000 * led_count)

    for _ = 1, steps do
        if mode == MODE_WHOLE_STRIP then
            advance_lightning(lightnings[1], trigger_mod)
        else
            for i = 1, led_count do
                advance_lightning(lightnings[i], trigger_mod)
            end
        end
    end
end

local function render_state(buffer, state, led_index)
    buffer:set_hsv(
        led_index,
        state.hue,
        state.saturation / 255.0,
        state.value / 255.0
    )
end

function plugin.on_init()
    math.randomseed(os.time())
    set_user_color("#FF0000")
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = clamp(math_floor(p.speed + 0.5), 1, 100)
    end

    if type(p.decay) == "number" then
        decay = clamp(math_floor(p.decay + 0.5), 2, 60)
    end

    if type(p.mode) == "number" then
        local next_mode = math_floor(p.mode + 0.5)
        if next_mode == MODE_WHOLE_STRIP or next_mode == MODE_PER_LED then
            mode = next_mode
        end
    end

    if type(p.random) == "boolean" then
        random_enabled = p.random
    end

    if type(p.color) == "string" then
        set_user_color(p.color)
    end
end

function plugin.on_tick(elapsed, buffer, width, height)
    local led_count = buffer:len()
    if led_count <= 0 then
        return
    end

    sync_lightning_count(led_count)
    step_simulation(led_count, elapsed)

    if mode == MODE_WHOLE_STRIP then
        local zone_state = lightnings[1]
        for i = 1, led_count do
            render_state(buffer, zone_state, i)
        end
    else
        for i = 1, led_count do
            render_state(buffer, lightnings[i], i)
        end
    end
end

function plugin.on_shutdown()
    lightnings = {}
    lightning_count = 0
    last_tick_time = nil
    frame_remainder = 0.0
    first_tick = true
end

return plugin
