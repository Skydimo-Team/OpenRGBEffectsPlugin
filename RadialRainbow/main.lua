local plugin = {}

-- Integer HSV conversion (MIT, copyright Martin Mitas).

local UINT32 = 4294967296.0
local SHAPE_CIRCLES = 0
local SHAPE_SQUARES = 1

local math_abs = math.abs
local math_ceil = math.ceil
local math_floor = math.floor
local math_max = math.max
local math_min = math.min
local math_sqrt = math.sqrt

local speed = 100
local frequency = 50
local cx_shift = 50
local cy_shift = 50
local shape = SHAPE_CIRCLES

local progress = 0.0
local last_t = nil

local function clamp(value, min_value, max_value)
    return math_min(math_max(value, min_value), max_value)
end

local function trunc_toward_zero(value)
    if value < 0 then
        return math_ceil(value)
    end
    return math_floor(value)
end

local function wrap_reference_hue(raw_hue)
    local hue = trunc_toward_zero(raw_hue)

    if hue < 0 or hue >= UINT32 then
        hue = hue % UINT32
    end

    return hue % 360
end

local function floor_div(numerator, denominator)
    return math_floor(numerator / denominator)
end

-- Keep the original integer arithmetic so the rainbow matches the
-- reference effect instead of drifting by +/-1 from float HSV conversion.
local function reference_hsv_to_rgb(hue, saturation, value)
    if saturation == 0 then
        return value, value, value
    end

    local h = hue % 360
    local s = saturation
    local v = value
    local sector = floor_div(h, 60)
    local p = floor_div(256 * v - s * v, 256)

    if sector % 2 == 1 then
        local q = floor_div(256 * 60 * v - h * s * v + 60 * s * v * sector, 256 * 60)
        if sector == 1 then
            return q, v, p
        elseif sector == 3 then
            return p, q, v
        end
        return v, p, q
    end

    local t = floor_div(256 * 60 * v + h * s * v - 60 * s * v * (sector + 1), 256 * 60)
    if sector == 0 then
        return v, t, p
    elseif sector == 2 then
        return p, v, t
    end
    return t, p, v
end

local function resolve_distance(x, y, center_x, center_y)
    if shape == SHAPE_CIRCLES then
        local dx = center_x - x
        local dy = center_y - y
        return math_sqrt(dx * dx + dy * dy)
    elseif shape == SHAPE_SQUARES then
        return math_max(math_abs(center_y - y), math_abs(center_x - x))
    end

    return nil
end

function plugin.on_init()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = clamp(p.speed, 1, 200)
    end

    if type(p.frequency) == "number" then
        frequency = clamp(p.frequency, 1, 100)
    end

    if type(p.cx) == "number" then
        cx_shift = clamp(p.cx, 0, 100)
    end

    if type(p.cy) == "number" then
        cy_shift = clamp(p.cy, 0, 100)
    end

    if type(p.shape) == "number" then
        local next_shape = trunc_toward_zero(p.shape)
        if next_shape == SHAPE_CIRCLES or next_shape == SHAPE_SQUARES then
            shape = next_shape
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

    local current_progress = progress
    local band_width = frequency * 0.5
    local center_x
    local center_y

    -- Uses `leds_count * shift` for linear zones, but `(cols - 1) * shift`
    -- for matrices; this keeps center placement identical.
    if height <= 1 then
        center_x = width * (cx_shift / 100.0)
        center_y = 0.0
    else
        center_x = (width - 1) * (cx_shift / 100.0)
        center_y = (height - 1) * (cy_shift / 100.0)
    end

    local led = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then
                return
            end

            local distance = resolve_distance(x, y, center_x, center_y)
            if distance == nil then
                buffer:set(led, 0, 0, 0)
            else
                local raw_hue = distance * band_width - current_progress
                local hue = wrap_reference_hue(raw_hue)
                local r, g, b = reference_hsv_to_rgb(hue, 255, 255)
                buffer:set(led, r, g, b)
            end

            led = led + 1
        end
    end

    if type(t) == "number" and t >= 0 then
        if last_t == nil or t < last_t then
            last_t = t
        else
            progress = progress + speed * (t - last_t)
            last_t = t
        end
    end
end

function plugin.on_shutdown()
end

return plugin
