local plugin = {}

local math_sqrt = math.sqrt
local math_sin  = math.sin
local math_cos  = math.cos
local math_floor = math.floor
local math_max  = math.max
local math_min  = math.min

---------------------------------------------------------------------------
-- Parameters (matching reference defaults)
---------------------------------------------------------------------------
local speed   = 50      -- 1-100, maps to rotation angular velocity
local glow    = 50      -- 1-100, controls falloff exponent (Slider2Val)
local radius  = 0       -- 0-100, solid-core radius around each circle center
local reverse = false   -- reverses rotation direction
local random_enabled = true

-- HSV state: h 0-359 integer, s 0-255 integer, v 0-255 integer
local hsv1 = { h = 0,   s = 255, v = 255 }
local hsv2 = { h = 180, s = 255, v = 255 }

---------------------------------------------------------------------------
-- RGB → HSV (integer: h 0-359, s 0-255, v 0-255)
---------------------------------------------------------------------------
local function rgb_to_hsv_int(r, g, b)
    local max_c = math_max(r, g, b)
    local min_c = math_min(r, g, b)
    local delta = max_c - min_c

    local h, s, v
    v = max_c

    if max_c == 0 then
        return 0, 0, 0
    end

    s = math_floor(delta * 255 / max_c + 0.5)

    if delta == 0 then
        h = 0
    elseif max_c == r then
        h = 60 * ((g - b) / delta)
    elseif max_c == g then
        h = 60 * ((b - r) / delta + 2)
    else
        h = 60 * ((r - g) / delta + 4)
    end

    if h < 0 then h = h + 360 end
    h = math_floor(h + 0.5) % 360

    return h, s, v
end

---------------------------------------------------------------------------
-- Parse "#RRGGBB" hex string → r, g, b (0-255)
---------------------------------------------------------------------------
local function parse_hex(value)
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

---------------------------------------------------------------------------
-- Screen blend (per-channel): 255 - ((255 - a) * (255 - b) >> 8)
-- Matches reference ColorUtils::ScreenChanel exactly
---------------------------------------------------------------------------
local function screen_blend(r1, g1, b1, r2, g2, b2)
    return 255 - math_floor((255 - r1) * (255 - r2) / 256),
           255 - math_floor((255 - g1) * (255 - g2) / 256),
           255 - math_floor((255 - b1) * (255 - b2) / 256)
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    -- no-op
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end

    if type(p.speed) == "number" then
        speed = p.speed
    end
    if type(p.glow) == "number" then
        glow = p.glow
    end
    if type(p.radius) == "number" then
        radius = math_floor(p.radius + 0.5)
    end
    if type(p.reverse) == "boolean" then
        reverse = p.reverse
    end
    if type(p.random) == "boolean" then
        local was_random = random_enabled
        random_enabled = p.random
        if random_enabled and not was_random then
            -- Entering random mode: reset to cycling hues (ref behavior)
            hsv1 = { h = 0,   s = 255, v = 255 }
            hsv2 = { h = 180, s = 255, v = 255 }
        end
    end
    if not random_enabled then
        if type(p.color1) == "string" then
            local r, g, b = parse_hex(p.color1)
            if r then
                local h, s, v = rgb_to_hsv_int(r, g, b)
                hsv1 = { h = h, s = s, v = v }
            end
        end
        if type(p.color2) == "string" then
            local r, g, b = parse_hex(p.color2)
            if r then
                local h, s, v = rgb_to_hsv_int(r, g, b)
                hsv2 = { h = h, s = s, v = v }
            end
        end
    end
end

---------------------------------------------------------------------------
-- Render
--
-- Reference algorithm (SwirlCircles.cpp):
--   Two circles orbit the center of the zone, diametrically opposed.
--   Each pixel is colored by blending two radial glows (Screen blend).
--
-- Timing:
--   progress += 0.1 * Speed / FPS   (per frame)
--   => rate = 0.1 * Speed            (radians per second)
--   => progress(t) = t * 0.1 * speed
--
--   Random hue: hsv.hue++ per frame @ ~60 FPS
--   => ~60 deg/s => full cycle in 6s
---------------------------------------------------------------------------

-- Hue cycling rate for random mode (degrees per second, matching ~60 FPS)
local HUE_CYCLE_RATE = 60

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    if type(width) ~= "number" or width <= 0 then
        width = n
    end
    if type(height) ~= "number" or height <= 0 then
        height = 1
    end

    -- Angular position of circle 1
    local progress = t * 0.1 * speed
    if reverse then
        progress = -progress
    end

    -- Circle center positions
    local hx = 0.5 * width
    local hy = 0.5 * height

    local x1 = hx + hx * math_cos(progress)
    local y1 = hy + hy * math_sin(progress)

    -- Circle 2 is diametrically opposite
    local x2 = width  - x1
    local y2 = height - y1

    -- Glow exponent: ref glow = 0.01 * Slider2Val  (range 0.01 - 1.0)
    local glow_exp = 0.01 * glow

    -- Current HSV for each circle
    local h1, s1, v1
    local h2, s2, v2

    if random_enabled then
        -- Hue cycles over time (ref: hsv.hue++ per frame at ~60 FPS)
        local hue_offset = (t * HUE_CYCLE_RATE) % 360
        h1 = hue_offset
        s1 = 255
        v1 = 255
        h2 = (hue_offset + 180) % 360
        s2 = 255
        v2 = 255
    else
        h1 = hsv1.h
        s1 = hsv1.s
        v1 = hsv1.v
        h2 = hsv2.h
        s2 = hsv2.s
        v2 = hsv2.v
    end

    -- Denominator for distance normalization
    local dist_denom = height + width

    local idx = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if idx > n then return end

            ---------------------------------------------------------------
            -- Circle 1 contribution
            ---------------------------------------------------------------
            local dx1 = x1 - x
            local dy1 = y1 - y
            local distance1 = math_sqrt(dx1 * dx1 + dy1 * dy1)

            local d1_pct
            if distance1 < radius then
                d1_pct = 0
            else
                d1_pct = (distance1 / dist_denom) ^ glow_exp
            end

            -- Value attenuated by distance (integer truncation like reference)
            local val1 = math_floor(v1 * (1 - d1_pct))
            if val1 < 0 then val1 = 0 end

            -- Convert to RGB via host (h: 0-360, s: 0-1, v: 0-1)
            local r1, g1, b1 = host.hsv_to_rgb(h1, s1 / 255, val1 / 255)

            ---------------------------------------------------------------
            -- Circle 2 contribution
            ---------------------------------------------------------------
            local dx2 = x2 - x
            local dy2 = y2 - y
            local distance2 = math_sqrt(dx2 * dx2 + dy2 * dy2)

            local d2_pct
            if distance2 < radius then
                d2_pct = 0
            else
                d2_pct = (distance2 / dist_denom) ^ glow_exp
            end

            local val2 = math_floor(v2 * (1 - d2_pct))
            if val2 < 0 then val2 = 0 end

            local r2, g2, b2 = host.hsv_to_rgb(h2, s2 / 255, val2 / 255)

            ---------------------------------------------------------------
            -- Screen blend and output
            ---------------------------------------------------------------
            local r, g, b = screen_blend(r1, g1, b1, r2, g2, b2)
            buffer:set(idx, r, g, b)

            idx = idx + 1
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
