local plugin = {}

local FPS = 60
local STEP_EPSILON = 1e-6

local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_random = math.random
local math_modf = math.modf
local table_remove = table.remove

local speed = 25
local max_drops_setting = 20
local drop_size = 1
local random_enabled = false
local only_first_enabled = false

local default_palette = {
    { r = 255, g = 0,   b = 0   },
    { r = 255, g = 153, b = 0   },
    { r = 255, g = 255, b = 0   },
    { r = 0,   g = 255, b = 136 },
    { r = 0,   g = 170, b = 255 },
}

local palette = {}
local drops = {}
local simulated_steps = 0
local last_time = nil
local last_width = 0
local last_height = 0
local seeded = false

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
        r = tonumber(hex:sub(1, 2), 16) or 255,
        g = tonumber(hex:sub(3, 4), 16) or 255,
        b = tonumber(hex:sub(5, 6), 16) or 255,
    }
end

local function reset_palette()
    palette = {}
    for i = 1, #default_palette do
        local color = default_palette[i]
        palette[i] = { r = color.r, g = color.g, b = color.b }
    end
end

local function reset_state(current_steps)
    drops = {}
    simulated_steps = current_steps or 0
end

local function truncate_scale(value, factor)
    return math_floor(value * factor)
end

local function scale_color(r, g, b, factor)
    if factor <= 0.0 then
        return 0, 0, 0
    end

    if factor >= 1.0 then
        return r, g, b
    end

    return truncate_scale(r, factor),
        truncate_scale(g, factor),
        truncate_scale(b, factor)
end

local function pick_random_color()
    local hue = math_random(0, 359)
    local r, g, b = host.hsv_to_rgb(hue, 1.0, 1.0)
    return { r = r, g = g, b = b }
end

local function pick_drop_color()
    if only_first_enabled then
        local color = palette[1]
        return {
            r = color and color.r or 255,
            g = color and color.g or 0,
            b = color and color.b or 0,
        }
    end

    if random_enabled then
        return pick_random_color()
    end

    local count = #palette
    if count <= 0 then
        return { r = 255, g = 0, b = 0 }
    end

    local color = palette[math_random(1, count)]
    return { r = color.r, g = color.g, b = color.b }
end

local function trigger_drop(width)
    if width <= 0 then
        return
    end

    local max_drops = math_min(width, max_drops_setting)
    if #drops >= max_drops then
        return
    end

    local spawn_divisor = 2 + math_floor(FPS / width)
    if math_random(0, spawn_divisor - 1) ~= 0 then
        return
    end

    local color = pick_drop_color()
    drops[#drops + 1] = {
        progress = 0.0,
        r = color.r,
        g = color.g,
        b = color.b,
        col = math_random(0, width - 1),
        speed_mult = math_random(1, 3) + math_random(),
        size = drop_size,
    }
end

local function run_drops()
    for i = 1, #drops do
        local drop = drops[i]
        drop.progress = drop.progress + (0.5 * drop.speed_mult * speed / FPS)
    end
end

local function clean_drops(height)
    local i = 1
    while i <= #drops do
        local drop = drops[i]
        if drop.progress > height + (3 * drop.size) then
            table_remove(drops, i)
        else
            i = i + 1
        end
    end
end

local function step_once(width, height)
    trigger_drop(width)
    run_drops()
    clean_drops(height)
end

local function sync_state(time_now, width, height)
    local target_steps = math_floor((time_now * FPS) + STEP_EPSILON)
    if target_steps < simulated_steps then
        reset_state(target_steps)
        return
    end

    while simulated_steps < target_steps do
        step_once(width, height)
        simulated_steps = simulated_steps + 1
    end
end

local function get_color(x, y)
    for i = 1, #drops do
        local drop = drops[i]

        if drop.col >= x and drop.col <= (x + drop.size - 1) then
            local distance = drop.progress - y
            local trail_length = math_floor((drop.speed_mult - 1.0) * ((drop.size / 2.0) + 1.0))

            if distance >= 0.0 and distance <= (drop.size + 1 + trail_length) then
                local whole, frac = math_modf(distance)

                if whole == 0 then
                    return scale_color(drop.r, drop.g, drop.b, frac)
                end

                if whole > 0 and whole <= drop.size then
                    return drop.r, drop.g, drop.b
                end

                return scale_color(drop.r, drop.g, drop.b, 0.75 / (whole - drop.size))
            end
        end
    end

    return 0, 0, 0
end

function plugin.on_init()
    if not seeded then
        math.randomseed(math_floor(os.clock() * 1000000) + os.time())
        seeded = true
    end

    reset_palette()
    reset_state(0)
    last_time = nil
    last_width = 0
    last_height = 0
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = math_min(200, math_max(1, math_floor(p.speed + 0.5)))
    end

    if type(p.drops) == "number" then
        max_drops_setting = math_min(50, math_max(1, math_floor(p.drops + 0.5)))
    end

    if type(p.drop_size) == "number" then
        drop_size = math_min(10, math_max(1, math_floor(p.drop_size + 0.5)))
    end

    if type(p.random) == "boolean" then
        random_enabled = p.random
    end

    if type(p.only_first) == "boolean" then
        only_first_enabled = p.only_first
    end

    if type(p.colors) == "table" then
        local new_palette = {}
        for i = 1, #p.colors do
            local parsed = parse_hex_color(p.colors[i])
            if parsed then
                new_palette[#new_palette + 1] = parsed
            end
        end

        if #new_palette > 0 then
            palette = new_palette
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

    if type(t) ~= "number" or t < 0 then
        t = 0
    end

    local current_steps = math_floor((t * FPS) + STEP_EPSILON)
    if last_time ~= nil and (t + STEP_EPSILON) < last_time then
        reset_state(current_steps)
    end

    if width ~= last_width or height ~= last_height then
        reset_state(current_steps)
        last_width = width
        last_height = height
    end

    sync_state(t, width, height)

    local led = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                last_time = t
                return
            end

            local r, g, b = get_color(x, y)
            buffer:set(led, r, g, b)
            led = led + 1
        end
    end

    last_time = t
end

function plugin.on_shutdown()
    drops = {}
    last_time = nil
end

return plugin
