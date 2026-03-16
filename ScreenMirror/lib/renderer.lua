local border = require("lib/border")
local color = require("lib/color")

local M = {}

local function clamp(x, lo, hi)
  if x < lo then
    return lo
  end
  if x > hi then
    return hi
  end
  return x
end

local function round(x)
  if x >= 0 then
    return math.floor(x + 0.5)
  end
  return math.ceil(x - 0.5)
end

function M.fill_black(buffer)
  local n = buffer:len()
  for i = 1, n do
    buffer:set(i, 0, 0, 0)
  end
end

function M.apply_params(cfg, p)
  if type(p.smoothness) == "number" then
    cfg.smoothness = clamp(p.smoothness, 0, 100)
  end
  if type(p.brightness) == "number" then
    cfg.brightness = clamp(p.brightness, 0.0, 3.0)
  end
  if type(p.saturation) == "number" then
    cfg.saturation = clamp(p.saturation, 0.0, 3.0)
  end
  if type(p.gamma) == "number" then
    cfg.gamma = clamp(p.gamma, 0.1, 4.0)
  end
  if type(p.blur) == "number" then
    cfg.blur = clamp(p.blur, 0, 50)
  end
  if type(p.autoCrop) == "boolean" then
    cfg.autoCrop = p.autoCrop
  end

  if type(p.bbThreshold) == "number" then
    cfg.bbThreshold = clamp(p.bbThreshold, 0, 100)
  end
  if type(p.bbMode) == "number" then
    cfg.bbMode = round(p.bbMode)
  end
  if type(p.bbBorderFrameCnt) == "number" then
    cfg.bbBorderFrameCnt = round(clamp(p.bbBorderFrameCnt, 0, 9999))
  end
  if type(p.bbUnknownFrameCnt) == "number" then
    cfg.bbUnknownFrameCnt = round(clamp(p.bbUnknownFrameCnt, 0, 9999))
  end
  if type(p.bbMaxInconsistentCnt) == "number" then
    cfg.bbMaxInconsistentCnt = round(clamp(p.bbMaxInconsistentCnt, 0, 9999))
  end
  if type(p.bbBlurRemoveCnt) == "number" then
    cfg.bbBlurRemoveCnt = round(clamp(p.bbBlurRemoveCnt, 0, 9999))
  end
end

local function gaussian_kernel(radius)
  if radius <= 0 then
    return { 1.0 }
  end

  local sigma = radius * 0.5 + 0.5
  local denom = 2.0 * sigma * sigma

  local weights = {}
  local sum = 0.0
  for k = -radius, radius do
    local v = math.exp(-(k * k) / denom)
    table.insert(weights, v)
    sum = sum + v
  end
  if sum > 0 then
    for i = 1, #weights do
      weights[i] = weights[i] / sum
    end
  end
  return weights
end

local function blur_1d(src, len, weights, radius)
  local out = {}
  for x = 0, len - 1 do
    local sum_w = 0.0
    local r_sum, g_sum, b_sum = 0.0, 0.0, 0.0
    for i = 1, #weights do
      local k = (i - 1) - radius
      local xx = x + k
      if xx < 0 then
        xx = 0
      elseif xx > len - 1 then
        xx = len - 1
      end
      local packed = src[xx + 1] or 0
      local r, g, b = color.unpack_rgb(packed)
      local w = weights[i]
      sum_w = sum_w + w
      r_sum = r_sum + r * w
      g_sum = g_sum + g * w
      b_sum = b_sum + b * w
    end
    local inv = sum_w > 0 and (1.0 / sum_w) or 0.0
    out[x + 1] = color.pack_rgb(r_sum * inv, g_sum * inv, b_sum * inv)
  end
  return out
end

local function blur_horizontal(width, height, src, weights, radius)
  local out = {}
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      local sum_w = 0.0
      local r_sum, g_sum, b_sum = 0.0, 0.0, 0.0
      for i = 1, #weights do
        local k = (i - 1) - radius
        local xx = x + k
        if xx < 0 then
          xx = 0
        elseif xx > width - 1 then
          xx = width - 1
        end
        local idx = y * width + xx + 1
        local packed = src[idx] or 0
        local r, g, b = color.unpack_rgb(packed)
        local w = weights[i]
        sum_w = sum_w + w
        r_sum = r_sum + r * w
        g_sum = g_sum + g * w
        b_sum = b_sum + b * w
      end
      local inv = sum_w > 0 and (1.0 / sum_w) or 0.0
      out[y * width + x + 1] = color.pack_rgb(r_sum * inv, g_sum * inv, b_sum * inv)
    end
  end
  return out
end

local function blur_vertical(width, height, src, weights, radius)
  local out = {}
  for y = 0, height - 1 do
    for x = 0, width - 1 do
      local sum_w = 0.0
      local r_sum, g_sum, b_sum = 0.0, 0.0, 0.0
      for i = 1, #weights do
        local k = (i - 1) - radius
        local yy = y + k
        if yy < 0 then
          yy = 0
        elseif yy > height - 1 then
          yy = height - 1
        end
        local idx = yy * width + x + 1
        local packed = src[idx] or 0
        local r, g, b = color.unpack_rgb(packed)
        local w = weights[i]
        sum_w = sum_w + w
        r_sum = r_sum + r * w
        g_sum = g_sum + g * w
        b_sum = b_sum + b * w
      end
      local inv = sum_w > 0 and (1.0 / sum_w) or 0.0
      out[y * width + x + 1] = color.pack_rgb(r_sum * inv, g_sum * inv, b_sum * inv)
    end
  end
  return out
end

local function apply_gaussian_blur(layout_w, layout_h, colors, radius)
  if radius <= 0 then
    return colors
  end

  local width = math.max(layout_w, 1)
  local height = math.max(layout_h, 1)
  local len = math.min(#colors, width * height)
  if len <= 0 then
    return colors
  end

  local weights = gaussian_kernel(radius)

  if height <= 1 then
    return blur_1d(colors, len, weights, radius)
  end

  local horiz = blur_horizontal(width, height, colors, weights, radius)
  return blur_vertical(width, height, horiz, weights, radius)
end

local function sample(frame, ratio_x, ratio_y, crop, cfg)
  local fw = frame.width or 1
  local fh = frame.height or 1
  if fw <= 0 then
    fw = 1
  end
  if fh <= 0 then
    fh = 1
  end

  local crop_left = clamp(crop.left or 0.0, 0.0, 0.45)
  local crop_right = clamp(crop.right or 0.0, 0.0, 0.45)
  local crop_top = clamp(crop.top or 0.0, 0.0, 0.45)
  local crop_bottom = clamp(crop.bottom or 0.0, 0.0, 0.45)

  local roi_w = math.max(0.1, 1.0 - crop_left - crop_right)
  local roi_h = math.max(0.1, 1.0 - crop_top - crop_bottom)

  local rx = clamp(crop_left + clamp(ratio_x, 0.0, 1.0) * roi_w, 0.0, 1.0)
  local ry = clamp(crop_top + clamp(ratio_y, 0.0, 1.0) * roi_h, 0.0, 1.0)

  local x = round((fw - 1) * rx)
  local y = round((fh - 1) * ry)
  if x < 0 then
    x = 0
  elseif x > fw - 1 then
    x = fw - 1
  end
  if y < 0 then
    y = 0
  elseif y > fh - 1 then
    y = fh - 1
  end

  local idx = y * fw + x + 1
  local packed = (frame.pixels or {})[idx] or 0
  local r, g, b = color.unpack_rgb(packed)

  r, g, b = color.apply_saturation(r, g, b, cfg.saturation)
  r, g, b = color.apply_brightness(r, g, b, cfg.brightness)
  r, g, b = color.apply_gamma(r, g, b, cfg.gamma)

  return color.pack_rgb(r, g, b)
end

function M.render(frame, buffer, width, height, state, cfg)
  local n = buffer:len()
  if n <= 0 then
    return
  end

  local prev = state.previous_buffer or {}
  state.previous_buffer = prev

  local bp = state.border_processor
  if not bp then
    bp = border.new()
    state.border_processor = bp
  end

  local crop = { left = 0, right = 0, top = 0, bottom = 0 }
  if cfg.autoCrop then
    bp:set_enabled(true)
    bp:set_threshold_percent(cfg.bbThreshold or 5)
    bp:set_mode(cfg.bbMode or 0)
    bp.border_switch_cnt = cfg.bbBorderFrameCnt or bp.border_switch_cnt
    bp.unknown_switch_cnt = cfg.bbUnknownFrameCnt or bp.unknown_switch_cnt
    bp.max_inconsistent_cnt = cfg.bbMaxInconsistentCnt or bp.max_inconsistent_cnt
    bp.blur_remove_cnt = cfg.bbBlurRemoveCnt or bp.blur_remove_cnt
    bp:process_frame(frame)
    crop = bp:crop_region_for(frame)
  else
    bp:set_enabled(false)
  end

  local colors = {}
  local i = 1
  for y = 0, height - 1 do
    local ry = height == 1 and 0.5 or (y + 0.5) / height
    for x = 0, width - 1 do
      if i > n then
        break
      end
      local rx = width == 1 and 0.5 or (x + 0.5) / width
      local target = sample(frame, rx, ry, crop, cfg)
      local prev_packed = prev[i] or target
      local out = color.smooth(prev_packed, target, cfg.smoothness or 0)
      prev[i] = out
      colors[i] = out
      i = i + 1
    end
  end

  local blur_radius = round(cfg.blur or 0)
  if blur_radius > 0 then
    colors = apply_gaussian_blur(width, height, colors, blur_radius)
    for j = 1, #colors do
      prev[j] = colors[j]
    end
  end

  for j = 1, n do
    local packed = colors[j] or 0
    local r, g, b = color.unpack_rgb(packed)
    buffer:set(j, r, g, b)
  end
end

return M

