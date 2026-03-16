local plugin = {}

local math_abs   = math.abs
local math_atan  = math.atan
local math_floor = math.floor
local math_max   = math.max
local math_modf  = math.modf
local math_sqrt  = math.sqrt
local PI = math.pi

-- ---------- Constants ----------

local COLOR_MODE_RAINBOW = 0
local COLOR_MODE_SINGLE  = 1

local DIR_DEFAULT  = 0  -- counter-clockwise (original reverse=false)
local DIR_REVERSED = 1  -- clockwise          (original reverse=true)

-- ---------- Parameters ----------

local speed      = 50
local shape      = 10
local direction  = DIR_DEFAULT
local color_mode = COLOR_MODE_RAINBOW
local user_color = { r = 255, g = 0, b = 0 }

-- ---------- Internal state ----------

local time_acc = 0.0
local prev_t   = 0.0

-- Cached HSV of user color (updated on param change)
local user_h = 0.0
local user_s = 1.0

-- ---------- Helpers ----------

--- Truncate toward zero (C-style cast to int).
local function c_trunc(value)
    local int_part = math_modf(value)
    return int_part
end

--- Emulate C++  abs( (int)(value) % divisor ).
--- C++ integer modulo keeps the sign of the dividend; Lua's % always returns >= 0.
local function c_abs_int_mod(value, divisor)
    local int_val   = c_trunc(value)
    local quotient  = c_trunc(int_val / divisor)
    local remainder = int_val - quotient * divisor
    return math_abs(remainder)
end

--- Parse "#RRGGBB" or "#RGB" hex string into {r, g, b}.
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

--- Convert RGB (0-255 each) to HSV.
--- Returns h (0-360), s (0-1), v (0-1).
local function rgb_to_hsv(r, g, b)
    local r_n = r / 255.0
    local g_n = g / 255.0
    local b_n = b / 255.0

    local max_c = math_max(r_n, g_n, b_n)
    local min_c = math.min(r_n, g_n, b_n)
    local delta = max_c - min_c

    local h, s
    if max_c == 0 or delta == 0 then
        h = 0
        s = (max_c == 0) and 0 or 0
    else
        s = delta / max_c
        if max_c == r_n then
            h = 60.0 * (((g_n - b_n) / delta) % 6.0)
        elseif max_c == g_n then
            h = 60.0 * (((b_n - r_n) / delta) + 2.0)
        else
            h = 60.0 * (((r_n - g_n) / delta) + 4.0)
        end
    end

    if h < 0 then h = h + 360.0 end

    return h, s, max_c
end

--- Recompute cached user HSV from user_color.
local function update_user_hsv()
    user_h, user_s, _ = rgb_to_hsv(user_color.r, user_color.g, user_color.b)
end

-- ---------- Plugin callbacks ----------

function plugin.on_init()
    update_user_hsv()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = p.speed
    end
    if type(p.shape) == "number" then
        shape = p.shape
    end
    if type(p.direction) == "number" then
        local d = math_floor(p.direction + 0.5)
        if d == DIR_DEFAULT or d == DIR_REVERSED then
            direction = d
        end
    end
    if type(p.color_mode) == "number" then
        local m = math_floor(p.color_mode + 0.5)
        if m == COLOR_MODE_RAINBOW or m == COLOR_MODE_SINGLE then
            color_mode = m
        end
    end
    if p.color ~= nil then
        local parsed = parse_hex_color(p.color)
        if parsed then
            user_color = parsed
            update_user_hsv()
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

    -- Time accumulation (smooth across speed changes).
    -- Original: time += Speed / FPS  →  effectively Speed per second.
    -- Our speed 50 ≈ original Speed 200  →  factor = speed × 4.
    local dt = t - prev_t
    prev_t = t
    time_acc = time_acc + speed * 4.0 * dt

    -- Centre point (matches original Linear vs Matrix logic).
    local cx, cy
    if height <= 1 then
        -- Linear: centre at (count*0.5, 0.5)
        cx = width * 0.5
        cy = 0.5
    else
        -- Matrix: centre at ((cols-1)*0.5, (rows-1)*0.5)
        cx = (width - 1) * 0.5
        cy = (height - 1) * 0.5
    end

    -- Direction sign (original: reverse→+atan2, normal→-atan2).
    local dir_sign = (direction == DIR_REVERSED) and 1.0 or -1.0

    local is_rainbow = (color_mode == COLOR_MODE_RAINBOW)
    local cur_shape  = shape
    local cur_time   = time_acc
    local cur_user_h = user_h
    local cur_user_s = user_s

    local i = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if i > n then
                return
            end

            -- Original uses atan2(x-cx, y-cy) — note the swapped argument order
            -- relative to the standard atan2(y,x). This produces the angle from the
            -- positive-Y axis rather than positive-X, giving the spiral its
            -- characteristic orientation.
            local angle    = dir_sign * math_atan(x - cx, y - cy) * 180.0 / PI
            local distance = math_sqrt((cx - x) ^ 2 + (cy - y) ^ 2)
            local combined = angle + cur_shape * distance - cur_time

            if is_rainbow then
                -- Rainbow: full-saturation hue spiral
                local hue = c_abs_int_mod(combined, 360)
                buffer:set_hsv(i, hue, 1.0, 1.0)
            else
                -- Single colour: user hue/sat with brightness modulated by the spiral
                local val = 1.0 - c_abs_int_mod(combined, 360) / 360.0
                buffer:set_hsv(i, cur_user_h, cur_user_s, val)
            end

            i = i + 1
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
