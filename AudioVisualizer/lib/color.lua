-- color utility module for audio_visualizer
local M = {}

local math_floor = math.floor
local math_max   = math.max
local math_min   = math.min

function M.clamp(v, lo, hi)
    if v < lo then return lo end
    if v > hi then return hi end
    return v
end

--- Unpack 0xRRGGBB integer into r, g, b (0-255)
function M.unpack_rgb(c)
    local r = math_floor(c / 65536) % 256
    local g = math_floor(c / 256) % 256
    local b = c % 256
    return r, g, b
end

--- Pack r, g, b (0-255) into 0xRRGGBB integer
function M.pack_rgb(r, g, b)
    return math_floor(r) * 65536 + math_floor(g) * 256 + math_floor(b)
end

--- Scale a color by brightness factor (0-255 range bright)
function M.scale_color(r, g, b, bright)
    return math_floor(bright * r / 256),
           math_floor(bright * g / 256),
           math_floor(bright * b / 256)
end

--- Scale a color by float factor (0.0-1.0)
function M.scale_f(r, g, b, f)
    f = M.clamp(f, 0.0, 1.0)
    return math_floor(f * r + 0.5),
           math_floor(f * g + 0.5),
           math_floor(f * b + 0.5)
end

--- HSV to RGB. h: 0-360, s: 0-255, v: 0-255. Returns r,g,b (0-255).
--- Matches the OpenRGB hsv2rgb function behavior.
function M.hsv_to_rgb(h, s, v)
    h = h % 360
    if s == 0 then return v, v, v end

    local region = math_floor(h / 60)
    local remainder = (h - (region * 60)) * 6

    local p = math_floor((v * (255 - s)) / 255)
    local q = math_floor((v * (255 - (s * remainder) / 360)) / 255)
    local t = math_floor((v * (255 - (s * (360 - remainder)) / 360)) / 255)

    if region == 0 then     return v, t, p
    elseif region == 1 then return q, v, p
    elseif region == 2 then return p, v, t
    elseif region == 3 then return p, q, v
    elseif region == 4 then return t, p, v
    else                    return v, p, q
    end
end

function M.fill_black(buffer)
    local n = buffer:len()
    for i = 1, n do
        buffer:set(i, 0, 0, 0)
    end
end

return M
