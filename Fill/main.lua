local plugin = {}

local math_floor = math.floor
local math_random = math.random

-- Parameters (matching reference defaults/ranges)
local speed = 50
local random_enabled = false
local user_r, user_g, user_b = 255, 0, 0

-- Internal state mirrors the C++ effect's evolving `time`.
local seeded = false
local last_elapsed = nil
local time_acc = 0.0
local observed_cycle = 0
local rand_r, rand_g, rand_b = 255, 0, 0

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
    -- Matches the reference RandomRGBColor usage seen in other ports:
    -- random hue, full saturation, full value.
    rand_r, rand_g, rand_b = host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
end

local function scale_channel(channel, factor)
    if factor <= 0.0 then
        return 0
    end
    if factor >= 1.0 then
        return channel
    end
    return math_floor(channel * factor)
end

local function set_scaled_color(buffer, idx, r, g, b, factor)
    if factor <= 0.0 then
        buffer:set(idx, 0, 0, 0)
        return
    end

    if factor >= 1.0 then
        buffer:set(idx, r, g, b)
        return
    end

    buffer:set(
        idx,
        scale_channel(r, factor),
        scale_channel(g, factor),
        scale_channel(b, factor)
    )
end

local function active_color()
    if random_enabled then
        return rand_r, rand_g, rand_b
    end
    return user_r, user_g, user_b
end

local function advance_time(elapsed)
    local delta = 0.0
    if type(last_elapsed) == "number" and elapsed >= last_elapsed then
        delta = elapsed - last_elapsed
    end
    last_elapsed = elapsed

    if delta <= 0.0 then
        return
    end

    -- Reference per-frame update:
    --   time += 0.01 * Speed / FPS
    -- which is equivalent to d(time)/dt = 0.01 * Speed.
    time_acc = time_acc + (0.01 * speed * delta)
end

local function sync_random_cycle(cycle)
    if cycle == observed_cycle then
        return
    end

    if random_enabled and cycle > observed_cycle then
        for _ = observed_cycle + 1, cycle do
            pick_random_color()
        end
    elseif random_enabled and cycle < observed_cycle then
        -- Defensive fallback for unexpected time rewinds.
        pick_random_color()
    end

    observed_cycle = cycle
end

function plugin.on_init()
    if not seeded then
        math.randomseed(math_floor(os.clock() * 1000000))
        seeded = true
    end

    last_elapsed = nil
    time_acc = 0.0
    observed_cycle = 0
    pick_random_color()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = p.speed
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

    advance_time(t)

    local cycle = math_floor(time_acc)
    local progress = time_acc - cycle
    sync_random_cycle(cycle)

    local cr, cg, cb = active_color()
    local is_fade_phase = (cycle % 2) == 1
    local position = progress * width

    -- The reference Fill effect always samples along X only.
    -- Layout flips/reversal are already handled by the core's physical mapping layer.
    local led = 1
    for _ = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                return
            end

            if is_fade_phase then
                set_scaled_color(buffer, led, cr, cg, cb, 1.0 - progress)
            else
                local distance = position - x
                if distance > 1.0 then
                    buffer:set(led, cr, cg, cb)
                elseif distance > 0.0 then
                    set_scaled_color(buffer, led, cr, cg, cb, distance)
                else
                    buffer:set(led, 0, 0, 0)
                end
            end

            led = led + 1
        end
    end
end

function plugin.on_shutdown()
    last_elapsed = nil
end

return plugin
