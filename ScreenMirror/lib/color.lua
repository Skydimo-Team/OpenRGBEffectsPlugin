local M = {}
local NEUTRAL_KELVIN = 6500

local function clamp(x, lo, hi)
  if x < lo then
    return lo
  end
  if x > hi then
    return hi
  end
  return x
end

local function clamp255(x)
  if x < 0 then
    return 0
  end
  if x > 255 then
    return 255
  end
  return x
end

local function round(x)
  if x >= 0 then
    return math.floor(x + 0.5)
  end
  return math.ceil(x - 0.5)
end

local function kelvin_to_rgb_scales(kelvin)
  local temp = clamp(kelvin or NEUTRAL_KELVIN, 1000, 40000) / 100.0
  local r
  local g
  local b

  if temp <= 66 then
    r = 255
    g = 99.4708025861 * math.log(temp) - 161.1195681661
    if temp <= 19 then
      b = 0
    else
      b = 138.5177312231 * math.log(temp - 10) - 305.0447927307
    end
  else
    r = 329.698727446 * ((temp - 60) ^ -0.1332047592)
    g = 288.1221695283 * ((temp - 60) ^ -0.0755148492)
    b = 255
  end

  return clamp255(r) / 255.0, clamp255(g) / 255.0, clamp255(b) / 255.0
end

local neutral_r, neutral_g, neutral_b = kelvin_to_rgb_scales(NEUTRAL_KELVIN)

local function smooth_channel(prev, target, factor)
  if prev == target then
    return target
  end

  local value = prev + (target - prev) * factor
  local rounded = clamp255(round(value))
  local prev_rounded = clamp255(round(prev))

  if rounded == prev_rounded then
    if target > prev then
      rounded = math.min(255, rounded + 1)
    else
      rounded = math.max(0, rounded - 1)
    end
  end

  if math.abs(target - rounded) <= 0.5 then
    return target
  end

  return rounded
end

function M.unpack_rgb(packed)
  packed = packed or 0
  local r = math.floor(packed / 65536) % 256
  local g = math.floor(packed / 256) % 256
  local b = packed % 256
  return r, g, b
end

function M.pack_rgb(r, g, b)
  r = clamp255(round(r))
  g = clamp255(round(g))
  b = clamp255(round(b))
  return r * 65536 + g * 256 + b
end

function M.apply_saturation(r, g, b, saturation)
  if not saturation or math.abs(saturation - 1.0) <= 0.01 then
    return r, g, b
  end

  local gray = r * 0.299 + g * 0.587 + b * 0.114
  r = gray + (r - gray) * saturation
  g = gray + (g - gray) * saturation
  b = gray + (b - gray) * saturation
  return clamp255(r), clamp255(g), clamp255(b)
end

function M.apply_brightness(r, g, b, brightness)
  if not brightness or math.abs(brightness - 1.0) <= 0.01 then
    return r, g, b
  end
  r = r * brightness
  g = g * brightness
  b = b * brightness
  return clamp255(r), clamp255(g), clamp255(b)
end

function M.apply_gamma(r, g, b, gamma)
  if not gamma or math.abs(gamma - 1.0) <= 0.01 then
    return r, g, b
  end

  local function corr(x)
    local v = x / 255.0
    local out = 255.0 * (v ^ gamma)
    return clamp255(out)
  end

  return corr(r), corr(g), corr(b)
end

function M.color_temperature_gains(kelvin)
  if not kelvin then
    return 1.0, 1.0, 1.0
  end

  local value = clamp(kelvin, 2000, 10000)
  if math.abs(value - NEUTRAL_KELVIN) <= 1 then
    return 1.0, 1.0, 1.0
  end

  local r, g, b = kelvin_to_rgb_scales(value)
  local gain_r = neutral_r > 0 and (r / neutral_r) or 1.0
  local gain_g = neutral_g > 0 and (g / neutral_g) or 1.0
  local gain_b = neutral_b > 0 and (b / neutral_b) or 1.0
  return gain_r, gain_g, gain_b
end

function M.apply_color_temperature(r, g, b, gain_r, gain_g, gain_b)
  gain_r = gain_r or 1.0
  gain_g = gain_g or 1.0
  gain_b = gain_b or 1.0

  if math.abs(gain_r - 1.0) <= 0.001 and math.abs(gain_g - 1.0) <= 0.001 and math.abs(gain_b - 1.0) <= 0.001 then
    return r, g, b
  end

  return clamp255(r * gain_r), clamp255(g * gain_g), clamp255(b * gain_b)
end

function M.apply_rgb_calibration(r, g, b, cal_r, cal_g, cal_b)
  cal_r = cal_r or 1.0
  cal_g = cal_g or 1.0
  cal_b = cal_b or 1.0

  if math.abs(cal_r - 1.0) <= 0.001 and math.abs(cal_g - 1.0) <= 0.001 and math.abs(cal_b - 1.0) <= 0.001 then
    return r, g, b
  end

  return clamp255(r * cal_r), clamp255(g * cal_g), clamp255(b * cal_b)
end

function M.smooth(prev_packed, target_packed, smoothness)
  if not smoothness or smoothness <= 0 then
    return target_packed
  end
  if smoothness >= 100 then
    return prev_packed
  end

  local pr, pg, pb = M.unpack_rgb(prev_packed)
  local tr, tg, tb = M.unpack_rgb(target_packed)
  local factor = (100.0 - smoothness) / 100.0

  local r = smooth_channel(pr, tr, factor)
  local g = smooth_channel(pg, tg, factor)
  local b = smooth_channel(pb, tb, factor)
  return M.pack_rgb(r, g, b)
end

return M

