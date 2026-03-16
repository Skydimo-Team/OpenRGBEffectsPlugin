local plugin = {}

local math_floor = math.floor
local math_max = math.max
local math_random = math.random

local speed = 50
local visor_width = 20
local random_enabled = false
local user_r, user_g, user_b = 255, 0, 0

local progress = 0.0
local p = 0.0
local p_step = 0.0
local step = false
local last_step = false
local flash_length = 1.0
local last_elapsed = 0.0
local seeded = false

local function clamp01(value)
    if value < 0.0 then
        return 0.0
    end
    if value > 1.0 then
        return 1.0
    end
    return value
end

local function hex_to_rgb(value)
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

local function scale_channel(channel, factor)
    return math_floor(channel * factor + 0.5)
end

local function scale_color(r, g, b, factor)
    if factor <= 0.0 then
        return 0, 0, 0
    end

    if factor >= 1.0 then
        return r, g, b
    end

    return scale_channel(r, factor),
        scale_channel(g, factor),
        scale_channel(b, factor)
end

local function lerp_color(r0, g0, b0, r1, g1, b1, t)
    local inv = 1.0 - t
    return math_floor(r0 * inv + r1 * t + 0.5),
        math_floor(g0 * inv + g1 * t + 0.5),
        math_floor(b0 * inv + b1 * t + 0.5)
end

local function pick_random_color()
    return host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
end

local function active_color()
    if random_enabled then
        return pick_random_color()
    end

    return user_r, user_g, user_b
end

local function sample_color(index, count, width_fraction, color_r, color_g, color_b)
    local raw_count = count
    if raw_count <= 0 then
        raw_count = 1
    end

    local w = math_max(1.5 / raw_count, width_fraction)
    local sample_count = raw_count
    if sample_count <= 1 then
        sample_count = 2
    end
    local x_step = p_step * (1.0 + 4.0 * w) - 1.5 * w
    local x = index / (sample_count - 1)
    local dist = x_step - x

    if dist < 0.0 then
        local l = clamp01((w + dist) / w)
        if step then
            return 0, 0, 0
        end
        return scale_color(color_r, color_g, color_b, l)
    end

    if dist > w then
        local l = clamp01(1.0 - ((dist - w) / w))
        if step then
            return scale_color(color_r, color_g, color_b, l)
        end
        return 0, 0, 0
    end

    local interp = clamp01((w - dist) / w)
    if step then
        return lerp_color(color_r, color_g, color_b, 0, 0, 0, interp)
    end

    return lerp_color(0, 0, 0, color_r, color_g, color_b, interp)
end

function plugin.on_init()
    if not seeded then
        math.randomseed(math_floor((os.time() % 1000000) + os.clock() * 1000000))
        seeded = true
    end

    progress = 0.0
    p = 0.0
    p_step = 0.0
    step = false
    last_step = false
    flash_length = 1.0
    last_elapsed = 0.0
end

function plugin.on_params(params)
    if type(params) ~= "table" then
        return
    end

    if type(params.speed) == "number" then
        speed = params.speed
    end

    if type(params.width) == "number" then
        visor_width = params.width
    end

    if type(params.random) == "boolean" then
        random_enabled = params.random
    end

    if type(params.color) == "string" then
        local r, g, b = hex_to_rgb(params.color)
        if r ~= nil then
            user_r, user_g, user_b = r, g, b
        end
    end
end

function plugin.on_tick(elapsed, buffer, width, height)
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

    local current_elapsed = 0.0
    if type(elapsed) == "number" and elapsed > 0.0 then
        current_elapsed = elapsed
    end

    local delta = current_elapsed - last_elapsed
    if delta < 0.0 then
        delta = 0.0
    end
    last_elapsed = current_elapsed

    progress = progress + (0.01 * speed * delta)

    if flash_length < 0.0 then
        flash_length = 1.0
        last_step = step
    end

    local width_fraction = 0.01 * visor_width
    p = progress - math_floor(progress)
    step = p < 0.5
    p_step = step and (2.0 * p) or (2.0 * (1.0 - p))

    local color_r, color_g, color_b = active_color()
    local flipping = last_step ~= step

    if flipping then
        for i = 1, n do
            buffer:set(i, color_r, color_g, color_b)
        end
        flash_length = flash_length - (0.03 * speed * delta)
        return
    end

    local led = 1
    for _ = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                return
            end

            local r, g, b = sample_color(x, width, width_fraction, color_r, color_g, color_b)
            buffer:set(led, r, g, b)
            led = led + 1
        end
    end
end

function plugin.on_shutdown()
    last_elapsed = 0.0
end

return plugin
