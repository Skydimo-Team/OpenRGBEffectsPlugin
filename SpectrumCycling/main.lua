local plugin = {}

local math_floor = math.floor

---------------------------------------------------------------------------
-- Parameters (matching reference defaults)
--   speed:      1–100, default 50   (degrees per second, ref: Speed 1–100)
--   saturation: 0–100, default 100  (percent, ref: 0–255 mapped to 0.0–1.0)
---------------------------------------------------------------------------
local speed      = 50
local saturation = 1.0  -- internal 0.0–1.0

---------------------------------------------------------------------------
-- Lifecycle
---------------------------------------------------------------------------

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
    if type(p.saturation) == "number" then
        saturation = p.saturation / 100.0
    end
end

---------------------------------------------------------------------------
-- Render
--
-- Faithfully reproduces the reference C++ SpectrumCycling effect:
--
--   progress += Speed / FPS          (per frame, accumulates over time)
--   HSVVal.hue        = int(progress) % 360
--   HSVVal.saturation = saturation   (0–255)
--   HSVVal.value      = 255
--   All LEDs ← hsv2rgb(HSVVal)
--
-- Mapped to total-elapsed-time (our framework provides `t` directly):
--   hue = floor(speed * t) % 360
--
-- At default speed=50 → 50°/s → full spectrum cycle ≈ 7.2 seconds
-- This matches the reference exactly: Speed=50, FPS=n → same accumulation.
---------------------------------------------------------------------------

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then
        return
    end

    -- Reference: progress += Speed / FPS  →  over time t: progress = Speed * t
    -- Reference: hue = int(progress) % 360
    local hue = math_floor(speed * t) % 360

    -- Set all LEDs to the same solid color (ref: SetAllZoneLEDs)
    for i = 1, n do
        buffer:set_hsv(i, hue, saturation, 1.0)
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
