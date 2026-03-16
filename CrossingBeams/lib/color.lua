local M = {}

local math_floor = math.floor
local math_max = math.max
local math_min = math.min

function M.clamp(v, lo, hi)
  if v < lo then return lo end
  if v > hi then return hi end
  return v
end

--- Screen blend: 1 - (1-a)*(1-b), per channel (0-255 integer inputs/output)
function M.screen_blend(a, b)
  local af = a / 255.0
  local bf = b / 255.0
  return math_floor((1.0 - (1.0 - af) * (1.0 - bf)) * 255.0)
end

--- Convert RGB (0-255) to HSV (h: 0-360, s: 0-1, v: 0-1)
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

--- Convert HSV (h: 0-360, s: 0-1, v: 0-1) to RGB (0-255)
function M.hsv_to_rgb(h, s, v)
  h = h % 360
  local c = v * s
  local x = c * (1 - math.abs((h / 60) % 2 - 1))
  local m = v - c
  local r1, g1, b1
  if h < 60 then
    r1, g1, b1 = c, x, 0
  elseif h < 120 then
    r1, g1, b1 = x, c, 0
  elseif h < 180 then
    r1, g1, b1 = 0, c, x
  elseif h < 240 then
    r1, g1, b1 = 0, x, c
  elseif h < 300 then
    r1, g1, b1 = x, 0, c
  else
    r1, g1, b1 = c, 0, x
  end
  return math_floor((r1 + m) * 255 + 0.5),
         math_floor((g1 + m) * 255 + 0.5),
         math_floor((b1 + m) * 255 + 0.5)
end

--- Parse hex color string "#RRGGBB" to r, g, b (0-255)
function M.hex_to_rgb(hex)
  if type(hex) ~= "string" then return 255, 0, 0 end
  hex = hex:gsub("^#", "")
  if #hex ~= 6 then return 255, 0, 0 end
  local r = tonumber(hex:sub(1, 2), 16) or 255
  local g = tonumber(hex:sub(3, 4), 16) or 0
  local b = tonumber(hex:sub(5, 6), 16) or 0
  return r, g, b
end

return M
