local color = require("lib.color")
local plugin = {}

local math_sin   = math.sin
local math_abs   = math.abs
local math_min   = math.min
local math_pow   = math.pow or function(a, b) return a ^ b end
local math_floor = math.floor

-- Parameters (matching C++ defaults)
local speed         = 50   -- mapped from original Speed 1-10 range
local h_speed       = 1    -- horizontal sine multiplier (1-100)
local v_speed       = 1    -- vertical sine multiplier (1-100)
local glow          = 1    -- glow falloff exponent (1-100)
local thickness     = 0    -- flat beam core width in pixels (0-100)
local random_colors = false

-- Internal state
local progress = 0.0

-- HSV state for the two beams (h: 0-360, s: 0-1, v: 0-1)
-- Default: beam1 = red, beam2 = blue
local hsv1_h, hsv1_s, hsv1_v = 0, 1.0, 1.0
local hsv2_h, hsv2_s, hsv2_v = 240, 1.0, 1.0

local function reset_user_colors(colors)
    if type(colors) ~= "table" or #colors < 2 then return end
    local r1, g1, b1 = color.hex_to_rgb(colors[1])
    local r2, g2, b2 = color.hex_to_rgb(colors[2])
    hsv1_h, hsv1_s, hsv1_v = color.rgb_to_hsv(r1, g1, b1)
    hsv2_h, hsv2_s, hsv2_v = color.rgb_to_hsv(r2, g2, b2)
end

function plugin.on_init()
    -- no-op
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end
    if type(p.speed) == "number" then
        speed = p.speed
    end
    if type(p.h_speed) == "number" then
        h_speed = p.h_speed
    end
    if type(p.v_speed) == "number" then
        v_speed = p.v_speed
    end
    if type(p.glow) == "number" then
        glow = p.glow
    end
    if type(p.thickness) == "number" then
        thickness = p.thickness
    end
    if type(p.random_colors) == "boolean" then
        local was_random = random_colors
        random_colors = p.random_colors
        if random_colors then
            -- C++ ref: hsv1.hue=0, hsv2.hue=180, sat=255, val=255
            hsv1_h = 0;   hsv1_s = 1.0; hsv1_v = 1.0
            hsv2_h = 180; hsv2_s = 1.0; hsv2_v = 1.0
        elseif was_random and not random_colors then
            -- Switching off random: reset to user colors
            if type(p.colors) == "table" then
                reset_user_colors(p.colors)
            end
        end
    end
    if type(p.colors) == "table" and not random_colors then
        reset_user_colors(p.colors)
    end
end

function plugin.on_tick(elapsed, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    if type(width) ~= "number" or width <= 0 then width = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    -- Compute sine wave positions (matching C++ exactly)
    -- C++ original: sine_x = sin(0.01 * h_speed * progress)
    local sine_x = math_sin(0.01 * h_speed * progress)
    local sine_y = math_sin(0.01 * v_speed * progress)

    -- Map sine [-1, 1] to position in pixel space
    local x_progress = 0.5 * (1 + sine_x) * width
    local y_progress = 0.5 * (1 + sine_y) * height

    local idx = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if idx > n then return end

            -- Distance from vertical beam (beam 1 sweeps horizontally)
            local distance_x = math_abs(x_progress - x)
            local dx_norm = distance_x / width
            local distance_x_pct
            if distance_x > thickness then
                distance_x_pct = math_min(1, math_pow(dx_norm, 0.01 * glow))
            else
                distance_x_pct = math_min(1, dx_norm)
            end

            -- Distance from horizontal beam (beam 2 sweeps vertically)
            local distance_y = math_abs(y_progress - y)
            local dy_norm = distance_y / height
            local distance_y_pct
            if distance_y > thickness then
                distance_y_pct = math_min(1, math_pow(dy_norm, 0.01 * glow))
            else
                distance_y_pct = math_min(1, dy_norm)
            end

            -- Beam 1 color: hsv1 with value scaled by distance
            local v1 = hsv1_v * (1 - distance_x_pct)
            local r1, g1, b1 = color.hsv_to_rgb(hsv1_h, hsv1_s, v1)

            -- Beam 2 color: hsv2 with value scaled by distance
            local v2 = hsv2_v * (1 - distance_y_pct)
            local r2, g2, b2 = color.hsv_to_rgb(hsv2_h, hsv2_s, v2)

            -- Screen blend (additive-ish, matching C++ ColorUtils::Screen)
            local r = color.screen_blend(r1, r2)
            local g = color.screen_blend(g1, g2)
            local b = color.screen_blend(b1, b2)

            buffer:set(idx, r, g, b)
            idx = idx + 1
        end
    end

    -- Advance progress
    -- C++ original: progress += Speed / FPS
    -- Original Speed range: 1-10, default 5, FPS ~60
    -- Our speed range: 1-100, default 50
    -- Map: speed 50 -> original Speed 5, FPS 60 -> 5/60 ≈ 0.0833
    -- So: (speed / 10) / 60 = speed / 600
    progress = progress + speed / 600.0

    -- Random color hue cycling
    -- C++ original: hsv1.hue++ (integer 0-359), once per frame at ~60fps
    -- Our hue is 0-360 float. At 60fps, 1 degree/frame = 6 degrees/tick at ~60fps
    -- We scale by 1.0 per tick to match ~1 degree per frame
    if random_colors then
        hsv1_h = (hsv1_h + 1) % 360
        hsv2_h = (hsv2_h + 1) % 360
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
