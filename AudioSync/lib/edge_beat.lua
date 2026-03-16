local color = require("lib.color")

local M = {}

local clamp = color.clamp
local screen_blend = color.screen_blend

function M.apply(r, g, b, bins, config)
  local bassAmp = (tonumber(bins[1]) or 0.0) + (tonumber(bins[9]) or 0.0)
  local edgeValue = 0.01 * (tonumber(config.edgeBeatSensitivity) or 100.0) * bassAmp
  edgeValue = clamp(edgeValue, 0.0, 1.0)

  local eh = tonumber(config.edgeBeatHue) or 0.0
  local es = (tonumber(config.edgeBeatSaturation) or 0.0) / 255.0
  local er, eg, eb = host.hsv_to_rgb(eh % 360.0, es, edgeValue)

  return screen_blend(r, er), screen_blend(g, eg), screen_blend(b, eb)
end

return M
