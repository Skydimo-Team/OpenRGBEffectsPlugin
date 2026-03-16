local plugin = {}

local math_floor = math.floor
local math_max   = math.max
local math_min   = math.min
local math_random = math.random

-- Parameters (defaults match reference C++ exactly)
-- Reference: MaxSpeed=100, MinSpeed=1, default=50
-- Reference: Slider2 (Width) Min=1, Max=100, default=20
local speed          = 50
local width_pct      = 20
local random_enabled = false

local user_c0 = { r = 255, g = 0,   b = 0   }  -- color 1
local user_c1 = { r = 0,   g = 0,   b = 255 }  -- color 2

-- Active colors (may be random or user-selected)
local c0 = { r = 255, g = 0,   b = 0   }
local c1 = { r = 0,   g = 0,   b = 255 }

-- Internal state matching reference C++
local progress  = 0.0
local last_step = false
local last_t    = nil

---------------------------------------------------------------------------
-- Helpers
---------------------------------------------------------------------------

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

local function random_rgb_color()
    local r, g, b = host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
    return { r = r, g = g, b = b }
end

--- RGB → HSV  (h: 0-360, s: 0-1, v: 0-1)
local function rgb_to_hsv(r, g, b)
    local rn, gn, bn = r / 255.0, g / 255.0, b / 255.0
    local max_c = math_max(rn, gn, bn)
    local min_c = math_min(rn, gn, bn)
    local delta = max_c - min_c

    local h, s
    if delta == 0 then
        h = 0
    elseif max_c == rn then
        h = 60 * (((gn - bn) / delta) % 6)
    elseif max_c == gn then
        h = 60 * (((bn - rn) / delta) + 2)
    else
        h = 60 * (((rn - gn) / delta) + 4)
    end
    if h < 0 then h = h + 360 end

    s = (max_c == 0) and 0 or (delta / max_c)

    return h, s, max_c
end

--- ColorUtils::Enlight — scale brightness (HSV value) by factor
--- Returns r, g, b (0-255)
local function enlight(color, factor)
    local h, s, v = rgb_to_hsv(color.r, color.g, color.b)
    v = v * factor
    return host.hsv_to_rgb(h, s, v)
end

--- ColorUtils::Interpolate — per-channel linear interpolation
--- fraction=0 → color1, fraction=1 → color2
--- Returns r, g, b (0-255)
local function interpolate(color1, color2, fraction)
    local r = math_floor((color2.r - color1.r) * fraction + color1.r)
    local g = math_floor((color2.g - color1.g) * fraction + color1.g)
    local b = math_floor((color2.b - color1.b) * fraction + color1.b)
    return r, g, b
end

---------------------------------------------------------------------------
-- Core rendering — faithfully ported from Visor::GetColor()
--
-- Parameters:
--   i     : LED index (0-based)
--   count : total LED count along the axis
--   w     : visor width as fraction [0-1]
--   p_step: triangular progress within current half-cycle [0-1]
--   step  : boolean, true = first half, false = second half
---------------------------------------------------------------------------
local function get_color(i, count, w, p_step, step)
    -- Enforce absolute minimum visor size (reference: 1.5/count)
    w = math_max(1.5 / count, w)

    -- Visor head position —— sweeps through the range with overshoot
    -- so that the visor fully enters from one side and exits the other.
    local x_step = p_step * (1.0 + 4.0 * w) - 1.5 * w

    -- Avoid division by zero (reference guard)
    if count <= 1 then
        count = 2
    end

    -- Normalised position of this LED [0-1]
    local x = i / (count - 1)

    -- Signed distance from visor head to this LED
    local dist = x_step - x

    -- Region 1: HEAD — LED is ahead of the visor (dist < 0)
    if dist < 0 then
        local l = math_max(0, math_min((w + dist) / w, 1.0))
        if step then
            return enlight(c1, l)
        else
            return enlight(c0, l)
        end
    end

    -- Region 2: TAIL — LED is behind the visor (dist > w)
    if dist > w then
        local l = math_max(0, math_min(1.0 - ((dist - w) / w), 1.0))
        if step then
            return enlight(c0, l)
        else
            return enlight(c1, l)
        end
    end

    -- Region 3: BODY — LED is within the visor bar (0 <= dist <= w)
    local interp = math_min(math_max((w - dist) / w, 0.0), 1.0)

    if step then
        return interpolate(c0, c1, interp)
    else
        return interpolate(c1, c0, interp)
    end
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    math.randomseed(os.clock() * 1000 + os.time())
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = math_max(1, math_min(100, p.speed))
    end

    if type(p.width) == "number" then
        width_pct = math_max(1, math_min(100, p.width))
    end

    if type(p.random) == "boolean" then
        random_enabled = p.random
    end

    if type(p.colors) == "table" and #p.colors >= 2 then
        local parsed0 = parse_hex_color(p.colors[1])
        local parsed1 = parse_hex_color(p.colors[2])
        if parsed0 then user_c0 = parsed0 end
        if parsed1 then user_c1 = parsed1 end
    end
end

function plugin.on_tick(t, buffer, width_hw, height)
    local n = buffer:len()
    if n <= 0 then
        return
    end

    if type(width_hw) ~= "number" or width_hw <= 0 then
        width_hw = n
    end
    if type(height) ~= "number" or height <= 0 then
        height = 1
    end

    -- Compute elapsed delta (same pattern as other project plugins)
    local delta = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            delta = t
        else
            delta = t - last_t
        end
        last_t = t
    end

    -- Reference: Progress += 0.01 * Speed / FPS
    -- which is d(Progress)/dt = 0.01 * Speed.
    -- With our speed range 1-100 (default 50), map identically.
    progress = progress + 0.01 * speed * delta

    -- Visor width as [0-1] fraction (reference: 0.01 * Slider2Val)
    local w = 0.01 * width_pct

    -- Fractional progress [0-1], repeating
    local p = progress - math_floor(progress)

    -- Which half of the cycle are we in?
    local step = (p < 0.5)

    -- Triangular wave within the current half: 0→1→0
    local p_step
    if step then
        p_step = 2.0 * p
    else
        p_step = 2.0 * (1.0 - p)
    end

    -- Detect color flip at half-cycle boundary
    local flipping = (last_step ~= step)
    if flipping then
        last_step = step
    end

    -- Update active colors
    if flipping and random_enabled then
        c0 = random_rgb_color()
        c1 = random_rgb_color()
    elseif not random_enabled then
        c0 = user_c0
        c1 = user_c1
    end

    -- Render
    if height <= 1 then
        -- SINGLE / LINEAR: iterate over all LEDs
        for led = 1, n do
            local r, g, b = get_color(led - 1, width_hw, w, p_step, step)
            buffer:set(led, r, g, b)
        end
    else
        -- MATRIX: visor sweeps along columns, same color for all rows in a column
        local idx = 1
        for _ = 0, height - 1 do
            for col = 0, width_hw - 1 do
                if idx > n then
                    return
                end
                local r, g, b = get_color(col, width_hw, w, p_step, step)
                buffer:set(idx, r, g, b)
                idx = idx + 1
            end
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
