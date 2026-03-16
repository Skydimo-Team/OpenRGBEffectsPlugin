local plugin = {}

local math_random = math.random
local math_floor  = math.floor
local math_fmod   = math.fmod

-- Per-LED flower state: { hue, speed_mult }
-- hue: float accumulator (wraps mod 360)
-- speed_mult: random 1.0 ~ 2.0
local flowers = {}
local flower_count = 0

-- Parameters
local params = {
    speed      = 50,   -- 1..100, default 50
    saturation = 100   -- 0..100, default 100 (maps to 0.0..1.0)
}

-- Seed the random generator with varying initial value
local seeded = false

--- Initialize a flower list for `count` LEDs
local function reset_flowers(count)
    flowers = {}
    for i = 1, count do
        flowers[i] = {
            hue        = math_random() * 360.0,       -- random initial hue 0~360
            speed_mult = 1.0 + math_random()           -- random speed multiplier 1.0~2.0
        }
    end
    flower_count = count
end

function plugin.on_init()
    if not seeded then
        math.randomseed(os.clock() * 1000000)
        seeded = true
    end
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end
    if type(p.speed) == "number" then
        params.speed = p.speed
    end
    if type(p.saturation) == "number" then
        params.saturation = p.saturation
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

    -- Re-initialize if LED count changed
    if flower_count ~= n then
        reset_flowers(n)
    end

    -- Reference Bloom: delta = Speed / FPS
    -- Speed range was 10..200, default 100, FPS typically 60
    -- Our speed is 1..100 (default 50)
    -- Map so that default 50 gives similar delta to reference 100/60 ≈ 1.67
    -- delta = speed * 0.0333  => at 50: 1.665
    local delta = params.speed * 0.0333

    -- Saturation: 0..100 mapped to 0.0..1.0
    local sat = params.saturation / 100.0

    -- Update all flowers and write to buffer
    local fl = flowers
    local i = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if i > n then
                return
            end

            local f = fl[i]
            -- Advance hue by individual speed
            f.hue = f.hue + f.speed_mult * delta
            -- Wrap hue to 0..360
            if f.hue >= 360.0 then
                f.hue = f.hue - math_floor(f.hue / 360.0) * 360.0
            end

            -- Full brightness (value = 1.0)
            buffer:set_hsv(i, f.hue, sat, 1.0)

            i = i + 1
        end
    end
end

function plugin.on_shutdown()
    flowers = {}
    flower_count = 0
end

return plugin
