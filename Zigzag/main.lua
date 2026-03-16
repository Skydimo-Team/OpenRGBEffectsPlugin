local plugin = {}

local math_floor = math.floor
local math_ceil  = math.ceil
local math_max   = math.max
local math_min   = math.min

-- Parameters matching the original C++ ZigZag effect
-- Original: Speed 1-20 default 10, UserColors[0], RandomColorsEnabled
local speed      = 10
local color_mode = 0   -- 0 = rainbow (RandomColorsEnabled), 1 = custom (UserColors[0])
local user_color = { r = 255, g = 0, b = 0 }

-- Internal state matching the original C++:
--   double time = 0.;
--   double progress = 0.;
local time_acc = 0.0
local progress = 0.0
local last_t   = nil

local function parse_hex_color(value)
    if type(value) ~= "string" then
        return nil
    end
    local hex = value:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then
        hex = hex:sub(2)
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

-- Convert RGB (0-255) to HSV (h: 0-360, s: 0-1, v: 0-1)
local function rgb_to_hsv(r, g, b)
    local r_n, g_n, b_n = r / 255.0, g / 255.0, b / 255.0
    local max = math_max(r_n, g_n, b_n)
    local min = math_min(r_n, g_n, b_n)
    local delta = max - min

    local h, s, v
    v = max

    if max == 0 then
        s = 0
        h = 0
    else
        s = delta / max
        if delta == 0 then
            h = 0
        elseif max == r_n then
            h = 60 * (((g_n - b_n) / delta) % 6)
        elseif max == g_n then
            h = 60 * (((b_n - r_n) / delta) + 2)
        else
            h = 60 * (((r_n - g_n) / delta) + 4)
        end
    end

    if h < 0 then h = h + 360 end

    return h, s, v
end

-- Truncate toward zero, matching C++ int() cast
local function trunc(x)
    if x >= 0 then
        return math_floor(x)
    else
        return math_ceil(x)
    end
end

-- Compute color for LED at grid position (x, y) in a grid of size (w, h).
-- Reproduces the original C++ GetColor(float x, float y, float w, float h):
--   1. ZigZag ordering: even columns top→bottom, odd columns bottom→top
--   2. `current_led_percent < progress` determines if the LED is lit
--   3. Brightness = pow(percent / progress, 3) — cubic falloff from head to tail
--   4. Rainbow mode: rotating hue; Custom mode: user color with value scaling
local function get_color(x, y, w, h)
    local total_leds = w * h

    -- Zigzag snake ordering: even columns go down, odd columns go up
    -- Original: ((int)x % 2 == 0 ? y : h - y - 1) + x * h
    local current_led_position
    if x % 2 == 0 then
        current_led_position = y + x * h
    else
        current_led_position = (h - y - 1) + x * h
    end

    local current_led_percent = current_led_position / total_leds

    if current_led_percent < progress then
        -- Cubic brightness falloff: 1.0 at head (percent == progress), 0.0 at tail
        -- Original: float distance = pow(current_led_percent / progress, 3);
        local distance = (current_led_percent / progress) ^ 3

        if color_mode == 0 then
            -- Rainbow mode (RandomColorsEnabled = true)
            -- Original: hsv.hue = int(distance*360. -100.*time) % 360;
            --           hsv.saturation = 255; hsv.value = 255;
            --           hsv.value *= distance;
            local hue = trunc(distance * 360.0 - 100.0 * time_acc) % 360
            -- s = 1.0 (full saturation), v = distance (brightness falloff)
            return host.hsv_to_rgb(hue, 1.0, distance)
        else
            -- Custom color mode
            -- Original: rgb2hsv(UserColors[0], &hsv); hsv.value *= distance;
            local uh, us, uv = rgb_to_hsv(user_color.r, user_color.g, user_color.b)
            return host.hsv_to_rgb(uh, us, uv * distance)
        end
    else
        return 0, 0, 0
    end
end

function plugin.on_init()
    -- no-op
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end
    if type(p.speed) == "number" then
        speed = math_max(1, math_min(20, p.speed))
    end
    if type(p.color_mode) == "number" then
        local m = math_floor(p.color_mode + 0.5)
        if m == 0 or m == 1 then
            color_mode = m
        end
    end
    if p.color ~= nil then
        local parsed = parse_hex_color(p.color)
        if parsed then
            user_color = parsed
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

    -- Render all LEDs using current progress (before advancing time)
    local idx = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if idx > n then
                goto done_render
            end
            local r, g, b = get_color(x, y, width, height)
            buffer:set(idx, r, g, b)
            idx = idx + 1
        end
    end
    ::done_render::

    -- Compute delta time
    local dt = 0
    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            dt = t
        else
            dt = t - last_t
        end
        last_t = t
    end

    -- Advance time matching the original:
    --   time += 0.01 * (float) Speed / (float) FPS;
    -- Using delta time for frame-rate independence (dt replaces 1/FPS)
    time_acc = time_acc + 0.01 * speed * dt

    -- Progress sweeps 0 → 2 then resets
    -- Original: progress = 2*(time-(long)time);
    progress = 2 * (time_acc - math_floor(time_acc))
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
