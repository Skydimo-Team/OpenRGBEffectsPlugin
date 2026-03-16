local plugin = {}

local math_floor = math.floor
local math_min = math.min
local math_max = math.max
local math_sqrt = math.sqrt
local math_sin = math.sin

-- Parameters with defaults matching the original OpenRGB Sunrise effect
local speed = 10             -- 1 - 20
local max_intensity = 80     -- 1 - 99
local intensity_speed = 10   -- 1 - 100
local radius = 50            -- 1 - 100
local grow_speed = 10        -- 1 - 50
local motion = false
local run_once = false

-- Default user colors: white, yellow, red, black
local user_colors = {
    { r = 255, g = 255, b = 255 },
    { r = 255, g = 255, b = 0 },
    { r = 255, g = 0, b = 0 },
    { r = 0, g = 0, b = 0 },
}

-- Pre-rendered gradient strip (100 samples matching the original 100x1 QImage)
local GRADIENT_SAMPLES = 100
local gradient_r = {}
local gradient_g = {}
local gradient_b = {}

---------------------------------------------------------------------------
-- Hex color parsing
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

---------------------------------------------------------------------------
-- Gradient rendering — replicates the Qt QLinearGradient with 4 stops
-- rasterised onto a 100x1 QImage.
--
-- The gradient coordinate space matches QLinearGradient(0,0 → 100,0).
-- Pixel p (0-99) has its centre at x = p + 0.5, so the normalised
-- gradient position is (p + 0.5) / 100.  PadSpread clamps positions
-- outside [0, 1] to the nearest stop colour.
---------------------------------------------------------------------------
local function lerp_channel(a, b, t)
    return math_floor(a + (b - a) * t + 0.5)
end

local function rebuild_gradient(first_stop, second_stop)
    -- Build the four stops: {position, color}
    -- Positions must be non-decreasing.  Clamp to avoid degenerate values.
    local s0 = 0
    local s1 = math_max(0, math_min(1, first_stop))
    local s2 = math_max(s1, math_min(1, second_stop))
    local s3 = 1

    local stops_pos = { s0, s1, s2, s3 }
    local stops_col = { user_colors[1], user_colors[2], user_colors[3], user_colors[4] }

    for p = 0, GRADIENT_SAMPLES - 1 do
        -- Normalised gradient coordinate at pixel centre
        local t = (p + 0.5) / GRADIENT_SAMPLES

        -- PadSpread: clamp to [0, 1]
        if t < 0 then t = 0 end
        if t > 1 then t = 1 end

        -- Find bracketing stops
        local left_idx = 1
        for i = 1, 3 do
            if t >= stops_pos[i] then
                left_idx = i
            end
        end
        local right_idx = left_idx + 1
        if right_idx > 4 then right_idx = 4 end

        local range = stops_pos[right_idx] - stops_pos[left_idx]
        local blend = (range > 1e-9) and ((t - stops_pos[left_idx]) / range) or 0

        local c1 = stops_col[left_idx]
        local c2 = stops_col[right_idx]

        gradient_r[p] = lerp_channel(c1.r, c2.r, blend)
        gradient_g[p] = lerp_channel(c1.g, c2.g, blend)
        gradient_b[p] = lerp_channel(c1.b, c2.b, blend)
    end
end

---------------------------------------------------------------------------
-- Plugin callbacks
---------------------------------------------------------------------------
function plugin.on_init()
    -- no-op
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.speed) == "number" then
        speed = math_max(1, math_min(20, math_floor(p.speed + 0.5)))
    end

    if type(p.intensity) == "number" then
        max_intensity = math_max(1, math_min(99, math_floor(p.intensity + 0.5)))
    end

    if type(p.intensity_speed) == "number" then
        intensity_speed = math_max(1, math_min(100, math_floor(p.intensity_speed + 0.5)))
    end

    if type(p.radius) == "number" then
        radius = math_max(1, math_min(100, math_floor(p.radius + 0.5)))
    end

    if type(p.grow_speed) == "number" then
        grow_speed = math_max(1, math_min(50, math_floor(p.grow_speed + 0.5)))
    end

    if type(p.run_once) == "boolean" then
        run_once = p.run_once
    end

    if type(p.motion) == "boolean" then
        motion = p.motion
    end

    if type(p.colors) == "table" then
        for i = 1, 4 do
            if p.colors[i] then
                local parsed = parse_hex_color(p.colors[i])
                if parsed then
                    user_colors[i] = parsed
                end
            end
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

    ---------------------------------------------------------------------------
    -- Time / progress computation
    -- Original: time += 0.1 * Speed / FPS   (per frame, FPS = 60)
    -- Per second that equals 0.1 * Speed.  Our `t` is cumulative elapsed
    -- seconds, so time_val = 0.1 * speed * t gives the same trajectory.
    ---------------------------------------------------------------------------
    local time_val = 0.1 * speed * t

    local progress, y_shift
    if run_once then
        progress = math_min(1, time_val)
        y_shift = 0
    else
        progress = 0.5 * (1 + math_sin(time_val))
        y_shift = -1 + 2 * progress
    end

    ---------------------------------------------------------------------------
    -- Build the per-frame gradient
    ---------------------------------------------------------------------------
    local first_stop = math_min(0.01 * max_intensity, progress ^ (0.1 * intensity_speed))
    local second_stop = first_stop + (1 - first_stop) * 0.5

    rebuild_gradient(first_stop, second_stop)

    ---------------------------------------------------------------------------
    -- Compute real radius (grows with progress)
    ---------------------------------------------------------------------------
    local real_radius = 0.01 * radius * width * (progress ^ (0.1 * grow_speed))

    local hx = 0.5 * (width - 1)
    local hy = 0.5 * (height - 1)

    -- Cache gradient arrays in locals for inner-loop speed
    local gr = gradient_r
    local gg = gradient_g
    local gb = gradient_b

    local led = 1
    for y = 0, height - 1 do
        local dy = y + (motion and (hy * y_shift) or 0) - hy

        for x = 0, width - 1 do
            if led > n then
                return
            end

            local dx = x - hx
            local distance = math_sqrt(dx * dx + dy * dy)

            -- Normalised distance → gradient position
            local percent
            if real_radius > 1e-9 then
                percent = distance / real_radius
            else
                -- Radius effectively zero → everything is outer colour
                percent = 1
            end
            if percent < 0 then percent = 0 end
            if percent > 1 then percent = 1 end

            -- Sample the pre-rendered gradient
            -- Original: pixelColor(floor(99 * percent), 0)
            local pixel = math_floor(99 * percent)
            if pixel < 0 then pixel = 0 end
            if pixel > 99 then pixel = 99 end

            buffer:set(led, gr[pixel], gg[pixel], gb[pixel])
            led = led + 1
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
