local SimplexNoise = require("lib.simplex_noise")

local plugin = {}

local math_floor = math.floor
local math_max = math.max

local REFERENCE_FPS = 60.0
local FRAME_DT = 1.0 / REFERENCE_FPS

local MODE_RAINBOW = 0
local MODE_INVERSE_RAINBOW = 1
local MODE_CUSTOM = 2

local MOTION_UP = 0
local MOTION_DOWN = 1
local MOTION_LEFT = 2
local MOTION_RIGHT = 3

local defaults = {
    speed = 50,
    frequency = 0.12,
    amplitude = 3.9,
    lacunarity = 0.75,
    persistence = 0.5,
    octaves = 2,
    motion = MOTION_UP,
    motion_speed = 0,
    colors_choice = MODE_RAINBOW,
    preset = 0,
}

local presets = {
    {
        name = "lava",
        colors = {
            "#FF5500",
            "#FFC800",
            "#C80000",
        },
    },
    {
        name = "borealis",
        colors = {
            "#14E81E",
            "#00EA8D",
            "#017ED5",
            "#B53DFF",
            "#8D00C4",
        },
    },
    {
        name = "ocean",
        colors = {
            "#00007F",
            "#0000FF",
            "#00FFFF",
            "#00AAFF",
        },
    },
    {
        name = "chemicals",
        colors = {
            "#9346FF",
            "#8868B5",
            "#7AFC94",
            "#29FF48",
            "#4BFF00",
        },
    },
}

local params = {
    speed = defaults.speed,
    frequency = defaults.frequency,
    amplitude = defaults.amplitude,
    lacunarity = defaults.lacunarity,
    persistence = defaults.persistence,
    octaves = defaults.octaves,
    motion = defaults.motion,
    motion_speed = defaults.motion_speed,
    colors_choice = defaults.colors_choice,
    preset = defaults.preset,
}

local noise = nil
local progress = 0.0
local last_time = nil
local time_carry = 0.0
local active_preset = defaults.preset
local custom_colors = {}
local gradient_samples = {}

local function clamp(value, min_value, max_value)
    if value < min_value then
        return min_value
    end
    if value > max_value then
        return max_value
    end
    return value
end

local function clamp_int(value, min_value, max_value)
    if type(value) ~= "number" then
        return nil
    end
    return clamp(math_floor(value + 0.5), min_value, max_value)
end

local function clamp_float(value, min_value, max_value)
    if type(value) ~= "number" then
        return nil
    end
    return clamp(value, min_value, max_value)
end

local function round_byte(value)
    return clamp(math_floor(value + 0.5), 0, 255)
end

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

local function clone_rgb_colors(colors)
    local out = {}
    for i = 1, #colors do
        local color = colors[i]
        out[i] = {
            r = color.r,
            g = color.g,
            b = color.b,
        }
    end
    return out
end

local function hex_list_to_rgb(colors)
    local out = {}
    for i = 1, #colors do
        local parsed = parse_hex_color(colors[i])
        if parsed ~= nil then
            out[#out + 1] = parsed
        end
    end
    return out
end

local function lerp_rgb(a, b, t)
    return {
        r = round_byte(a.r + (b.r - a.r) * t),
        g = round_byte(a.g + (b.g - a.g) * t),
        b = round_byte(a.b + (b.b - a.b) * t),
    }
end

local function sample_gradient(colors, t)
    local count = #colors
    if count <= 0 then
        return { r = 0, g = 0, b = 0 }
    end
    if count == 1 then
        return colors[1]
    end

    if t <= 0.0 then
        return colors[1]
    end

    local step = 1.0 / count
    local last_stop = (count - 1) * step
    if t >= last_stop then
        return colors[count]
    end

    local segment = math_floor(t / step) + 1
    if segment >= count then
        return colors[count]
    end

    local start_t = (segment - 1) * step
    local local_t = (t - start_t) / step
    return lerp_rgb(colors[segment], colors[segment + 1], local_t)
end

local function generate_gradient()
    gradient_samples = {}
    for i = 0, 100 do
        gradient_samples[i + 1] = sample_gradient(custom_colors, i / 100.0)
    end
end

local function apply_preset(index)
    local preset = presets[index + 1] or presets[1]
    active_preset = index
    params.preset = index
    custom_colors = hex_list_to_rgb(preset.colors)
    generate_gradient()
end

local function normalize_custom_colors(raw_colors)
    if type(raw_colors) ~= "table" then
        return false
    end

    local parsed = hex_list_to_rgb(raw_colors)
    if #parsed <= 0 then
        return false
    end

    custom_colors = clone_rgb_colors(parsed)
    generate_gradient()
    return true
end

local function reset_noise()
    noise = SimplexNoise.new(
        params.frequency,
        params.amplitude,
        params.lacunarity,
        params.persistence
    )
end

local function reset_runtime_state()
    progress = 0.0
    last_time = nil
    time_carry = 0.0
end

function plugin.on_init()
    apply_preset(defaults.preset)
    reset_noise()
    reset_runtime_state()
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    local next_speed = clamp_int(p.speed, 1, 100)
    if next_speed ~= nil then
        params.speed = next_speed
    end

    local noise_dirty = false

    local next_frequency = clamp_float(p.frequency, 0.0001, 0.5)
    if next_frequency ~= nil and next_frequency ~= params.frequency then
        params.frequency = next_frequency
        noise_dirty = true
    end

    local next_amplitude = clamp_float(p.amplitude, 0.0001, 5.0)
    if next_amplitude ~= nil and next_amplitude ~= params.amplitude then
        params.amplitude = next_amplitude
        noise_dirty = true
    end

    local next_lacunarity = clamp_float(p.lacunarity, 0.0001, 5.0)
    if next_lacunarity ~= nil and next_lacunarity ~= params.lacunarity then
        params.lacunarity = next_lacunarity
        noise_dirty = true
    end

    local next_persistence = clamp_float(p.persistence, 0.0001, 5.0)
    if next_persistence ~= nil and next_persistence ~= params.persistence then
        params.persistence = next_persistence
        noise_dirty = true
    end

    local next_octaves = clamp_int(p.octaves, 1, 20)
    if next_octaves ~= nil and next_octaves ~= params.octaves then
        params.octaves = next_octaves
        noise_dirty = true
    end

    local next_motion = clamp_int(p.motion, MOTION_UP, MOTION_RIGHT)
    if next_motion ~= nil then
        params.motion = next_motion
    end

    local next_motion_speed = clamp_int(p.motion_speed, 0, 99)
    if next_motion_speed ~= nil then
        params.motion_speed = next_motion_speed
    end

    local next_colors_choice = clamp_int(p.colors_choice, MODE_RAINBOW, MODE_CUSTOM)
    if next_colors_choice ~= nil then
        params.colors_choice = next_colors_choice
    end

    local preset_changed = false
    local next_preset = clamp_int(p.preset, 0, #presets - 1)
    if next_preset ~= nil then
        if next_preset ~= active_preset then
            apply_preset(next_preset)
            preset_changed = true
        else
            params.preset = next_preset
        end
    end

    if not preset_changed then
        normalize_custom_colors(p.colors)
    end

    if noise_dirty or noise == nil then
        reset_noise()
    end
end

function plugin.on_tick(t, buffer, width, height)
    local led_count = buffer:len()
    if led_count <= 0 then
        return
    end

    if type(width) ~= "number" or width <= 0 then
        width = led_count
    end
    if type(height) ~= "number" or height <= 0 then
        height = 1
    end

    if noise == nil then
        reset_noise()
    end

    local x_shift = 0.0
    local y_shift = 0.0
    if params.motion == MOTION_UP then
        y_shift = params.motion_speed * progress
    elseif params.motion == MOTION_DOWN then
        y_shift = params.motion_speed * -progress
    elseif params.motion == MOTION_LEFT then
        x_shift = params.motion_speed * progress
    elseif params.motion == MOTION_RIGHT then
        x_shift = params.motion_speed * -progress
    end

    local led = 1
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if led > led_count then
                return
            end

            local value = noise:fractal(params.octaves, x + x_shift, y + y_shift, progress)
            local frac = (1.0 + value) * 0.5

            if params.colors_choice == MODE_RAINBOW then
                buffer:set_hsv(led, 360.0 * frac, 1.0, 1.0)
            elseif params.colors_choice == MODE_INVERSE_RAINBOW then
                buffer:set_hsv(led, 360.0 - (360.0 * frac), 1.0, 1.0)
            else
                local color_x = math_floor((1.0 - frac) * 100.0)
                color_x = clamp(color_x, 0, 100)
                local color = gradient_samples[color_x + 1] or custom_colors[#custom_colors] or { r = 0, g = 0, b = 0 }
                buffer:set(led, color.r, color.g, color.b)
            end

            led = led + 1
        end
    end

    local dt = FRAME_DT
    if type(t) == "number" and t >= 0.0 then
        if last_time ~= nil then
            dt = math_max(0.0, t - last_time)
        end
        last_time = t
    else
        last_time = nil
    end

    time_carry = time_carry + dt
    while time_carry >= FRAME_DT do
        progress = progress + (0.1 * params.speed / REFERENCE_FPS)
        time_carry = time_carry - FRAME_DT
    end
end

function plugin.on_shutdown()
    -- no-op
end

return plugin
