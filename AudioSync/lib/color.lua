local M = {}

local math_floor = math.floor
local math_max = math.max
local math_min = math.min

function M.clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

function M.screen_blend(a, b)
  local af = a / 255.0
  local bf = b / 255.0
  return math_floor((1.0 - (1.0 - af) * (1.0 - bf)) * 255.0)
end

function M.lerp_color(r0, g0, b0, r1, g1, b1, t)
  local inv = 1.0 - t
  return math_floor(r0 * inv + r1 * t + 0.5),
         math_floor(g0 * inv + g1 * t + 0.5),
         math_floor(b0 * inv + b1 * t + 0.5)
end

function M.unpack_rgb(packed)
  local r = math_floor(packed / 65536) % 256
  local g = math_floor(packed / 256) % 256
  local b = packed % 256
  return r, g, b
end

function M.rgb_to_hsv(r, g, b)
  local rf = r / 255.0
  local gf = g / 255.0
  local bf = b / 255.0
  local maxc = math_max(rf, gf, bf)
  local minc = math_min(rf, gf, bf)
  local delta = maxc - minc
  local h, s, v
  v = maxc
  if maxc == 0 then
    s = 0
  else
    s = delta / maxc
  end
  if delta == 0 then
    h = 0
  elseif maxc == rf then
    h = 60 * (((gf - bf) / delta) % 6)
  elseif maxc == gf then
    h = 60 * (((bf - rf) / delta) + 2)
  else
    h = 60 * (((rf - gf) / delta) + 4)
  end
  if h < 0 then h = h + 360 end
  return h, s, v
end

function M.fill_black(buffer)
  local n = buffer:len()
  for i = 1, n do
    buffer:set(i, 0, 0, 0)
  end
end

return M
