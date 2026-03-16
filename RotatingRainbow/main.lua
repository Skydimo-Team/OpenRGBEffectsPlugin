local plugin = {}

local math_cos = math.cos
local math_sin = math.sin

local speed = 20
local color_speed = 30
local reverse = false

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
    if type(p.reverse) == "boolean" then
        reverse = p.reverse
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

    -- Match reference: time = 1000.0 + 0.1 * Speed * elapsed
    -- Reference accumulates time += 0.1 * Speed / FPS each frame;
    -- after t seconds at FPS fps, that equals 0.1 * Speed * t.
    -- The 1000.0 offset is the reference's initial phase.
    local time = 1000.0 + 0.1 * speed * t

    local rot = reverse and -time or time
    local cos_t = math_cos(rot)
    local sin_t = math_sin(rot)

    -- Reference uses different center computation for Linear vs Matrix:
    -- Linear: cx = leds_count * 0.5, cy = 0.5, y = 0.5 (y term cancels)
    -- Matrix: cx = (cols - 1) * 0.5, cy = (rows - 1) * 0.5
    local is_linear = (height <= 1)
    local cx = is_linear and (width * 0.5) or ((width - 1) * 0.5)
    local cy = is_linear and 0.5 or ((height - 1) * 0.5)

    local cs = color_speed

    local i = 1
    for y = 0, height - 1 do
        -- For linear zones, reference passes y = 0.5 (matching cy = 0.5, so dy = 0)
        local fy = is_linear and 0.5 or y
        local dy_cos = (fy - cy) * 2.0 * cos_t

        for x = 0, width - 1 do
            if i > n then
                return
            end

            local hue = (time * cs + 360.0 * (dy_cos + (x - cx) * 2.0 * sin_t) / 128.0) % 360.0
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
