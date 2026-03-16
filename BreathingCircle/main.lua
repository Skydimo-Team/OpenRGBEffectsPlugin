local plugin = {}

-- Localize hot-path math functions
local PI        = math.pi
local math_floor  = math.floor
local math_sin    = math.sin
local math_sqrt   = math.sqrt
local math_min    = math.min
local math_random = math.random

---------------------------------------------------------------------------
-- Parameters (match manifest defaults)
---------------------------------------------------------------------------
local speed     = 50       -- Animation speed (10-100)
local thickness = 10       -- Ring thickness  (1-20)
local random_enabled = false
local user_r, user_g, user_b = 255, 0, 0  -- default: #FF0000

---------------------------------------------------------------------------
-- Internal state
---------------------------------------------------------------------------
local last_cycle = -1
local rand_r, rand_g, rand_b = 255, 0, 0

---------------------------------------------------------------------------
-- Helpers
---------------------------------------------------------------------------

--- Parse "#RRGGBB" hex string → r, g, b (0-255)
local function hex_to_rgb(hex)
    if type(hex) ~= "string" then return 255, 0, 0 end
    hex = hex:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then hex = hex:sub(2) end
    if #hex == 3 then
        hex = hex:sub(1,1):rep(2) .. hex:sub(2,2):rep(2) .. hex:sub(3,3):rep(2)
    end
    if #hex ~= 6 or hex:find("[^%x]") then return 255, 0, 0 end
    return tonumber(hex:sub(1, 2), 16) or 255,
           tonumber(hex:sub(3, 4), 16) or 0,
           tonumber(hex:sub(5, 6), 16) or 0
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    math.randomseed(os.time())
    -- Initialize first random color
    -- Matches C++ constructor: randomColor = ColorUtils::RandomRGBColor()
    -- RandomRGBColor = random hue, full saturation and value
    rand_r, rand_g, rand_b = host.hsv_to_rgb(math_random() * 360, 1.0, 1.0)
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end
    if type(p.speed) == "number" then
        speed = p.speed
    end
    if type(p.thickness) == "number" then
        thickness = p.thickness
    end
    if type(p.random) == "boolean" then
        random_enabled = p.random
    end
    if type(p.color) == "string" then
        user_r, user_g, user_b = hex_to_rgb(p.color)
    end
end

---------------------------------------------------------------------------
-- Render
--
-- Faithfully reproduces the reference C++ BreathingCircle effect:
--
-- StepEffect (per frame):
--   time     += Speed / FPS
--   progress  = 0.35 * (1 + sin(0.1 * time))       (ring radius: 0 → 0.7)
--   On shrink→grow transition: pick new random color
--
-- GetColor(x, y, w, h):
--   distance = min(1, sqrt((0.5 - x/w)^2 + (0.5 - y/h)^2))
--   Ring test: progress - Slider2Val/(0.5*(w+h)) ≤ distance ≤ progress
--   Inside ring → user/random color; outside → OFF (black)
--
-- Mapped to elapsed time (t in seconds):
--   In C++, time advances by Speed per second  (Speed/FPS * FPS = Speed)
--   So time_val = t * speed gives the same rate
---------------------------------------------------------------------------

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    if type(width) ~= "number" or width <= 0 then width = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    -- Time accumulation: C++ advances `time` by Speed per second
    local time_val = t * speed

    -- Breathing progress: ring radius oscillates between 0 and 0.7
    -- Matches C++: progress = 0.35 * (1 + sin(0.1 * time))
    local theta    = 0.1 * time_val
    local progress = 0.35 * (1 + math_sin(theta))

    -------------------------------------------------------------------
    -- Random color cycling
    -- Pick a new color at each sine minimum (shrink→grow transition).
    -- sin(theta) minimum occurs at theta = 3PI/2 + 2kPI
    -- Cycle index: floor((theta + PI/2) / (2*PI))
    -------------------------------------------------------------------
    local cycle = math_floor((theta + PI / 2) / (2 * PI))
    if cycle ~= last_cycle then
        last_cycle = cycle
        -- Matches C++ ColorUtils::RandomRGBColor():
        -- random hue with full saturation and value
        rand_r, rand_g, rand_b = host.hsv_to_rgb(math_random() * 360, 1.0, 1.0)
    end

    -- Active color
    local cr, cg, cb
    if random_enabled then
        cr, cg, cb = rand_r, rand_g, rand_b
    else
        cr, cg, cb = user_r, user_g, user_b
    end

    -------------------------------------------------------------------
    -- Thickness normalization
    -- C++: Slider2Val / (0.5 * (w + h))  where w = width-1, h = height-1
    -------------------------------------------------------------------
    local w_dim = width - 1
    local h_dim = height - 1
    local avg_dim = w_dim + h_dim
    local thickness_norm
    if avg_dim > 0 then
        thickness_norm = thickness / (0.5 * avg_dim)
    else
        -- Single LED: show color whenever progress > 0
        thickness_norm = 1.0
    end

    -- Inner ring boundary
    local inner_edge = progress - thickness_norm

    -------------------------------------------------------------------
    -- Render each pixel
    -------------------------------------------------------------------
    local led = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if led > n then return end

            -- Normalize coordinates to [0, 1]
            -- Handle edge case: single row/column → center at 0.5
            local nx = (w_dim > 0) and (x / w_dim) or 0.5
            local ny = (h_dim > 0) and (y / h_dim) or 0.5

            -- Euclidean distance from center (0.5, 0.5), capped at 1.0
            -- Matches C++: min(1.0, sqrt(pow(0.5 - x/w, 2) + pow(0.5 - y/h, 2)))
            local dx = 0.5 - nx
            local dy = 0.5 - ny
            local distance = math_min(1.0, math_sqrt(dx * dx + dy * dy))

            -- Ring test: pixel is colored if inner_edge ≤ distance ≤ progress
            -- Matches C++:
            --   if (distance > progress || distance < progress - thickness) → OFF
            --   else → color
            if distance <= progress and distance >= inner_edge then
                buffer:set(led, cr, cg, cb)
            else
                buffer:set(led, 0, 0, 0)
            end

            led = led + 1
        end
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
