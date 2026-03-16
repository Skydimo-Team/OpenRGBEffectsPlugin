local plugin = {}

-- Localize hot-path math functions
local math_floor = math.floor
local math_sqrt  = math.sqrt
local math_abs   = math.abs
local math_min   = math.min
local math_max   = math.max
local math_random = math.random

----------------------------------------------------------------------------
-- Color helpers
----------------------------------------------------------------------------

--- Parse "#RRGGBB" hex string to r, g, b (0-255)
local function hex_to_rgb(hex)
    if type(hex) ~= "string" then return 255, 255, 255 end
    hex = hex:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then hex = hex:sub(2) end
    if #hex == 3 then
        hex = hex:sub(1,1):rep(2) .. hex:sub(2,2):rep(2) .. hex:sub(3,3):rep(2)
    end
    if #hex ~= 6 then return 255, 255, 255 end
    return tonumber(hex:sub(1,2), 16) or 255,
           tonumber(hex:sub(3,4), 16) or 255,
           tonumber(hex:sub(5,6), 16) or 255
end

--- RGB (0-255) -> HSV  h:[0,360)  s:[0,1]  v:[0,1]
local function rgb_to_hsv(r, g, b)
    local rf, gf, bf = r / 255, g / 255, b / 255
    local maxc = math_max(rf, gf, bf)
    local minc = math_min(rf, gf, bf)
    local delta = maxc - minc
    local h, s, v = 0, 0, maxc

    if delta > 0 then
        if maxc == rf then
            h = 60 * (((gf - bf) / delta) % 6)
        elseif maxc == gf then
            h = 60 * (((bf - rf) / delta) + 2)
        else
            h = 60 * (((rf - gf) / delta) + 4)
        end
        s = delta / maxc
    end
    if h < 0 then h = h + 360 end
    return h, s, v
end

--- HSV  h:[0,360)  s:[0,1]  v:[0,1]  -> RGB (0-255)
local function hsv_to_rgb(h, s, v)
    local c = v * s
    local x = c * (1 - math_abs((h / 60) % 2 - 1))
    local m = v - c
    local r1, g1, b1 = 0, 0, 0

    if h < 60 then        r1, g1, b1 = c, x, 0
    elseif h < 120 then   r1, g1, b1 = x, c, 0
    elseif h < 180 then   r1, g1, b1 = 0, c, x
    elseif h < 240 then   r1, g1, b1 = 0, x, c
    elseif h < 300 then   r1, g1, b1 = x, 0, c
    else                   r1, g1, b1 = c, 0, x
    end

    return math_floor((r1 + m) * 255 + 0.5),
           math_floor((g1 + m) * 255 + 0.5),
           math_floor((b1 + m) * 255 + 0.5)
end

--- Screen blend (per-channel): 1 - (1-a)*(1-b),  inputs/output 0-255
local function screen_ch(a, b)
    local af = a / 255
    local bf = b / 255
    return math_floor((1 - (1 - af) * (1 - bf)) * 255 + 0.5)
end

--- Screen blend two RGB triplets
local function screen_blend(r1, g1, b1, r2, g2, b2)
    return screen_ch(r1, r2), screen_ch(g1, g2), screen_ch(b1, b2)
end

----------------------------------------------------------------------------
-- Config (matches manifest defaults)
----------------------------------------------------------------------------

local config = {
    max_bubbles = 10,
    rarity      = 50,
    speed       = 1,
    max_expansion = 100,
    thickness   = 10,
    bg_r        = 0,
    bg_g        = 0,
    bg_b        = 0,
}

-- User palette (parsed RGB tables)
local palette = {
    { r = 255, g = 0,   b = 0   },
    { r = 255, g = 153, b = 0   },
    { r = 255, g = 255, b = 0   },
    { r = 0,   g = 255, b = 136 },
    { r = 0,   g = 170, b = 255 },
}

----------------------------------------------------------------------------
-- Bubble state arrays (parallel arrays, same style as C++ original)
----------------------------------------------------------------------------

local bubble_expansion = {}   -- current expansion radius
local bubble_speed     = {}   -- per-bubble random speed factor [1..11]
local bubble_color     = {}   -- index into palette (or direct {r,g,b})
local bubble_cx        = {}   -- center x [0..1]
local bubble_cy        = {}   -- center y [0..1]
local bubble_count     = 0

-- Reference FPS from the original effect
local FPS = 60

-- We track the previous tick time so we can accumulate per-frame steps
local prev_time = nil

----------------------------------------------------------------------------
-- Bubble management
----------------------------------------------------------------------------

local function init_bubble()
    bubble_count = bubble_count + 1
    local idx = bubble_count

    bubble_expansion[idx] = 0
    -- Random speed in [1, 11] (original: 1 + 10 * rand01)
    bubble_speed[idx] = 1 + 10 * math_random()
    -- Pick a random color from the palette
    local pc = palette[math_random(1, #palette)]
    bubble_color[idx] = { r = pc.r, g = pc.g, b = pc.b }
    -- Random center in [0,1] x [0,1]
    bubble_cx[idx] = math_random()
    bubble_cy[idx] = math_random()
end

local function cleanup_bubbles()
    local i = 1
    while i <= bubble_count do
        if bubble_expansion[i] > config.max_expansion then
            -- Swap-remove with last element for O(1) delete
            local last = bubble_count
            bubble_expansion[i] = bubble_expansion[last]
            bubble_speed[i]     = bubble_speed[last]
            bubble_color[i]     = bubble_color[last]
            bubble_cx[i]        = bubble_cx[last]
            bubble_cy[i]        = bubble_cy[last]

            bubble_expansion[last] = nil
            bubble_speed[last]     = nil
            bubble_color[last]     = nil
            bubble_cx[last]        = nil
            bubble_cy[last]        = nil

            bubble_count = bubble_count - 1
            -- Don't increment i; re-check the swapped element
        else
            i = i + 1
        end
    end
end

----------------------------------------------------------------------------
-- Plugin callbacks
----------------------------------------------------------------------------

function plugin.on_init()
    bubble_count = 0
    prev_time = nil
    math.randomseed(os.time())
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end

    if type(p.max_bubbles) == "number" then
        config.max_bubbles = math_floor(p.max_bubbles + 0.5)
    end
    if type(p.rarity) == "number" then
        config.rarity = math_max(1, math_floor(p.rarity + 0.5))
    end
    if type(p.speed) == "number" then
        config.speed = p.speed
    end
    if type(p.max_expansion) == "number" then
        config.max_expansion = math_floor(p.max_expansion + 0.5)
    end
    if type(p.thickness) == "number" then
        config.thickness = math_max(1, math_floor(p.thickness + 0.5))
    end
    if type(p.background) == "string" then
        config.bg_r, config.bg_g, config.bg_b = hex_to_rgb(p.background)
    end
    if type(p.colors) == "table" then
        local new_palette = {}
        for i = 1, #p.colors do
            local r, g, b = hex_to_rgb(p.colors[i])
            new_palette[#new_palette + 1] = { r = r, g = g, b = b }
        end
        if #new_palette > 0 then
            palette = new_palette
        end
    end
end

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    if type(width) ~= "number" or width <= 0 then width = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    -- Compute dt (time since last frame).
    -- Original C++ advances bubbles by: 0.2 * speed_mult * speeds[i] / FPS  per frame
    -- We replicate this using elapsed dt * FPS to get the same rate.
    local dt
    if prev_time == nil then
        dt = 1 / FPS
    else
        dt = t - prev_time
        if dt <= 0 then dt = 1 / FPS end
    end
    prev_time = t

    -- Number of "frames" equivalent in this dt
    local frame_steps = dt * FPS

    -- Advance each bubble's expansion (matches original per-frame formula)
    for i = 1, bubble_count do
        bubble_expansion[i] = bubble_expansion[i]
            + 0.2 * config.speed * bubble_speed[i] / FPS * frame_steps
    end

    -- Spawn new bubbles based on rarity probability
    -- Original: each frame, if rand() % rarity == 0 => probability 1/rarity per frame
    -- We simulate multiple frame spawns for frame_steps
    local spawn_rolls = math_max(1, math_floor(frame_steps + 0.5))
    for _ = 1, spawn_rolls do
        if bubble_count < config.max_bubbles then
            if math_random(1, config.rarity) == 1 then
                init_bubble()
            end
        end
    end

    -- Remove expired bubbles
    cleanup_bubbles()

    -- Cache config locals for inner loop
    local thickness = config.thickness
    local bg_r, bg_g, bg_b = config.bg_r, config.bg_g, config.bg_b
    local bc = bubble_count

    -- Render each pixel
    local led = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then return end

            local best_val = 0
            local best_idx = 0

            -- For each bubble, compute ring brightness at this pixel
            for i = 1, bc do
                -- Euclidean distance from pixel to bubble center (in pixel coords)
                local dx = width  * bubble_cx[i] - x
                local dy = height * bubble_cy[i] - y
                local distance = math_sqrt(dx * dx + dy * dy)

                -- Ring function: how close is this distance to the expansion radius?
                local shallow = math_abs(distance - bubble_expansion[i])
                                / (0.1 * thickness)

                -- Inverse-square glow, capped at 255
                local value
                if shallow < 0.001 then
                    value = 255
                else
                    value = math_min(255, 255 / (shallow * shallow))
                end

                if value > best_val then
                    best_val = value
                    best_idx = i
                end
            end

            if best_idx > 0 then
                -- Get the bubble's base color, convert to HSV, override V, back to RGB
                local col = bubble_color[best_idx]
                local bh, bs, _ = rgb_to_hsv(col.r, col.g, col.b)

                -- Original sets hsv.value = val (0-255 range), so normalize to [0,1]
                local bv = best_val / 255

                local cr, cg, cb = hsv_to_rgb(bh, bs, bv)

                -- Screen blend with background
                local fr, fg, fb = screen_blend(cr, cg, cb, bg_r, bg_g, bg_b)
                buffer:set(led, fr, fg, fb)
            else
                buffer:set(led, bg_r, bg_g, bg_b)
            end

            led = led + 1
        end
    end
end

function plugin.on_shutdown()
    bubble_count = 0
    bubble_expansion = {}
    bubble_speed = {}
    bubble_color = {}
    bubble_cx = {}
    bubble_cy = {}
    prev_time = nil
end

return plugin
