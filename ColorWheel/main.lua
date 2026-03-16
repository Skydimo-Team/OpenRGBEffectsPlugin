local plugin = {}

local math_floor = math.floor
local pi = math.pi

-- Use math.atan with two args (Lua 5.3+/5.4 atan2 equivalent)
local atan2 = function(y, x) return math.atan(y, x) end

-- Parameters with defaults
local speed     = 50
local cx_shift  = 50
local cy_shift  = 50
local direction = 0  -- 0 = clockwise, 1 = counter-clockwise

-- Running progress (accumulated rotation in degrees)
local progress = 0.0

function plugin.on_init()
    -- no-op
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end
    if type(p.speed) == "number" then
        speed = p.speed
    end
    if type(p.cx) == "number" then
        cx_shift = p.cx
    end
    if type(p.cy) == "number" then
        cy_shift = p.cy
    end
    if type(p.direction) == "number" then
        local next_direction = math_floor(p.direction + 0.5)
        if next_direction == 0 or next_direction == 1 then
            direction = next_direction
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

    local cx_mult = cx_shift / 100.0
    local cy_mult = cy_shift / 100.0
    local cx = (width - 1) * cx_mult
    local cy = (height - 1) * cy_mult

    -- Screen-space atan2 already encodes the wheel orientation.
    -- The direction selector should only control how the wheel animates over time.
    local dir_mult = (direction == 0) and 1.0 or -1.0

    local i = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if i > n then
                return
            end

            local angle = atan2(y - cy, x - cx)

            -- Convert angle to degrees [0, 360) and rotate over time.
            -- Clockwise should advance in the opposite angular direction of the hue offset,
            -- while counter-clockwise should do the reverse.
            local hue = (180.0 + angle * (180.0 / pi) - dir_mult * progress) % 360.0

            buffer:set_hsv(i, hue, 1.0, 1.0)

            i = i + 1
        end
    end

    -- Advance rotation: speed 50 maps to a moderate rotation rate
    -- Reference used Speed/FPS; we use speed param scaled similarly
    progress = (progress + speed * 0.05) % 360.0
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
