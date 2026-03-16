local plugin = {}

local math_cos = math.cos
local math_sin = math.sin
local math_abs = math.abs

local speed = 50
local color_speed = 20
local frequency = 1

function plugin.on_init()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end
    if type(p.speed) == "number" then
        speed = p.speed
    end
    if type(p.color_speed) == "number" then
        color_speed = p.color_speed
    end
    if type(p.frequency) == "number" then
        frequency = p.frequency
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

    local et = t * 0.01 * speed
    local cos_t = math_cos(et)
    local sin_t = math_sin(et)

    local cx = (width - 1) * 0.5
    local cy = (height > 1) and ((height - 1) * 0.5) or 0.5

    -- Tent function base: symmetric from center, creating the "double" rainbow.
    -- Proportional to width so the pattern scales with LED count.
    local half_w = width * 0.44

    local cs = color_speed
    local freq = frequency

    local i = 1
    for y = 0, height - 1 do
        local fdy_c = freq * (y - cy) * cos_t

        for x = 0, width - 1 do
            if i > n then
                return
            end

            local dx = half_w - math_abs(x - cx)
            local hue = (et * cs + 360.0 * (fdy_c + dx * freq * sin_t) / 128.0) % 360.0
            if hue < 0 then
                hue = hue + 360.0
            end

            buffer:set_hsv(i, hue, 1.0, 1.0)
            i = i + 1
        end
    end
end

function plugin.on_shutdown()
end

return plugin
