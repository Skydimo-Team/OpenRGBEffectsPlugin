local plugin = {}

local math_floor = math.floor
local math_max = math.max
local math_min = math.min

local speed = 50
local spacing = 2
local random_enabled = false
local user_r, user_g, user_b = 255, 0, 0

-- Mirror the reference effect's mutable state:
-- render with the current values first, then advance progress and hue.
local progress = 0.0
local last_t = nil
local random_hue = 0.0

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

local function active_color()
    if random_enabled then
        return host.hsv_to_rgb(random_hue, 1.0, 1.0)
    end

    return user_r, user_g, user_b
end

local function update_progress(t)
    local delta = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            delta = t
        else
            delta = t - last_t
        end
        last_t = t
    end

    -- Reference per-frame update:
    --   progress += 0.1 * Speed / FPS
    -- which is equivalent to d(progress)/dt = 0.1 * Speed.
    progress = progress + (0.1 * speed * delta)
end

function plugin.on_init()
    progress = 0.0
    last_t = nil
    random_hue = 0.0
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = math_max(1, math_min(200, p.speed))
    end

    if type(p.spacing) == "number" then
        spacing = math_max(2, math_min(20, math_floor(p.spacing)))
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
    local shift = math_floor(progress)
    local led = 1

    -- The reference effect samples only X and repeats the same marquee on each row.
    for _ = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                break
            end

            if ((x + shift) % spacing) == 0 then
                buffer:set(led, lit_r, lit_g, lit_b)
            else
                buffer:set(led, 0, 0, 0)
            end

            led = led + 1
        end

        if led > n then
            break
        end
    end

    update_progress(t)

    if random_enabled then
        random_hue = (random_hue + 1.0) % 360.0
    end
end

function plugin.on_shutdown()
    last_t = nil
end

return plugin
