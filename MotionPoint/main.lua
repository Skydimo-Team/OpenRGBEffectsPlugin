local plugin = {}

local math_sin    = math.sin
local math_abs    = math.abs
local math_floor  = math.floor
local math_random = math.random

---------------------------------------------------------------------------
-- Parameters (defaults map to original C++ MotionPoint)
--   Original Speed 1‑50 default 25.
--   Our slider 1‑100 default 50; mapped_speed = speed / 2.
--   progress_per_second = 0.1 * mapped_speed  (matches original exactly)
---------------------------------------------------------------------------
local speed           = 50
local random_enabled  = false
local user_r, user_g, user_b = 255, 0, 0
local bg_r,   bg_g,   bg_b   = 0,   0,   0

---------------------------------------------------------------------------
-- Internal state
---------------------------------------------------------------------------
local progress  = 0.0
local last_t    = nil

local cur_r, cur_g, cur_b = 255, 0, 0
local was_at_endpoint = false

---------------------------------------------------------------------------
-- Helpers
---------------------------------------------------------------------------
local function parse_hex_color(value)
    if type(value) ~= "string" then return nil end
    local hex = value:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then hex = hex:sub(2) end
    if #hex ~= 6 or hex:find("[^%x]") then return nil end
    return tonumber(hex:sub(1, 2), 16) or 0,
           tonumber(hex:sub(3, 4), 16) or 0,
           tonumber(hex:sub(5, 6), 16) or 0
end

local function random_rgb()
    local h = math_random() * 360.0
    return host.hsv_to_rgb(h, 1.0, 1.0)
end

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

function plugin.on_init()
    progress = 0.0
    last_t   = nil
    cur_r, cur_g, cur_b = user_r, user_g, user_b
    was_at_endpoint = false
    math.randomseed(os.clock() * 1000 + os.time())
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end

    if type(p.speed) == "number" then
        speed = p.speed
    end

    if type(p.random) == "boolean" then
        random_enabled = p.random
        if not random_enabled then
            cur_r, cur_g, cur_b = user_r, user_g, user_b
        end
    end

    if type(p.color) == "string" then
        local r, g, b = parse_hex_color(p.color)
        if r then
            user_r, user_g, user_b = r, g, b
            if not random_enabled then
                cur_r, cur_g, cur_b = r, g, b
            end
        end
    end

    if type(p.background) == "string" then
        local r, g, b = parse_hex_color(p.background)
        if r then
            bg_r, bg_g, bg_b = r, g, b
        end
    end
end

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    if type(width)  ~= "number" or width  <= 0 then width  = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    -- dt from elapsed time
    if last_t == nil then last_t = t end
    local dt = t - last_t
    last_t = t
    if dt < 0 or dt > 0.5 then dt = 0 end

    ---------------------------------------------------------------------------
    -- Sinusoidal position: oscillates smoothly between 0 and 1
    -- Ported verbatim: t = (1 + sin(progress)) / 2
    ---------------------------------------------------------------------------
    local sine_t = (1.0 + math_sin(progress)) / 2.0

    ---------------------------------------------------------------------------
    -- Color selection
    -- Original: random color generated when point reaches endpoints (t≈0 or t≈1)
    -- Between endpoints the last generated color persists.
    ---------------------------------------------------------------------------
    if random_enabled then
        local at_endpoint = (sine_t <= 0.0005 or sine_t >= 0.9995)
        if at_endpoint and not was_at_endpoint then
            cur_r, cur_g, cur_b = random_rgb()
        end
        was_at_endpoint = at_endpoint
    else
        cur_r, cur_g, cur_b = user_r, user_g, user_b
    end

    ---------------------------------------------------------------------------
    -- Render — identical pattern on every row (original ignores Y)
    -- GetColor(w, x, t):
    --   distance = |x − t·(w−1)|
    --   distance > 2 → background
    --   else         → lerp(current, background, distance / 2)
    ---------------------------------------------------------------------------
    local cr, cg, cb    = cur_r, cur_g, cur_b
    local bgr, bgg, bgb = bg_r, bg_g, bg_b
    local led = 1

    for _row = 0, height - 1 do
        local w = width
        if w == 0 then w = 1 end
        local point_pos = sine_t * (w - 1)

        for x = 0, w - 1 do
            if led > n then return end

            local distance = math_abs(x - point_pos)

            if distance > 2 then
                buffer:set(led, bgr, bgg, bgb)
            else
                local factor = distance / 2.0
                local inv    = 1.0 - factor
                buffer:set(led,
                    math_floor(cr * inv + bgr * factor + 0.5),
                    math_floor(cg * inv + bgg * factor + 0.5),
                    math_floor(cb * inv + bgb * factor + 0.5))
            end

            led = led + 1
        end
    end

    ---------------------------------------------------------------------------
    -- Advance phase
    -- Original per-frame: progress += 0.1 * Speed / FPS
    -- ⇒ per second: 0.1 * Speed
    -- Our speed 1‑100 maps to original 0.5‑50 via /2
    ---------------------------------------------------------------------------
    progress = progress + 0.1 * (speed / 2.0) * dt
end

function plugin.on_shutdown()
    last_t = nil
end

return plugin
