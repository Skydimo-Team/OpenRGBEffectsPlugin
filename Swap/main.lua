local plugin = {}

local math_floor = math.floor
local math_random = math.random
local math_randomseed = math.randomseed
local os_clock = os.clock
local os_time = os.time

-- Parameters (matching C++ defaults: Speed 1-20, default 10)
local speed = 10
local random_enabled = false

local user_colors = {
    { r = 255, g = 0, b = 0 },
    { r = 0, g = 0, b = 255 },
}

-- Random color pool (refreshed on direction transitions)
local random1 = { r = 0, g = 0, b = 0 }
local random2 = { r = 0, g = 0, b = 0 }

-- Persistent render state (mirrors C++ member variables)
local time_acc = 0.0 -- accumulated time (C++ `time`)
local progress = 0.0 -- fractional part of time_acc
local dir = false -- current sweep direction (false=0, true=1)
local old_dir = false -- previous direction for transition detection
local c1 = { r = 255, g = 0, b = 0 } -- current color 1
local c2 = { r = 0, g = 0, b = 255 } -- current color 2

local last_t = nil

local function random_rgb_color()
    local r, g, b = host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
    return { r = r, g = g, b = b }
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

function plugin.on_init()
    math_randomseed(os_time(), math_floor((os_clock() % 1) * 1000000))
    random1 = random_rgb_color()
    random2 = random_rgb_color()
    time_acc = 0.0
    progress = 0.0
    dir = false
    old_dir = false
    c1 = { r = user_colors[1].r, g = user_colors[1].g, b = user_colors[1].b }
    c2 = { r = user_colors[2].r, g = user_colors[2].g, b = user_colors[2].b }
    last_t = nil
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = p.speed
        if speed < 1 then speed = 1 end
        if speed > 20 then speed = 20 end
    end

    if type(p.colors) == "table" then
        update_user_colors(p.colors)
    end

    if type(p.random) == "boolean" then
        local was_random = random_enabled
        random_enabled = p.random
        if random_enabled and not was_random then
            random1 = random_rgb_color()
            random2 = random_rgb_color()
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

    -- ── 1. Render using current state (matches C++ StepEffect render phase) ──

    -- Sweep threshold: dir=true → progress sweeps left-to-right;
    --                  dir=false → (1 - progress) sweeps right-to-left
    local x = dir and progress or (1.0 - progress)
    local axis_len = width
    local color1_r, color1_g, color1_b = c1.r, c1.g, c1.b
    local color2_r, color2_g, color2_b = c2.r, c2.g, c2.b
    local threshold = x * (axis_len + 1)

    local led = 1
    for _ = 0, height - 1 do
        for col = 0, axis_len - 1 do
            if led > n then
                goto done_render
            end

            -- C++ GetColor: (i+1) <= x * (w+1) ? c1 : c2
            if (col + 1) <= threshold then
                buffer:set(led, color1_r, color1_g, color1_b)
            else
                buffer:set(led, color2_r, color2_g, color2_b)
            end

            led = led + 1
        end
    end
    ::done_render::

    -- ── 2. Advance time accumulator (matches C++ `time += 0.1 * Speed / FPS`) ──

    local delta = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            delta = t
        else
            delta = t - last_t
        end
        last_t = t
    end

    -- C++: time += 0.1 * Speed / FPS  →  per-frame increment
    -- Lua: delta ≈ 1/FPS, so  0.1 * speed * delta  is equivalent
    time_acc = time_acc + 0.1 * speed * delta

    -- ── 3. Update progress & direction for next frame ──

    local whole = math_floor(time_acc)
    progress = time_acc - whole
    dir = (whole % 2) == 1

    -- ── 4. Update c1, c2 for next frame ──

    if random_enabled then
        c1 = random1
        c2 = random2
    else
        c1 = user_colors[1]
        c2 = user_colors[2]
    end

    -- ── 5. Direction change → refresh random color for next frame ──

    if not old_dir and dir then
        -- 0 → 1 transition: refresh random1 (used as c1)
        random1 = random_rgb_color()
    elseif old_dir and not dir then
        -- 1 → 0 transition: refresh random2 (used as c2)
        random2 = random_rgb_color()
    end
    old_dir = dir
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
