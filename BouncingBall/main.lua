local plugin = {}

local math_sqrt   = math.sqrt
local math_floor  = math.floor
local math_abs    = math.abs
local math_random = math.random

---------------------------------------------------------------------------
-- Gravity formula: value <= 10 → linear;  value > 10 → 10 + 1.07^value
-- Ported verbatim from BouncingBallSimulation::GetGravity()
---------------------------------------------------------------------------
local function get_gravity(value)
    if value <= 10 then
        return value
    else
        return 10 + (1.07 ^ value)
    end
end

---------------------------------------------------------------------------
-- Parameters  (defaults match reference C++ exactly)
---------------------------------------------------------------------------
local param_radius        = 15
local param_gravity_raw   = 10
local param_h_velocity    = 10
local param_spectrum_vel  = 10
local param_drop_pct      = 90

---------------------------------------------------------------------------
-- Simulation state
---------------------------------------------------------------------------
local x,  y  = 0, 0       -- ball position
local dx, dy = 0, 0       -- velocity
local ddy    = 0           -- gravity acceleration
local impact_velocity = 0

local sim_width  = 0       -- LED matrix / strip dimensions
local sim_height = 0
local is_matrix  = false
local hue_degrees = 0.0

local prev_t = nil         -- for computing per-frame dt

-- Precomputed ball mask: list of {rx, ry, br}
-- br = brightness 0..1, linear falloff from center
local ball_points = {}


---------------------------------------------------------------------------
-- Precompute circle offsets + per-pixel brightness
-- Ported from BouncingBallSimulation::calculatePointsInCircle()
---------------------------------------------------------------------------
local function calculate_ball_points(radius)
    ball_points = {}
    local r = math_floor(radius)
    if r < 1 then r = 1 end
    local r2 = r * r
    for ry = -r, r do
        for rx = -r, r do
            if rx * rx + ry * ry <= r2 then
                local dist = math_sqrt(rx * rx + ry * ry)
                local br = (1.0 - dist / r)
                ball_points[#ball_points + 1] = { rx = rx, ry = ry, br = br }
            end
        end
    end
end

---------------------------------------------------------------------------
-- (Re)initialise simulation
-- Ported from BouncingBallSimulation::initSimulation()
---------------------------------------------------------------------------
local function init_simulation()
    if sim_height <= 0 then return end

    -- drop height in LEDs
    local drop_h = param_drop_pct / 100.0 * (sim_height - 1)

    -- v = sqrt(2·g·h)
    impact_velocity = math_sqrt(2 * ddy * drop_h)

    -- Ball starts at top, at the drop height offset
    -- Reference: y = height - dropHeight  (y=0 is top, y=height-1 is bottom)
    y  = sim_height - drop_h
    dy = 0

    -- Random X position (matrix only; linear → stays 0)
    if sim_width > 1 then
        x = math_random(0, sim_width - 1)
    else
        x = 0
    end

    -- Random initial horizontal direction
    local speed = math_abs(dx)
    if speed == 0 then speed = param_h_velocity end
    dx = speed * (math_random(0, 1) == 0 and -1 or 1)

    prev_t = nil  -- reset dt tracking so first frame after reinit is skipped
end

---------------------------------------------------------------------------
-- Lifecycle callbacks
---------------------------------------------------------------------------

function plugin.on_init()
    math.randomseed(os.clock() * 1000 + os.time())
    ddy = get_gravity(param_gravity_raw)
    dx  = param_h_velocity
    calculate_ball_points(param_radius)
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end

    if type(p.radius) == "number" then
        param_radius = p.radius
        calculate_ball_points(param_radius)
    end

    local need_reinit = false

    if type(p.gravity) == "number" then
        param_gravity_raw = p.gravity
        ddy = get_gravity(param_gravity_raw)
        need_reinit = true
    end

    if type(p.dropHeight) == "number" then
        param_drop_pct = p.dropHeight
        need_reinit = true
    end

    if type(p.horizontalVelocity) == "number" then
        param_h_velocity = p.horizontalVelocity
        -- Preserve current movement direction
        local sign = dx < 0 and -1 or 1
        dx = param_h_velocity * sign
    end

    if type(p.spectrumVelocity) == "number" then
        param_spectrum_vel = p.spectrumVelocity
    end

    if need_reinit then
        init_simulation()
    end
end

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    -- Dimension convention from runtime.rs:
    --   matrix  → (matrix_width, matrix_height)
    --   linear  → (led_count, 1)
    -- For linear strips we simulate as a vertical column (w=1, h=led_count).
    if type(width) ~= "number" or width <= 0 then width = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    local cur_is_matrix = (height > 1)
    local eff_w, eff_h
    if cur_is_matrix then
        eff_w = width
        eff_h = height
    else
        -- Linear strip: simulate as vertical column
        eff_w = 1
        eff_h = width  -- width holds led_count for linear strips
    end

    -- Detect dimension changes → re-init
    if eff_w ~= sim_width or eff_h ~= sim_height or cur_is_matrix ~= is_matrix then
        sim_width  = eff_w
        sim_height = eff_h
        is_matrix  = cur_is_matrix
        init_simulation()
    end

    -- Compute per-frame dt from total elapsed time
    if prev_t == nil then
        prev_t = t
        return  -- skip first frame (no dt yet)
    end
    local dt = t - prev_t
    prev_t = t
    if dt <= 0 or dt > 0.5 then
        return  -- skip degenerate frames
    end

    -------------------------------------------------------------------
    -- 1) Clear entire buffer to black, then draw ball
    -------------------------------------------------------------------
    for i = 1, n do
        buffer:set(i, 0, 0, 0)
    end

    local hsv_to_rgb = host.hsv_to_rgb
    local base_hue = hue_degrees

    for pi = 1, #ball_points do
        local pt = ball_points[pi]
        local sx = math_floor(x + pt.rx)
        local sy = math_floor(y + pt.ry)

        -- Bounds check in simulation domain
        if sx >= 0 and sy >= 0 and sx < sim_width and sy < sim_height then
            local led_idx
            if is_matrix then
                -- Row-major: matches the runtime buffer layout
                led_idx = sy * sim_width + sx + 1
            else
                -- Linear strip: only y matters
                led_idx = sy + 1
            end

            if led_idx >= 1 and led_idx <= n then
                local r, g, b = hsv_to_rgb(base_hue, 1.0, pt.br)
                buffer:set(led_idx, r, g, b)
            end
        end
    end

    -------------------------------------------------------------------
    -- 2) Horizontal physics (matrix only)
    -- Ported from BouncingBallSimulation::StepEffect()
    -------------------------------------------------------------------
    if is_matrix and sim_width > 1 then
        local dxp = dx
        local xp  = x + dxp * dt

        if xp < 0 then
            local pct = xp / (xp - x)
            dx = -dxp
            x  = dx * dt * pct
        elseif xp >= sim_width then
            local overshoot = xp - sim_width - 1
            local pct = overshoot / (xp - x)
            dx = -dxp
            x  = sim_width - 1 + dx * dt * pct
        else
            x = x + dx * dt
        end
    end

    -------------------------------------------------------------------
    -- 3) Vertical physics (gravity → ball falls downward → +y)
    -- Ported from BouncingBallSimulation::StepEffect()
    -------------------------------------------------------------------
    local dyp = dy + ddy * dt
    local yp  = y + dyp * dt

    if yp >= sim_height then
        local overshoot = yp - sim_height - 1
        local pct = overshoot / (yp - y)
        dy = -impact_velocity + ddy * dt * pct
        y  = sim_height - 1 + dy * dt * pct
    else
        dy = dy + ddy * dt
        y  = y  + dy * dt
    end

    -------------------------------------------------------------------
    -- 4) Color cycling
    -------------------------------------------------------------------
    hue_degrees = hue_degrees + param_spectrum_vel * dt
    if hue_degrees >= 360 then
        hue_degrees = hue_degrees - 360
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
