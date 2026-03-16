local plugin = {}

local math_floor = math.floor
local math_abs = math.abs
local math_max = math.max
local math_random = math.random

----------------------------------------------------------------------------
-- Parameters (mirror reference defaults)
--   Speed : 1–20, default 10   (reference: MinSpeed=1, MaxSpeed=20, SetSpeed(10))
--   Direction : 0=Horizontal, 1=Vertical  (matrix only)
--   Random : boolean
--   Color  : hex string
----------------------------------------------------------------------------
local speed = 10
local direction = 0
local random_enabled = false
local user_r, user_g, user_b = 255, 0, 0

----------------------------------------------------------------------------
-- Internal state  (one set per plugin instance = one zone)
--   stop          – current target index; the light head travels from 0 toward stop
--   progress      – float position of the moving light along the axis
--   zone_color    – active fill color for the current cycle
--   effective_count – number of positions along the active axis
----------------------------------------------------------------------------
local seeded = false
local last_elapsed = nil
local stop = 1
local progress = 0.0
local zone_color_r, zone_color_g, zone_color_b = 255, 0, 0
local effective_count = 0
local prev_width, prev_height = 0, 0
local needs_reset = false

----------------------------------------------------------------------------
-- Helpers
----------------------------------------------------------------------------
local function parse_hex_color(value)
    if type(value) ~= "string" then return nil end
    local hex = value:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then hex = hex:sub(2) end
    if #hex == 3 then
        hex = hex:sub(1, 1):rep(2)
            .. hex:sub(2, 2):rep(2)
            .. hex:sub(3, 3):rep(2)
    end
    if #hex ~= 6 or hex:find("[^%x]") then return nil end
    return tonumber(hex:sub(1, 2), 16) or 255,
        tonumber(hex:sub(3, 4), 16) or 0,
        tonumber(hex:sub(5, 6), 16) or 0
end

local function pick_zone_color()
    if random_enabled then
        zone_color_r, zone_color_g, zone_color_b =
            host.hsv_to_rgb(math_random() * 360.0, 1.0, 1.0)
    else
        zone_color_r, zone_color_g, zone_color_b = user_r, user_g, user_b
    end
end

local function reset_state(count)
    effective_count = count
    stop = math_max(count - 1, 1)
    progress = 0.0
    pick_zone_color()
end

--- Reference: ColorUtils::Enlight(color, factor) – multiply each channel by factor.
local function scale_channel(ch, factor)
    if factor <= 0.0 then return 0 end
    if factor >= 1.0 then return ch end
    return math_floor(ch * factor + 0.5)
end

--- Matches C++ Stack::GetColor(controller_zone_idx, led_idx).
--- led_idx is 0-based along the active axis.
local function get_color(led_idx)
    -- LEDs past the stop are already "stacked" with the zone colour.
    if stop < led_idx then
        return zone_color_r, zone_color_g, zone_color_b
    end

    local distance = math_abs(progress - led_idx)

    if distance > 1.0 then
        return 0, 0, 0
    end

    -- Fade proportional to distance from the moving head.
    local factor = 1.0 - distance
    return scale_channel(zone_color_r, factor),
        scale_channel(zone_color_g, factor),
        scale_channel(zone_color_b, factor)
end

----------------------------------------------------------------------------
-- Lifecycle
----------------------------------------------------------------------------
function plugin.on_init()
    if not seeded then
        math.randomseed(math_floor(os.clock() * 1000000))
        seeded = true
    end
    last_elapsed = nil
    progress = 0.0
    stop = 1
    effective_count = 0
    prev_width = 0
    prev_height = 0
    needs_reset = false
    pick_zone_color()
end

function plugin.on_params(p)
    if type(p) ~= "table" then return end

    if type(p.speed) == "number" then
        speed = p.speed
    end

    if type(p.direction) == "number" then
        if p.direction ~= direction then
            direction = p.direction
            needs_reset = true -- mirror C++ on_direction_currentIndexChanged
        end
    end

    if type(p.random) == "boolean" then
        random_enabled = p.random
    end

    if type(p.color) == "string" then
        local r, g, b = parse_hex_color(p.color)
        if r then
            user_r, user_g, user_b = r, g, b
        end
    end
end

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    -- Normalise dimensions.
    if type(width) ~= "number" or width <= 0 then width = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    local is_matrix = (height > 1)

    -- Determine the effective LED count along the active axis.
    local count
    if is_matrix then
        if direction == 0 then
            count = width   -- horizontal: columns are the axis
        else
            count = height  -- vertical: rows are the axis
        end
    else
        count = width       -- linear strip
    end

    -- Re-initialise when dimensions change or a reset was requested.
    if width ~= prev_width or height ~= prev_height or needs_reset or effective_count ~= count then
        prev_width = width
        prev_height = height
        needs_reset = false
        reset_state(count)
    end

    -- Compute delta-time.
    local dt = 0.0
    if type(t) == "number" and t >= 0 then
        if last_elapsed ~= nil and t >= last_elapsed then
            dt = t - last_elapsed
        end
        last_elapsed = t
    end

    --------------------------------------------------------------------------
    -- 1.  R E N D E R   (matches reference StepEffect rendering loop)
    --------------------------------------------------------------------------
    if is_matrix then
        if direction == 0 then
            -- Horizontal: one colour per column, same across all rows.
            local led = 1
            for row = 0, height - 1 do
                for col = 0, width - 1 do
                    if led > n then return end
                    local r, g, b = get_color(col)
                    buffer:set(led, r, g, b)
                    led = led + 1
                end
            end
        else
            -- Vertical: one colour per row, same across all columns.
            local led = 1
            for row = 0, height - 1 do
                for col = 0, width - 1 do
                    if led > n then return end
                    local r, g, b = get_color(row)
                    buffer:set(led, r, g, b)
                    led = led + 1
                end
            end
        end
    else
        -- Linear strip.
        for i = 0, n - 1 do
            local r, g, b = get_color(i)
            buffer:set(i + 1, r, g, b)
        end
    end

    --------------------------------------------------------------------------
    -- 2.  A D V A N C E   progress  (matches reference post-render update)
    --     Reference formula:
    --       delta_progress = 0.1 * Speed / FPS          (per frame)
    --       progress += delta_progress * effective_count
    --     Converted to dt-based:
    --       progress += 0.1 * Speed * effective_count * dt
    --------------------------------------------------------------------------
    if dt > 0 and dt <= 0.5 then
        progress = progress + 0.1 * speed * effective_count * dt

        if progress >= stop then
            stop = stop - 1

            if stop <= 0 then
                -- Whole zone filled → restart with (potentially new) colour.
                reset_state(effective_count)
            else
                progress = 0.0
            end
        end
    end
end

function plugin.on_shutdown()
    last_elapsed = nil
end

return plugin
