local plugin = {}

local math_abs = math.abs
local math_cos = math.cos
local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_sin = math.sin
local math_sqrt = math.sqrt
local math_pow = math.pow or function(a, b) return a ^ b end
local TAU = math.pi * 2.0
local REFERENCE_FPS = 60.0

local MODE_CLOCKWISE = 0
local MODE_COUNTER_CLOCKWISE = 1
local MODE_PENDULUM = 2
local MODE_WIPERS = 3
local MODE_SWING_H = 4
local MODE_SWING_V = 5

-- Parameters (matching the C++ RotatingBeam defaults)
local speed = 50
local glow = 10
local thickness = 0
local mode = MODE_CLOCKWISE
local random_colors = false

-- Internal state
local progress = 0.0
local user_colors = { "#FF0000", "#0000FF" }

-- Active HSV colors.
-- When random_colors is disabled these mirror user_colors.
-- When enabled they become the animated rotating hues from the reference effect.
local hsv1_h, hsv1_s, hsv1_v = 0.0, 1.0, 1.0
local hsv2_h, hsv2_s, hsv2_v = 240.0, 1.0, 1.0

local function clamp(value, lo, hi)
    if value < lo then
        return lo
    end
    if value > hi then
        return hi
    end
    return value
end

local function lerp_channel(a, b, t)
    return math_floor(a + (b - a) * t + 0.5)
end

local function hex_to_rgb(hex)
    if type(hex) ~= "string" then
        return 255, 0, 0
    end

    hex = hex:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then
        hex = hex:sub(2)
    end
    if #hex == 3 then
        hex = hex:sub(1, 1):rep(2) .. hex:sub(2, 2):rep(2) .. hex:sub(3, 3):rep(2)
    end
    if #hex ~= 6 or hex:find("[^%x]") then
        return 255, 0, 0
    end

    return tonumber(hex:sub(1, 2), 16) or 255,
        tonumber(hex:sub(3, 4), 16) or 0,
        tonumber(hex:sub(5, 6), 16) or 0
end

local function rgb_to_hsv(r, g, b)
    local rf = clamp(r / 255.0, 0.0, 1.0)
    local gf = clamp(g / 255.0, 0.0, 1.0)
    local bf = clamp(b / 255.0, 0.0, 1.0)
    local maxc = math_max(rf, gf, bf)
    local minc = math_min(rf, gf, bf)
    local delta = maxc - minc

    local h = 0.0
    local s = 0.0
    local v = maxc

    if maxc > 0.0 then
        s = delta / maxc
    end

    if delta > 0.0 then
        if maxc == rf then
            h = 60.0 * (((gf - bf) / delta) % 6.0)
        elseif maxc == gf then
            h = 60.0 * (((bf - rf) / delta) + 2.0)
        else
            h = 60.0 * (((rf - gf) / delta) + 4.0)
        end
    end

    if h < 0.0 then
        h = h + 360.0
    end

    return h, s, v
end

local function apply_user_colors()
    local r1, g1, b1 = hex_to_rgb(user_colors[1])
    local r2, g2, b2 = hex_to_rgb(user_colors[2])

    hsv1_h, hsv1_s, hsv1_v = rgb_to_hsv(r1, g1, b1)
    hsv2_h, hsv2_s, hsv2_v = rgb_to_hsv(r2, g2, b2)
end

local function set_random_colors_enabled(enabled)
    random_colors = enabled

    if enabled then
        hsv1_h, hsv1_s, hsv1_v = 0.0, 1.0, 1.0
        hsv2_h, hsv2_s, hsv2_v = 180.0, 1.0, 1.0
    else
        apply_user_colors()
    end
end

local function resolve_line_points()
    if mode == MODE_CLOCKWISE then
        local x = 0.5 * (1.0 + math_cos(progress))
        local y = 0.5 * (1.0 + math_sin(progress))
        return x, y, 1.0 - x, 1.0 - y
    end

    if mode == MODE_COUNTER_CLOCKWISE then
        local x = 0.5 * (1.0 + math_cos(-progress))
        local y = 0.5 * (1.0 + math_sin(-progress))
        return x, y, 1.0 - x, 1.0 - y
    end

    if mode == MODE_PENDULUM then
        local x = 0.5 * (1.0 + math_cos(progress))
        return 0.5, 0.0, x, 1.0
    end

    if mode == MODE_WIPERS then
        local x = 0.5 * (1.0 + math_cos(progress))
        return x, 0.0, 0.5, 1.0
    end

    if mode == MODE_SWING_H then
        local x = 0.5 * (1.0 + math_cos(progress))
        return 0.0, x, 1.0, 1.0 - x
    end

    if mode == MODE_SWING_V then
        local x = 0.5 * (1.0 + math_cos(progress))
        return x, 0.0, 1.0 - x, 1.0
    end

    return 0.0, 0.0, 1.0, 1.0
end

local function line_distance(x0, y0, p1x, p1y, p2x, p2y, w, h)
    local x1 = p1x * w
    local x2 = p2x * w
    local y1 = p1y * h
    local y2 = p2y * h

    local dx = x2 - x1
    local dy = y2 - y1
    local denom = math_sqrt(dx * dx + dy * dy)
    if denom <= 1e-9 then
        return 0.0
    end

    return math_abs(dx * (y1 - y0) - (x1 - x0) * dy) / denom
end

local function render_sample(buffer, idx, x0, y0, p1x, p1y, p2x, p2y, w, h, avg_dim, bg_r, bg_g, bg_b)
    local distance = line_distance(x0, y0, p1x, p1y, p2x, p2y, w, h)
    local distance_norm = 0.0
    if avg_dim > 0.0 then
        distance_norm = distance / avg_dim
    end

    local exponent = (distance < thickness) and 1.0 or (0.01 * glow)
    local beam_value = hsv1_v - hsv1_v * math_pow(distance_norm, exponent)
    beam_value = clamp(beam_value, 0.0, 1.0)

    local beam_r, beam_g, beam_b = host.hsv_to_rgb(hsv1_h, hsv1_s, beam_value)
    local mix = clamp(1.0 - distance_norm, 0.0, 1.0)

    buffer:set(
        idx,
        lerp_channel(bg_r, beam_r, mix),
        lerp_channel(bg_g, beam_g, mix),
        lerp_channel(bg_b, beam_b, mix)
    )
end

function plugin.on_init()
    progress = 0.0
    apply_user_colors()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = p.speed
    end

    if type(p.glow) == "number" then
        glow = p.glow
    end

    if type(p.thickness) == "number" then
        thickness = p.thickness
    end

    if type(p.mode) == "number" then
        local next_mode = math_floor(p.mode + 0.5)
        if next_mode >= MODE_CLOCKWISE and next_mode <= MODE_SWING_V then
            mode = next_mode
        end
    end

    local colors_updated = false
    if type(p.colors) == "table" then
        if type(p.colors[1]) == "string" then
            user_colors[1] = p.colors[1]
            colors_updated = true
        end
        if type(p.colors[2]) == "string" then
            user_colors[2] = p.colors[2]
            colors_updated = true
        end
    end

    local turned_off_random = false
    if type(p.random_colors) == "boolean" and p.random_colors ~= random_colors then
        set_random_colors_enabled(p.random_colors)
        turned_off_random = not random_colors
    end

    if not random_colors and (colors_updated or turned_off_random) then
        apply_user_colors()
    end
end

function plugin.on_tick(_, buffer, width, height)
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

    local p1x, p1y, p2x, p2y = resolve_line_points()
    local bg_r, bg_g, bg_b = host.hsv_to_rgb(hsv2_h, hsv2_s, hsv2_v)

    if height <= 1 then
        -- OpenRGB renders linear zones on a square virtual canvas and samples
        -- a fixed row at y = width * 0.25. Recreate that mapping exactly.
        local dim = width - 1
        local avg_dim = dim
        local sample_y = width * 0.25

        for i = 1, width do
            render_sample(buffer, i, i - 1, sample_y, p1x, p1y, p2x, p2y, dim, dim, avg_dim, bg_r, bg_g, bg_b)
        end
    else
        local w = width - 1
        local h = height - 1
        local avg_dim = 0.5 * (w + h)
        local idx = 1

        for y = 0, height - 1 do
            for x = 0, width - 1 do
                if idx > n then
                    return
                end

                render_sample(buffer, idx, x, y, p1x, p1y, p2x, p2y, w, h, avg_dim, bg_r, bg_g, bg_b)
                idx = idx + 1
            end
        end
    end

    progress = (progress + (0.1 * speed / REFERENCE_FPS)) % TAU

    if random_colors then
        hsv1_h = (hsv1_h + 1.0) % 360.0
        hsv2_h = (hsv2_h + 1.0) % 360.0
    end
end

function plugin.on_shutdown()
end

return plugin
