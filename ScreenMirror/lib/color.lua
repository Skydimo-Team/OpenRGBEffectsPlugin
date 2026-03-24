local M = {}

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

