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

local function is_black(packed, threshold)
  local r, g, b = color.unpack_rgb(packed)
  return r < threshold and g < threshold and b < threshold
end

local function frame_is_all_black(frame, threshold)
  local pixels = frame.pixels
  for i = 1, #pixels do
    if not is_black(pixels[i], threshold) then
      return false
    end
  end
  return true
end

local function scan_border_sizes(frame, threshold)
  local w = frame.width or 1
  local h = frame.height or 1
  local pixels = frame.pixels or {}

  if w <= 0 or h <= 0 or #pixels <= 0 then
    return { unknown = true, top = 0, bottom = 0, left = 0, right = 0 }
  end

  if frame_is_all_black(frame, threshold) then
    return { unknown = true, top = 0, bottom = 0, left = 0, right = 0 }
  end

  local function idx(x, y)
    return y * w + x + 1
  end

  local top = 0
  for y = 0, h - 1 do
    local any = false
    for x = 0, w - 1 do
      if not is_black(pixels[idx(x, y)], threshold) then
        any = true
        break
      end
    end
    if any then
      break
    end
    top = top + 1
  end

  local bottom = 0
  for y = h - 1, 0, -1 do
    local any = false
    for x = 0, w - 1 do
      if not is_black(pixels[idx(x, y)], threshold) then
        any = true
        break
      end
    end
    if any then
      break
    end
    bottom = bottom + 1
  end

  local left = 0
  for x = 0, w - 1 do
    local any = false
    for y = 0, h - 1 do
      if not is_black(pixels[idx(x, y)], threshold) then
        any = true
        break
      end
    end
    if any then
      break
    end
    left = left + 1
  end

  local right = 0
  for x = w - 1, 0, -1 do
    local any = false
    for y = 0, h - 1 do
      if not is_black(pixels[idx(x, y)], threshold) then
        any = true
        break
      end
    end
    if any then
      break
    end
    right = right + 1
  end

  return { unknown = false, top = top, bottom = bottom, left = left, right = right }
end

local function border_equal(a, b)
  if a.unknown then
    return b.unknown
  end
  if b.unknown then
    return false
  end
  return a.horizontal_size == b.horizontal_size and a.vertical_size == b.vertical_size
end

function M.new()
  local self = {
    enabled = true,
    unknown_switch_cnt = 600,
    border_switch_cnt = 50,
    max_inconsistent_cnt = 10,
    blur_remove_cnt = 1,
    mode = 0,
    threshold_percent = 5,
    current_border = { unknown = true, horizontal_size = 0, vertical_size = 0 },
    previous_detected_border = { unknown = true, horizontal_size = 0, vertical_size = 0 },
    consistent_cnt = 0,
    inconsistent_cnt = 10,
  }

  function self:set_enabled(enabled)
    self.enabled = enabled and true or false
    if not self.enabled then
      self:reset_state()
    end
  end

  function self:reset_state()
    self.current_border = { unknown = true, horizontal_size = 0, vertical_size = 0 }
    self.previous_detected_border = { unknown = true, horizontal_size = 0, vertical_size = 0 }
    self.consistent_cnt = 0
    self.inconsistent_cnt = self.max_inconsistent_cnt
  end

  function self:set_threshold_percent(p)
    if type(p) == "number" then
      self.threshold_percent = clamp(p, 0, 100)
    end
  end

  function self:set_mode(mode)
    if type(mode) == "number" then
      self.mode = math.floor(mode + 0.5)
    end
  end

  local function detect(frame)
    local threshold = math.floor(clamp(self.threshold_percent, 0, 100) / 100.0 * 255.0 + 0.5)
    local sizes = scan_border_sizes(frame, threshold)
    if sizes.unknown then
      return { unknown = true, horizontal_size = 0, vertical_size = 0 }
    end

    -- Modes:
    -- 0 default / 1 classic: crop both axes
    -- 2 osd: same as default (kept for compatibility)
    -- 3 letterbox: crop top/bottom only
    local top = math.min(sizes.top, sizes.bottom)
    local left = math.min(sizes.left, sizes.right)

    if self.mode == 3 then
      left = 0
    end

    return { unknown = false, horizontal_size = top, vertical_size = left }
  end

  local function update_border(new_detected)
    if border_equal(new_detected, self.previous_detected_border) then
      self.consistent_cnt = self.consistent_cnt + 1
      self.inconsistent_cnt = 0
    else
      self.inconsistent_cnt = self.inconsistent_cnt + 1
      if self.inconsistent_cnt <= self.max_inconsistent_cnt then
        return false
      end
      self.previous_detected_border = new_detected
      self.consistent_cnt = 0
    end

    if border_equal(self.current_border, new_detected) then
      self.inconsistent_cnt = 0
      return false
    end

    local changed = false
    if new_detected.unknown then
      if self.consistent_cnt == self.unknown_switch_cnt then
        self.current_border = new_detected
        changed = true
      end
    else
      if self.current_border.unknown or self.consistent_cnt == self.border_switch_cnt then
        self.current_border = new_detected
        changed = true
      end
    end

    return changed
  end

  function self:process_frame(frame)
    if not self.enabled then
      self.current_border = { unknown = true, horizontal_size = 0, vertical_size = 0 }
      return
    end

    local detected = detect(frame)
    if not detected.unknown then
      if detected.horizontal_size > 0 then
        detected.horizontal_size = detected.horizontal_size + self.blur_remove_cnt
      end
      if detected.vertical_size > 0 then
        detected.vertical_size = detected.vertical_size + self.blur_remove_cnt
      end
    end

    update_border(detected)
  end

  function self:crop_region_for(frame)
    if self.current_border.unknown then
      return { left = 0, right = 0, top = 0, bottom = 0 }
    end

    local w = (frame.width or 1)
    local h = (frame.height or 1)
    if w <= 0 then
      w = 1
    end
    if h <= 0 then
      h = 1
    end

    local top = clamp(self.current_border.horizontal_size / h, 0.0, 0.45)
    local left = clamp(self.current_border.vertical_size / w, 0.0, 0.45)

    return { left = left, right = left, top = top, bottom = top }
  end

  return self
end

return M

