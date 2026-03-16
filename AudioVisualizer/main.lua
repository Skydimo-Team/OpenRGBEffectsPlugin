--[[
  Audio Visualizer — Lua port of OpenRGBEffectsPlugin AudioVisualizer
  Original: Adam Honse (calcprogrammer1@gmail.com), modded by CoffeeIsLife
  Port: Skydimo

  Implements a 256×64 virtual pixel grid with:
    - Configurable background and foreground patterns (26 modes each)
    - Spectrograph visualization (FFT bins → vertical bars)
    - Bar graph on row 0 (amplitude-based)
    - Single-color row 1 (for single-LED devices)
    - Reactive/silent background modes
    - Mapping to linear / matrix / single-LED devices
]]

local color = require("lib.color")

local plugin = {}

---------------------------------------------------------------------------
-- Constants
---------------------------------------------------------------------------
local VIS_W = 256
local VIS_H = 64

local ROW_BAR_GRAPH     = 0   -- y index for bar graph
local ROW_SINGLE_COLOR  = 1   -- y index for single-color devices
local ROW_SPECTRO_TOP   = 2   -- first spectrograph row

-- Pattern enum values (must match manifest select options)
local PAT_SOLID_BLACK                     = 0
local PAT_SOLID_WHITE                     = 1
local PAT_SOLID_RED                       = 2
local PAT_SOLID_ORANGE                    = 3
local PAT_SOLID_YELLOW                    = 4
local PAT_SOLID_GREEN                     = 5
local PAT_SOLID_CYAN                      = 6
local PAT_SOLID_BLUE                      = 7
local PAT_SOLID_PURPLE                    = 8
local PAT_SOLID_ELECTRIC_AQUAMARINE       = 9
local PAT_STATIC_RED_BLUE                 = 10
local PAT_STATIC_CYAN_ORANGE             = 11
local PAT_STATIC_CYAN_PURPLE            = 12
local PAT_STATIC_CYAN_ELECTRIC_AQUAMARINE = 13
local PAT_STATIC_GREEN_YELLOW_RED        = 14
local PAT_STATIC_GREEN_WHITE_RED         = 15
local PAT_STATIC_BLUE_CYAN_WHITE         = 16
local PAT_STATIC_RED_WHITE_BLUE          = 17
local PAT_STATIC_RAINBOW                 = 18
local PAT_STATIC_RAINBOW_INVERSE         = 19
local PAT_ANIM_RAINBOW_SINUSOIDAL       = 20
local PAT_ANIM_RAINBOW_HSV              = 21
local PAT_ANIM_COLOR_WHEEL              = 22
local PAT_ANIM_COLOR_WHEEL_2            = 23
local PAT_ANIM_SPECTRUM_CYCLE           = 24
local PAT_ANIM_SINUSOIDAL_CYCLE         = 25

-- Single-color mode enum
local SC_BLACK              = 0
local SC_WHITE              = 1
local SC_RED                = 2
local SC_ORANGE             = 3
local SC_YELLOW             = 4
local SC_GREEN              = 5
local SC_CYAN               = 6
local SC_BLUE               = 7
local SC_PURPLE             = 8
local SC_ELECTRIC_AQUAMARINE = 9
local SC_BACKGROUND         = 10
local SC_FOLLOW_BACKGROUND  = 11
local SC_FOLLOW_FOREGROUND  = 12

-- Color constants (0xRRGGBB) matching OpenRGB Colors.h
local C_BLACK   = 0x000000
local C_WHITE   = 0xFFFFFF
local C_RED     = 0xFF0000
local C_ORANGE  = 0xFFA500
local C_YELLOW  = 0xFFFF00
local C_LIME    = 0x00FF00
local C_CYAN    = 0x00FFFF
local C_BLUE    = 0x0000FF
local C_PURPLE  = 0x800080
local C_ELEC_UL = 0x4000FF  -- Electric Ultramarine

-- Single color static presets  (sc_mode → 0xRRGGBB)
local SC_COLORS = {
    [SC_BLACK]              = C_BLACK,
    [SC_WHITE]              = C_WHITE,
    [SC_RED]                = C_RED,
    [SC_ORANGE]             = C_ORANGE,
    [SC_YELLOW]             = C_YELLOW,
    [SC_GREEN]              = C_LIME,
    [SC_CYAN]               = C_CYAN,
    [SC_BLUE]               = C_BLUE,
    [SC_PURPLE]             = C_PURPLE,
    [SC_ELECTRIC_AQUAMARINE]= C_ELEC_UL,
}

---------------------------------------------------------------------------
-- Config (updated via on_params)
---------------------------------------------------------------------------
local cfg = {
    bg_mode           = PAT_ANIM_RAINBOW_SINUSOIDAL,
    fg_mode           = PAT_STATIC_GREEN_YELLOW_RED,
    single_color_mode = SC_FOLLOW_FOREGROUND,
    bg_brightness     = 100,
    anim_speed        = 100,
    bg_timeout        = 120,
    reactive_bg       = false,
    silent_bg         = false,
    avg_size          = 8,
}

---------------------------------------------------------------------------
-- State
---------------------------------------------------------------------------
local bkgd_step       = 0.0
local background_timer = 0.0
local FPS             = 60

-- Virtual pixel grids: pixels_bg, pixels_fg, pixels_render
-- Stored as flat arrays [y * VIS_W + x + 1] = {r, g, b}
-- Using separate r/g/b arrays for performance
local px_bg_r, px_bg_g, px_bg_b = {}, {}, {}
local px_fg_r, px_fg_g, px_fg_b = {}, {}, {}
local px_out_r, px_out_g, px_out_b = {}, {}, {}

local function px_idx(x, y) return y * VIS_W + x + 1 end

-- Preallocate pixel arrays
local total_px = VIS_W * VIS_H
for i = 1, total_px do
    px_bg_r[i] = 0; px_bg_g[i] = 0; px_bg_b[i] = 0
    px_fg_r[i] = 0; px_fg_g[i] = 0; px_fg_b[i] = 0
    px_out_r[i] = 0; px_out_g[i] = 0; px_out_b[i] = 0
end

---------------------------------------------------------------------------
-- Upvalues for hot path
---------------------------------------------------------------------------
local math_floor = math.floor
local math_sin   = math.sin
local math_sqrt  = math.sqrt
local math_atan2 = math.atan
local math_abs   = math.abs
local math_max   = math.max
local math_min   = math.min
local PI         = math.pi
local unpack_rgb = color.unpack_rgb
local hsv_to_rgb = color.hsv_to_rgb
local clamp      = color.clamp

-- atan2 polyfill (Lua 5.4 uses math.atan with two args)
local atan2 = math.atan

---------------------------------------------------------------------------
-- Drawing functions — write into target r/g/b arrays
---------------------------------------------------------------------------

local function draw_solid_color(bright, col, tr, tg, tb)
    bright = math_floor(bright * (255.0 / 100.0))
    local cr, cg, cb = unpack_rgb(col)
    local sr = math_floor(bright * cr / 256)
    local sg = math_floor(bright * cg / 256)
    local sb = math_floor(bright * cb / 256)
    for i = 1, total_px do
        tr[i] = sr; tg[i] = sg; tb[i] = sb
    end
end

local function draw_spectrum_cycle(bright, step, tr, tg, tb)
    bright = math_floor(bright * (255.0 / 100.0))
    local h = math_floor(step) % 360
    local r, g, b = hsv_to_rgb(h, 255, bright)
    for i = 1, total_px do
        tr[i] = r; tg[i] = g; tb[i] = b
    end
end

local function draw_sinusoidal_cycle(bright, step, tr, tg, tb)
    bright = math_floor(bright * (255.0 / 100.0))
    local base = ((math_floor(360.0 / 255.0 - step) % 360) / 360.0) * 2 * PI
    local red = math_floor(127 * (math_sin(base) + 1))
    local grn = math_floor(127 * (math_sin(base - 6.28 / 3) + 1))
    local blu = math_floor(127 * (math_sin(base + 6.28 / 3) + 1))
    local sr = math_floor(bright * red / 256)
    local sg = math_floor(bright * grn / 256)
    local sb = math_floor(bright * blu / 256)
    for i = 1, total_px do
        tr[i] = sr; tg[i] = sg; tb[i] = sb
    end
end

local function draw_rainbow(bright, step, tr, tg, tb)
    bright = math_floor(bright * (255.0 / 100.0))
    for x = 0, VIS_W - 1 do
        local h = (math_floor(step + (256 - x)) % 360)
        local r, g, b = hsv_to_rgb(h, 255, bright)
        for y = 0, VIS_H - 1 do
            local idx = px_idx(x, y)
            tr[idx] = r; tg[idx] = g; tb[idx] = b
        end
    end
end

local function draw_rainbow_sinusoidal(bright, step, tr, tg, tb)
    bright = math_floor(bright * (255.0 / 100.0))
    for x = 0, VIS_W - 1 do
        local base = ((math_floor(x * (360.0 / 255.0) - step) % 360) / 360.0) * 2 * PI
        local red = math_floor(127 * (math_sin(base) + 1))
        local grn = math_floor(127 * (math_sin(base - 6.28 / 3) + 1))
        local blu = math_floor(127 * (math_sin(base + 6.28 / 3) + 1))
        local sr = math_floor(bright * red / 256)
        local sg = math_floor(bright * grn / 256)
        local sb = math_floor(bright * blu / 256)
        for y = 0, VIS_H - 1 do
            local idx = px_idx(x, y)
            tr[idx] = sr; tg[idx] = sg; tb[idx] = sb
        end
    end
end

local function draw_color_wheel(bright, step, cx, cy, tr, tg, tb)
    bright = math_floor(bright * (255.0 / 100.0))
    for x = 0, VIS_W - 1 do
        for y = 0, VIS_H - 1 do
            local hue = step + (180 + atan2(y - cy, x - cx) * (180.0 / PI)) % 360
            local h = math_floor(hue) % 360
            local r, g, b = hsv_to_rgb(h, 255, bright)
            local idx = px_idx(x, y)
            tr[idx] = r; tg[idx] = g; tb[idx] = b
        end
    end
end

local function draw_horizontal_bars(bright, colors, num_colors, tr, tg, tb)
    bright = math_floor(bright * (255.0 / 100.0))
    for x = 0, VIS_W - 1 do
        for y = 0, VIS_H - 1 do
            local idx_px = px_idx(x, y)
            local ci, cr, cg, cb
            if y == ROW_BAR_GRAPH then
                if x < 128 then
                    ci = num_colors - math_floor(x * (num_colors / 128.0))
                    if ci >= num_colors then ci = num_colors - 1 end
                else
                    ci = math_floor((x - 128) * (num_colors / 128.0))
                end
            else
                ci = num_colors - math_floor(y * (num_colors / 63.0))
            end
            ci = clamp(ci, 0, num_colors - 1)
            cr, cg, cb = unpack_rgb(colors[ci + 1])
            tr[idx_px] = math_floor(bright * cr / 256)
            tg[idx_px] = math_floor(bright * cg / 256)
            tb[idx_px] = math_floor(bright * cb / 256)
        end
    end
end

local function draw_pattern(pattern, bright, step, tr, tg, tb)
    if pattern == PAT_SOLID_BLACK then
        draw_solid_color(bright, C_BLACK, tr, tg, tb)
    elseif pattern == PAT_SOLID_WHITE then
        draw_solid_color(bright, C_WHITE, tr, tg, tb)
    elseif pattern == PAT_SOLID_RED then
        draw_solid_color(bright, C_RED, tr, tg, tb)
    elseif pattern == PAT_SOLID_ORANGE then
        draw_solid_color(bright, C_ORANGE, tr, tg, tb)
    elseif pattern == PAT_SOLID_YELLOW then
        draw_solid_color(bright, C_YELLOW, tr, tg, tb)
    elseif pattern == PAT_SOLID_GREEN then
        draw_solid_color(bright, C_LIME, tr, tg, tb)
    elseif pattern == PAT_SOLID_CYAN then
        draw_solid_color(bright, C_CYAN, tr, tg, tb)
    elseif pattern == PAT_SOLID_BLUE then
        draw_solid_color(bright, C_BLUE, tr, tg, tb)
    elseif pattern == PAT_SOLID_PURPLE then
        draw_solid_color(bright, C_PURPLE, tr, tg, tb)
    elseif pattern == PAT_SOLID_ELECTRIC_AQUAMARINE then
        draw_solid_color(bright, C_ELEC_UL, tr, tg, tb)
    elseif pattern == PAT_STATIC_RED_BLUE then
        draw_horizontal_bars(bright, { C_RED, C_BLUE }, 2, tr, tg, tb)
    elseif pattern == PAT_STATIC_CYAN_ORANGE then
        draw_horizontal_bars(bright, { C_CYAN, C_ORANGE }, 2, tr, tg, tb)
    elseif pattern == PAT_STATIC_CYAN_PURPLE then
        draw_horizontal_bars(bright, { C_CYAN, C_PURPLE }, 2, tr, tg, tb)
    elseif pattern == PAT_STATIC_CYAN_ELECTRIC_AQUAMARINE then
        draw_horizontal_bars(bright, { C_CYAN, C_ELEC_UL }, 2, tr, tg, tb)
    elseif pattern == PAT_STATIC_GREEN_YELLOW_RED then
        draw_horizontal_bars(bright, { C_LIME, C_YELLOW, C_RED }, 3, tr, tg, tb)
    elseif pattern == PAT_STATIC_GREEN_WHITE_RED then
        draw_horizontal_bars(bright, { C_LIME, C_WHITE, C_RED }, 3, tr, tg, tb)
    elseif pattern == PAT_STATIC_BLUE_CYAN_WHITE then
        draw_horizontal_bars(bright, { C_BLUE, C_CYAN, C_WHITE }, 3, tr, tg, tb)
    elseif pattern == PAT_STATIC_RED_WHITE_BLUE then
        draw_horizontal_bars(bright, { C_RED, C_WHITE, C_BLUE }, 3, tr, tg, tb)
    elseif pattern == PAT_STATIC_RAINBOW then
        draw_horizontal_bars(bright, { C_RED, C_YELLOW, C_LIME, C_CYAN, C_BLUE, C_PURPLE }, 6, tr, tg, tb)
    elseif pattern == PAT_STATIC_RAINBOW_INVERSE then
        draw_horizontal_bars(bright, { C_PURPLE, C_BLUE, C_CYAN, C_LIME, C_YELLOW, C_RED }, 6, tr, tg, tb)
    elseif pattern == PAT_ANIM_RAINBOW_SINUSOIDAL then
        draw_rainbow_sinusoidal(bright, step, tr, tg, tb)
    elseif pattern == PAT_ANIM_RAINBOW_HSV then
        draw_rainbow(bright, step, tr, tg, tb)
    elseif pattern == PAT_ANIM_COLOR_WHEEL then
        draw_color_wheel(bright, step, 128, 32, tr, tg, tb)
    elseif pattern == PAT_ANIM_COLOR_WHEEL_2 then
        draw_color_wheel(bright, step, 128, 64, tr, tg, tb)
    elseif pattern == PAT_ANIM_SPECTRUM_CYCLE then
        draw_spectrum_cycle(bright, step, tr, tg, tb)
    elseif pattern == PAT_ANIM_SINUSOIDAL_CYCLE then
        draw_sinusoidal_cycle(bright, step, tr, tg, tb)
    end
end

---------------------------------------------------------------------------
-- Single-color drawing (for ROW_SINGLE_COLOR)
---------------------------------------------------------------------------

local function draw_single_color_static(amplitude, col)
    amplitude = clamp(amplitude, 0.0, 1.0)
    local cr, cg, cb = unpack_rgb(col)
    local or_ = math_floor(amplitude * cr)
    local og  = math_floor(amplitude * cg)
    local ob  = math_floor(amplitude * cb)
    for x = 0, VIS_W - 1 do
        local idx = px_idx(x, ROW_SINGLE_COLOR)
        px_out_r[idx] = or_; px_out_g[idx] = og; px_out_b[idx] = ob
    end
end

local function draw_single_color_foreground(amplitude)
    amplitude = clamp(amplitude, 0.0, 1.0)
    local y_idx = clamp(math_floor(64.0 - amplitude * 62.0), 0, VIS_H - 1)
    local in_idx = px_idx(0, y_idx)
    local ir, ig, ib = px_fg_r[in_idx], px_fg_g[in_idx], px_fg_b[in_idx]
    local or_ = math_floor(amplitude * ir)
    local og  = math_floor(amplitude * ig)
    local ob  = math_floor(amplitude * ib)
    for x = 0, VIS_W - 1 do
        local out_idx = px_idx(x, ROW_SINGLE_COLOR)
        if cfg.fg_mode >= PAT_ANIM_RAINBOW_SINUSOIDAL then
            local fg_idx = px_idx(x, ROW_SINGLE_COLOR)
            ir = px_fg_r[fg_idx]; ig = px_fg_g[fg_idx]; ib = px_fg_b[fg_idx]
            or_ = math_floor(amplitude * ir)
            og  = math_floor(amplitude * ig)
            ob  = math_floor(amplitude * ib)
        end
        px_out_r[out_idx] = or_; px_out_g[out_idx] = og; px_out_b[out_idx] = ob
    end
end

local function draw_single_color_background(amplitude)
    amplitude = clamp(amplitude, 0.0, 1.0)
    for x = 0, VIS_W - 1 do
        local bg_idx  = px_idx(x, ROW_SINGLE_COLOR)
        local out_idx = px_idx(x, ROW_SINGLE_COLOR)
        px_out_r[out_idx] = math_floor(amplitude * px_bg_r[bg_idx])
        px_out_g[out_idx] = math_floor(amplitude * px_bg_g[bg_idx])
        px_out_b[out_idx] = math_floor(amplitude * px_bg_b[bg_idx])
    end
end

---------------------------------------------------------------------------
-- Index mapping for zone → virtual pixel grid
---------------------------------------------------------------------------

local function setup_linear_grid(x_count)
    local x_idx = {}
    if (x_count % 2) == 0 then
        for x = 0, x_count - 1 do
            x_idx[x] = math_floor(x * (256.0 / x_count) + (128.0 / x_count))
        end
    else
        for x = 0, x_count - 1 do
            if x == math_floor(x_count / 2) then
                x_idx[x] = 128
            elseif x < math_floor(x_count / 2) + 1 then
                x_idx[x] = math_floor(x_count / 2) + (x + 1) * math_floor(256 / (x_count + 1))
            else
                x_idx[x] = math_floor(x_count / 2) + 1 + x * math_floor(256 / (x_count + 1))
            end
        end
    end
    return x_idx
end

local function setup_matrix_x_grid(x_count)
    local COLS = 256
    local x_idx = {}
    for x = 0, x_count - 1 do
        if x_count < 10 then
            x_idx[x] = math_floor(x * (COLS / x_count) + 0.5 * (COLS / x_count))
        elseif x < math_floor(x_count / 2) then
            x_idx[x] = math_floor(x * (COLS / (x_count - 1)) + 0.5 * (COLS / (x_count - 1)))
        else
            x_idx[x] = math_floor(x * (COLS / (x_count - 1)) - 0.5 * (COLS / (x_count - 1)))
        end
        x_idx[x] = clamp(x_idx[x], 0, COLS - 1)
    end
    return x_idx
end

local function setup_matrix_y_grid(y_count)
    local SPECTRO_ROWS = VIS_H - ROW_SPECTRO_TOP
    local y_idx = {}
    for y = 0, y_count - 1 do
        y_idx[y] = math_floor(ROW_SPECTRO_TOP + y * (SPECTRO_ROWS / y_count) + 0.5 * (SPECTRO_ROWS / y_count))
        y_idx[y] = clamp(y_idx[y], 0, VIS_H - 1)
    end
    return y_idx
end

-- Cache for index maps
local cached_linear_map = nil
local cached_linear_len = 0
local cached_matrix_x   = nil
local cached_matrix_xlen = 0
local cached_matrix_y   = nil
local cached_matrix_ylen = 0

---------------------------------------------------------------------------
-- Plugin lifecycle
---------------------------------------------------------------------------

function plugin.on_init() end

function plugin.on_params(p)
    if type(p) ~= "table" then return end
    for k, v in pairs(p) do
        if cfg[k] ~= nil then
            cfg[k] = v
        end
    end
    -- reactive_bg and silent_bg are mutually exclusive (match C++ behavior)
    if cfg.reactive_bg and cfg.silent_bg then
        cfg.silent_bg = false
    end
end

function plugin.on_tick(elapsed, buffer, width, height)
    local n = buffer:len()
    if n <= 0 then return end

    -- Default dimensions
    if type(width)  ~= "number" or width  <= 0 then width  = n end
    if type(height) ~= "number" or height <= 0 then height = 1 end

    -- Audio capture
    if not audio or type(audio.capture) ~= "function" then
        color.fill_black(buffer)
        return
    end

    local avg_sz = math_floor(tonumber(cfg.avg_size) or 8)
    avg_sz = clamp(avg_sz, 1, 256)
    local frame = audio.capture(avg_sz)
    if not frame or type(frame) ~= "table" or type(frame.bins) ~= "table" then
        color.fill_black(buffer)
        return
    end

    local bins = frame.bins  -- 256 floats (1-indexed)

    -----------------------------------------------------------------------
    -- Overflow background step
    -----------------------------------------------------------------------
    if bkgd_step >= 360.0 then bkgd_step = 0.0 end
    if bkgd_step < 0.0 then bkgd_step = 360.0 end

    -----------------------------------------------------------------------
    -- Draw background & foreground patterns
    -----------------------------------------------------------------------
    draw_pattern(cfg.bg_mode, cfg.bg_brightness, bkgd_step, px_bg_r, px_bg_g, px_bg_b)
    draw_pattern(cfg.fg_mode, 100, bkgd_step, px_fg_r, px_fg_g, px_fg_b)

    -----------------------------------------------------------------------
    -- Base brightness from a low-frequency bin (index 5, 0-based → bins[6])
    -----------------------------------------------------------------------
    local brightness = tonumber(bins[6]) or 0.0

    -----------------------------------------------------------------------
    -- Background timer & shutdown fade logic
    -----------------------------------------------------------------------
    background_timer = background_timer + 60.0 / FPS
    local bg_timeout = tonumber(cfg.bg_timeout) or 120

    -- Check if audio is silent: if any bin has signal, reset timer
    if bg_timeout > 0 then
        for i = 0, 127 do
            if (tonumber(bins[2 * i + 1]) or 0) >= 0.0001 then
                background_timer = 0
                break
            end
        end
        if background_timer >= bg_timeout then
            if background_timer >= 3 * bg_timeout then
                background_timer = 3 * bg_timeout
            end
            brightness = (background_timer - bg_timeout) / (2.0 * bg_timeout)
        end
    end

    -----------------------------------------------------------------------
    -- Compose spectrograph: 256×64 pixel grid
    -----------------------------------------------------------------------
    local reactive_bg = cfg.reactive_bg
    local silent_bg   = cfg.silent_bg

    for x = 0, VIS_W - 1 do
        local bin_val = tonumber(bins[x + 1]) or 0.0

        for y = 0, VIS_H - 1 do
            local idx = px_idx(x, y)

            -- Spectrograph foreground: bin amplitude > threshold for this row
            if bin_val > ((1.0 / 64.0) * (64.0 - y)) then
                px_out_r[idx] = px_fg_r[idx]
                px_out_g[idx] = px_fg_g[idx]
                px_out_b[idx] = px_fg_b[idx]
            else
                -- Background
                if reactive_bg or silent_bg then
                    if (not silent_bg) or (background_timer >= bg_timeout and bg_timeout > 0) then
                        px_out_r[idx] = math_floor(brightness * px_bg_r[idx])
                        px_out_g[idx] = math_floor(brightness * px_bg_g[idx])
                        px_out_b[idx] = math_floor(brightness * px_bg_b[idx])
                    else
                        px_out_r[idx] = 0; px_out_g[idx] = 0; px_out_b[idx] = 0
                    end
                else
                    px_out_r[idx] = px_bg_r[idx]
                    px_out_g[idx] = px_bg_g[idx]
                    px_out_b[idx] = px_bg_b[idx]
                end
            end

            -- Bar graph override on row 0
            if y == ROW_BAR_GRAPH then
                local amp5 = (tonumber(bins[6]) or 0.0) - 0.05
                local bar_active
                if x < 128 then
                    bar_active = amp5 > ((1.0 / 128.0) * (127 - x))
                else
                    bar_active = amp5 > ((1.0 / 128.0) * (x - 128))
                end
                if bar_active then
                    px_out_r[idx] = px_fg_r[idx]
                    px_out_g[idx] = px_fg_g[idx]
                    px_out_b[idx] = px_fg_b[idx]
                else
                    if reactive_bg or silent_bg then
                        if (not silent_bg) or (background_timer >= bg_timeout and bg_timeout > 0) then
                            px_out_r[idx] = math_floor(brightness * px_bg_r[idx])
                            px_out_g[idx] = math_floor(brightness * px_bg_g[idx])
                            px_out_b[idx] = math_floor(brightness * px_bg_b[idx])
                        else
                            px_out_r[idx] = 0; px_out_g[idx] = 0; px_out_b[idx] = 0
                        end
                    else
                        px_out_r[idx] = px_bg_r[idx]
                        px_out_g[idx] = px_bg_g[idx]
                        px_out_b[idx] = px_bg_b[idx]
                    end
                end
            end
        end
    end

    -----------------------------------------------------------------------
    -- Single-color row for single-LED devices
    -----------------------------------------------------------------------
    local sc_mode = cfg.single_color_mode
    local sc_brightness = brightness

    if sc_mode == SC_FOLLOW_BACKGROUND then
        sc_brightness = (cfg.bg_brightness / 100.0) * brightness
    end

    if bg_timeout <= 0 or background_timer < bg_timeout then
        local sc_color = SC_COLORS[sc_mode]
        if sc_color then
            draw_single_color_static(sc_brightness, sc_color)
        elseif sc_mode == SC_BACKGROUND then
            -- leave background unmodified (do nothing)
        elseif sc_mode == SC_FOLLOW_BACKGROUND then
            draw_single_color_background(sc_brightness)
        elseif sc_mode == SC_FOLLOW_FOREGROUND then
            draw_single_color_foreground(sc_brightness)
        end
    end

    -----------------------------------------------------------------------
    -- Increment background animation step
    -----------------------------------------------------------------------
    bkgd_step = bkgd_step + ((cfg.anim_speed * 100) / (100.0 * FPS))

    -----------------------------------------------------------------------
    -- Map virtual pixel grid to actual LED buffer
    -----------------------------------------------------------------------
    local is_matrix = (height > 1 and width > 1)
    local is_single = (width == 1 and height == 1) or n == 1

    if is_matrix then
        -- Matrix mapping
        if cached_matrix_xlen ~= width then
            cached_matrix_x = setup_matrix_x_grid(width)
            cached_matrix_xlen = width
        end
        if cached_matrix_ylen ~= height then
            cached_matrix_y = setup_matrix_y_grid(height)
            cached_matrix_ylen = height
        end
        local xi = cached_matrix_x
        local yi = cached_matrix_y
        local led = 1
        for y = 0, height - 1 do
            for x = 0, width - 1 do
                if led > n then break end
                local px_i = px_idx(xi[x], yi[y])
                buffer:set(led, px_out_r[px_i], px_out_g[px_i], px_out_b[px_i])
                led = led + 1
            end
            if led > n then break end
        end
    elseif is_single then
        -- Single-LED: use single-color row, position 0
        local px_i = px_idx(0, ROW_SINGLE_COLOR)
        for i = 1, n do
            buffer:set(i, px_out_r[px_i], px_out_g[px_i], px_out_b[px_i])
        end
    else
        -- Linear strip mapping: use bar graph row (row 0)
        if cached_linear_len ~= n then
            cached_linear_map = setup_linear_grid(n)
            cached_linear_len = n
        end
        local xi = cached_linear_map
        for x = 0, n - 1 do
            local px_i = px_idx(xi[x], ROW_BAR_GRAPH)
            buffer:set(x + 1, px_out_r[px_i], px_out_g[px_i], px_out_b[px_i])
        end
    end
end

function plugin.on_shutdown() end

return plugin
