local plugin = {}

local math_ceil = math.ceil
local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_random = math.random
local math_randomseed = math.randomseed
local os_clock = os.clock
local os_time = os.time

local speed = 10
local random_enabled = false

local user_colors = {
    { r = 255, g = 0, b = 0 },
    { r = 0, g = 0, b = 255 },
}

local random_colors = {
    { r = 255, g = 0, b = 0 },
    { r = 0, g = 0, b = 255 },
}

-- Match the reference effect's mutable per-zone progress so speed changes stay continuous.
local progress = 0.0
local last_t = nil

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

    local first = parse_hex_color(colors[1])
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

local function refresh_random_colors()
    random_colors[1] = random_rgb_color()
    random_colors[2] = random_rgb_color()
end

local function trunc_toward_zero(value)
    if value < 0 then
        return math_ceil(value)
    end
    return math_floor(value)
end

local function lerp_channel(start_value, finish_value, blend)
    return trunc_toward_zero(start_value + blend * (finish_value - start_value))
end

function plugin.on_init()
    math_randomseed(os_time(), math_floor((os_clock() % 1) * 1000000))
    refresh_random_colors()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = math_max(1, math_min(30, p.speed))
    end

    if type(p.colors) == "table" then
        update_user_colors(p.colors)
    end

    if type(p.random) == "boolean" then
        local was_random = random_enabled
        random_enabled = p.random
        if random_enabled and not was_random then
            refresh_random_colors()
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

    local axis_len = math_max(1, width)
    local cycle_limit = axis_len * 2.0
    local current_progress = progress
    local colors = random_enabled and random_colors or user_colors
    local start_color = colors[1]
    local finish_color = colors[2]

    local led = 1
    for _ = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                return
            end

            local pos_index = x
            local gradient_pos
            if (current_progress + pos_index) > axis_len then
                gradient_pos = axis_len - ((current_progress + pos_index) - axis_len)
            else
                gradient_pos = current_progress + pos_index
            end

            if gradient_pos <= 0 then
                gradient_pos = -gradient_pos
            end

            local blend = gradient_pos / axis_len
            local r = lerp_channel(start_color.r, finish_color.r, blend)
            local g = lerp_channel(start_color.g, finish_color.g, blend)
            local b = lerp_channel(start_color.b, finish_color.b, blend)

            buffer:set(led, r, g, b)
            led = led + 1
        end
    end

    local delta = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            delta = t
        else
            delta = t - last_t
        end
        last_t = t
    end

    local increment = 0.1 * axis_len * speed * delta
    -- The C++ effect advances progress after rendering, so keep the same order here.
    if current_progress < cycle_limit then
        progress = current_progress + increment
    else
        progress = 0.0
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
