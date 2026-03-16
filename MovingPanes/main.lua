local plugin = {}

local math_sin   = math.sin
local math_floor = math.floor
local math_max   = math.max
local math_min   = math.min

---------------------------------------------------------------------------
-- Parameters
--   Original C++ reference:
--     Speed     1–100, default 50
--     Slider2Val (Divisions)  2–50, default 4
--     UserColors  2 colors
--     IsReversable  true
--
-- Time advancement (original): time += 0.1 * Speed / FPS  (per frame)
--   → per second: 0.1 * Speed
---------------------------------------------------------------------------

local speed     = 50
local divisions = 4
local reverse   = false

local user_colors = {
    { r = 255, g = 0, b = 0 },
    { r = 0, g = 0, b = 255 },
}

---------------------------------------------------------------------------
-- Internal state
---------------------------------------------------------------------------
local time_acc = 0.0
local last_t   = nil

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

--- Clamp to 0–255 and truncate, matching C++ int cast behaviour.
local function trunc_channel(value)
    if value < 0 then
        return 0
    elseif value > 255 then
        return 255
    end
    return math_floor(value)
end

--- Linear interpolation between two RGB colors (matches ColorUtils::Interpolate).
--- @param c1 table  {r,g,b} – start color
--- @param c2 table  {r,g,b} – end color
--- @param s  number  blend factor 0..1
local function interpolate_rgb(c1, c2, s)
    local r = trunc_channel(c1.r + s * (c2.r - c1.r))
    local g = trunc_channel(c1.g + s * (c2.g - c1.g))
    local b = trunc_channel(c1.b + s * (c2.b - c1.b))
    return r, g, b
end

---------------------------------------------------------------------------
-- Core colour function – ported verbatim from MovingPanes::GetColor
--
--   int   zone    = x / (w / Slider2Val)
--   int   zone_id = zone % 2
--   float pi4     = 3.14 * 0.25
--   float t       = reverse ? time : -time
--   float s       = 0.5 * (1 + sin(y / (h * 0.25) + (zone_id ? 1 : -1) * t + pi4))
--   return Interpolate(UserColors[zone_id?1:0], UserColors[zone_id?0:1], s)
---------------------------------------------------------------------------
local PI4 = 3.14 * 0.25

local function get_color(x, y, w, h, t_signed)
    local zone    = math_floor(x / (w / divisions))
    local zone_id = zone % 2  -- 0 or 1

    local direction = (zone_id ~= 0) and 1 or -1
    local s = 0.5 * (1.0 + math_sin(y / (h * 0.25) + direction * t_signed + PI4))

    -- zone_id == 0  →  Interpolate(color[0], color[1], s)
    -- zone_id == 1  →  Interpolate(color[1], color[0], s)
    local c1, c2
    if zone_id ~= 0 then
        c1 = user_colors[2]
        c2 = user_colors[1]
    else
        c1 = user_colors[1]
        c2 = user_colors[2]
    end

    return interpolate_rgb(c1, c2, s)
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    time_acc = 0.0
    last_t   = nil
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = math_max(1, math_min(100, math_floor(p.speed + 0.5)))
    end

    if type(p.divisions) == "number" then
        divisions = math_max(2, math_min(50, math_floor(p.divisions + 0.5)))
    end

    if type(p.reverse) == "boolean" then
        reverse = p.reverse
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

    if type(width) ~= "number" or width <= 0 then
        width = n
    end
    if type(height) ~= "number" or height <= 0 then
        height = 1
    end

    -- Compute delta time
    local dt = 0.0
    if type(t) == "number" and t >= 0 then
        if last_t ~= nil and t >= last_t then
            dt = t - last_t
        end
        last_t = t
    end

    -- Signed time for direction: original uses  t = reverse ? time : -time
    local t_signed = reverse and time_acc or -time_acc

    -- Render
    if height <= 1 then
        -- LINEAR / SINGLE: original calls GetColor(LedID, LedID, leds_count, leds_count, reverse)
        local w = math_max(1, width)
        for led = 1, n do
            local x = led - 1  -- 0-based index
            local r, g, b = get_color(x, x, w, w, t_signed)
            buffer:set(led, r, g, b)
        end
    else
        -- MATRIX: original calls GetColor(col_id, row_id, cols, rows, reverse)
        local cols = width
        local rows = height
        local led = 1
        for row = 0, rows - 1 do
            for col = 0, cols - 1 do
                if led > n then
                    goto done
                end
                local r, g, b = get_color(col, row, cols, rows, t_signed)
                buffer:set(led, r, g, b)
                led = led + 1
            end
        end
        ::done::
    end

    -- Advance time after rendering (matches original order):
    --   time += 0.1 * Speed / FPS  →  per second: 0.1 * Speed
    time_acc = time_acc + 0.1 * speed * dt
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
