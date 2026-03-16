local plugin = {}

local math_floor = math.floor
local math_abs = math.abs
local math_max = math.max

-- Clock mode constants
local MODE_12_HOUR = 0
local MODE_24_HOUR = 1

-- Parameters
local clock_mode = MODE_12_HOUR
local hour_color = { r = 255, g = 0, b = 0 }
local minute_color = { r = 0, g = 255, b = 0 }
local second_color = { r = 0, g = 0, b = 255 }

local function parse_hex_color(value, fallback)
    if type(value) ~= "string" then
        return fallback
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
        return fallback
    end
    return {
        r = tonumber(hex:sub(1, 2), 16) or fallback.r,
        g = tonumber(hex:sub(3, 4), 16) or fallback.g,
        b = tonumber(hex:sub(5, 6), 16) or fallback.b,
    }
end

function plugin.on_init()
    -- no-op
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end
    if type(p.clockMode) == "number" then
        local v = math_floor(p.clockMode + 0.5)
        if v == MODE_12_HOUR or v == MODE_24_HOUR then
            clock_mode = v
        end
    end
    hour_color = parse_hex_color(p.hourColor, hour_color)
    minute_color = parse_hex_color(p.minuteColor, minute_color)
    second_color = parse_hex_color(p.secondColor, second_color)
end

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    if type(width) ~= "number" or width <= 0 then width = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    -- Get current local time with sub-second precision
    local now = os.time()
    local date = os.date("*t", now)

    -- os.clock() gives CPU time, not wall-clock sub-seconds.
    -- Use the fractional part of the elapsed time parameter for smooth animation.
    -- The 't' parameter is a monotonic elapsed time in seconds;
    -- we use its fractional part as millisecond approximation for smooth hand motion.
    local ms_frac = t - math_floor(t)

    local mode = clock_mode == MODE_24_HOUR and 24 or 12

    -- Fractional hand positions (matching reference C++ exactly)
    local s = date.sec + ms_frac              -- 0..~60
    local m = date.min + date.sec / 60.0      -- 0..~60
    local h = (date.hour % mode) + date.min / 60.0  -- 0..mode

    -- Render each row
    local idx = 1
    for y = 0, height - 1 do
        -- Use width of current row for hand mapping
        local w = width

        -- Map hand positions to LED positions along the strip
        local step_h = (w - 1) * h / mode    -- hour hand LED position
        local step_m = (w - 1) * m / 60.0    -- minute hand LED position
        local step_s = (w - 1) * s / 60.0    -- second hand LED position

        for x = 0, w - 1 do
            if idx > n then return end

            local xf = x * 1.0  -- ensure float

            -- Hour hand contribution
            local hr, hg, hb = 0, 0, 0
            local dh = math_abs(xf - step_h)
            if dh <= 1.0 then
                local brightness = 1.0 - dh
                hr = math_floor(hour_color.r * brightness + 0.5)
                hg = math_floor(hour_color.g * brightness + 0.5)
                hb = math_floor(hour_color.b * brightness + 0.5)
            end

            -- Minute hand contribution
            local mr, mg, mb = 0, 0, 0
            local dm = math_abs(xf - step_m)
            if dm <= 1.0 then
                local brightness = 1.0 - dm
                mr = math_floor(minute_color.r * brightness + 0.5)
                mg = math_floor(minute_color.g * brightness + 0.5)
                mb = math_floor(minute_color.b * brightness + 0.5)
            end

            -- Second hand contribution
            local sr, sg, sb = 0, 0, 0
            local ds = math_abs(xf - step_s)
            if ds <= 1.0 then
                local brightness = 1.0 - ds
                sr = math_floor(second_color.r * brightness + 0.5)
                sg = math_floor(second_color.g * brightness + 0.5)
                sb = math_floor(second_color.b * brightness + 0.5)
            end

            -- Additive blend (Lighten = max per channel, matching ColorUtils::Lighten)
            local fr = math_max(hr, math_max(mr, sr))
            local fg = math_max(hg, math_max(mg, sg))
            local fb = math_max(hb, math_max(mb, sb))

            buffer:set(idx, fr, fg, fb)
            idx = idx + 1
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
