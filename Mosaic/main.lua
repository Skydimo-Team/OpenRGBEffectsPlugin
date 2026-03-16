local plugin = {}

local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_random = math.random
local math_randomseed = math.randomseed
local os_clock = os.clock
local os_time = os.time

local REFERENCE_FPS = 60.0

local speed = 10
local rarity = 10
local random_enabled = false

local user_colors = {
    { h = 0.0, s = 1.0 },
}

local tiles = {}
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

    return tonumber(hex:sub(1, 2), 16) or 0,
        tonumber(hex:sub(3, 4), 16) or 0,
        tonumber(hex:sub(5, 6), 16) or 0
end

local function rgb_to_hs(r, g, b)
    local rn = (r or 0) / 255.0
    local gn = (g or 0) / 255.0
    local bn = (b or 0) / 255.0

    local max_channel = math_max(rn, math_max(gn, bn))
    local min_channel = math_min(rn, math_min(gn, bn))
    local delta = max_channel - min_channel

    local h = 0.0
    if delta > 0.0 then
        if max_channel == rn then
            h = 60.0 * (((gn - bn) / delta) % 6.0)
        elseif max_channel == gn then
            h = 60.0 * (((bn - rn) / delta) + 2.0)
        else
            h = 60.0 * (((rn - gn) / delta) + 4.0)
        end
    end

    local s = 0.0
    if max_channel > 0.0 then
        s = delta / max_channel
    end

    return h, s
end

local function update_user_colors(raw_colors)
    if type(raw_colors) ~= "table" or #raw_colors < 1 then
        return
    end

    local resolved = {}
    for i = 1, #raw_colors do
        local r, g, b = parse_hex_color(raw_colors[i])
        if r ~= nil then
            local h, s = rgb_to_hs(r, g, b)
            resolved[#resolved + 1] = { h = h, s = s }
        end
    end

    if #resolved > 0 then
        user_colors = resolved
    end
end

local function ensure_tile_count(count)
    local current = #tiles
    if current < count then
        for i = current + 1, count do
            tiles[i] = {
                h = 0.0,
                s = 1.0,
                brightness = 0.0,
                decrease_speed_mult = 1.0,
            }
        end
        return
    end

    if current > count then
        for i = current, count + 1, -1 do
            tiles[i] = nil
        end
    end
end

local function pick_spawn_color()
    if random_enabled then
        return math_random() * 360.0, 1.0
    end

    local color = user_colors[math_random(#user_colors)] or user_colors[1]
    if not color then
        return 0.0, 1.0
    end

    return color.h, color.s
end

local function update_tiles(delta_frames)
    local decay_step = 0.0005 * speed * delta_frames

    for i = 1, #tiles do
        local tile = tiles[i]

        if tile.brightness <= 0.0 then
            if math_random(rarity) == 1 then
                tile.brightness = 1.0
                tile.decrease_speed_mult = 1.0 + math_random()
                tile.h, tile.s = pick_spawn_color()
            end
        end

        tile.brightness = tile.brightness - (decay_step * tile.decrease_speed_mult)
    end
end

function plugin.on_init()
    math_randomseed(os_time(), math_floor((os_clock() % 1) * 1000000))
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = math_max(1, math_min(200, math_floor(p.speed + 0.5)))
    end

    if type(p.rarity) == "number" then
        rarity = math_max(10, math_min(2000, math_floor(p.rarity + 0.5)))
    end

    if type(p.random) == "boolean" then
        random_enabled = p.random
    end

    if type(p.colors) == "table" then
        update_user_colors(p.colors)
    end
end

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then
        return
    end

    ensure_tile_count(n)

    local delta_frames = 1.0
    if type(t) == "number" and t >= 0.0 then
        if last_t ~= nil and t >= last_t then
            delta_frames = math_max(0.0, (t - last_t) * REFERENCE_FPS)
        end
        last_t = t
    else
        last_t = nil
    end

    update_tiles(delta_frames)

    for i = 1, n do
        local tile = tiles[i]
        local value = tile.brightness
        if value > 0.0 then
            buffer:set_hsv(i, tile.h, tile.s, value)
        else
            buffer:set(i, 0, 0, 0)
        end
    end
end

function plugin.on_shutdown()
end

return plugin
