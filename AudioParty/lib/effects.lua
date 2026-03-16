local color = require("lib.color")

local M = {}

local math_floor = math.floor
local math_abs   = math.abs
local math_random = math.random
local clamp = color.clamp
local screen_blend = color.screen_blend

-- ============================================================================
-- Triggered visual effects (ported from AudioParty C++ reference)
--
-- 7 effect types:
--   0: white bar moving left-to-right
--   1: white bar moving right-to-left
--   2: white bar moving top-to-bottom
--   3: white bar moving bottom-to-top
--   4: random RGB flash
--   5: random white intensity
--   6: white blink/fade
-- ============================================================================

function M.get_effect_color(idx, progress, pos_x, pos_y, w, h)
  if progress >= 1.0 then
    return 0, 0, 0
  end

  local er, eg, eb = 0, 0, 0

  if idx == 0 then
    -- moving bar LTR
    if math_abs(pos_x - progress * w) <= 1.0 then
      er, eg, eb = 255, 255, 255
    end

  elseif idx == 1 then
    -- moving bar RTL
    if math_abs(pos_x - (w - progress * w)) <= 1.0 then
      er, eg, eb = 255, 255, 255
    end

  elseif idx == 2 then
    -- moving bar top-to-bottom
    if math_abs(pos_y - progress * h) <= 1.0 then
      er, eg, eb = 255, 255, 255
    end

  elseif idx == 3 then
    -- moving bar bottom-to-top
    if math_abs(pos_y - (h - progress * h)) <= 1.0 then
      er, eg, eb = 255, 255, 255
    end

  elseif idx == 4 then
    -- random RGB
    er = math_random(0, 255)
    eg = math_random(0, 255)
    eb = math_random(0, 255)

  elseif idx == 5 then
    -- random white intensity
    local v = math_floor(math_random() * 255 + 0.5)
    er, eg, eb = v, v, v

  elseif idx == 6 then
    -- blink/fade out
    local v = math_floor((1.0 - progress) * 255 + 0.5)
    v = clamp(v, 0, 255)
    er, eg, eb = v, v, v
  end

  return er, eg, eb
end

function M.blend(base_r, base_g, base_b, fx_r, fx_g, fx_b)
  if fx_r == 0 and fx_g == 0 and fx_b == 0 then
    return base_r, base_g, base_b
  end
  return screen_blend(base_r, fx_r),
         screen_blend(base_g, fx_g),
         screen_blend(base_b, fx_b)
end

return M
