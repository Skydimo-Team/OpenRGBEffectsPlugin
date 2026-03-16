local color = require("lib.color")

local M = {}

local math_floor = math.floor
local math_exp = math.exp
local math_max = math.max
local clamp = color.clamp
local unpack_rgb = color.unpack_rgb
local lerp_color = color.lerp_color

-- ============================================================================
-- Gaussian blur (separable, flat pixel array: width * height * 3 channels)
-- ============================================================================

-- Reusable buffers to avoid per-call allocation
local blur_temp = {}
local blur_out = {}

local function gaussian_blur(pixels, w, h, radius)
  if radius <= 0 then return pixels end

  local sigma = radius / 3.0
  local sigma2 = 2.0 * sigma * sigma
  local ksize = radius * 2 + 1
  local kernel = {}
  local ksum = 0.0
  for i = 1, ksize do
    local x = i - 1 - radius
    local val = math_exp(-(x * x) / sigma2)
    kernel[i] = val
    ksum = ksum + val
  end
  for i = 1, ksize do
    kernel[i] = kernel[i] / ksum
  end

  local temp = blur_temp
  local out = blur_out

  -- Horizontal pass
  for y = 0, h - 1 do
    for x = 0, w - 1 do
      local rs, gs, bs = 0.0, 0.0, 0.0
      for k = 1, ksize do
        local sx = x + (k - 1 - radius)
        if sx < 0 then sx = 0 end
        if sx >= w then sx = w - 1 end
        local off = (y * w + sx) * 3
        local weight = kernel[k]
        rs = rs + (pixels[off + 1] or 0) * weight
        gs = gs + (pixels[off + 2] or 0) * weight
        bs = bs + (pixels[off + 3] or 0) * weight
      end
      local off = (y * w + x) * 3
      temp[off + 1] = clamp(math_floor(rs + 0.5), 0, 255)
      temp[off + 2] = clamp(math_floor(gs + 0.5), 0, 255)
      temp[off + 3] = clamp(math_floor(bs + 0.5), 0, 255)
    end
  end

  -- Vertical pass
  for y = 0, h - 1 do
    for x = 0, w - 1 do
      local rs, gs, bs = 0.0, 0.0, 0.0
      for k = 1, ksize do
        local sy = y + (k - 1 - radius)
        if sy < 0 then sy = 0 end
        if sy >= h then sy = h - 1 end
        local off = (sy * w + x) * 3
        local weight = kernel[k]
        rs = rs + (temp[off + 1] or 0) * weight
        gs = gs + (temp[off + 2] or 0) * weight
        bs = bs + (temp[off + 3] or 0) * weight
      end
      local off = (y * w + x) * 3
      out[off + 1] = clamp(math_floor(rs + 0.5), 0, 255)
      out[off + 2] = clamp(math_floor(gs + 0.5), 0, 255)
      out[off + 3] = clamp(math_floor(bs + 0.5), 0, 255)
    end
  end

  return out
end

-- ============================================================================
-- Palette extraction from album art
-- ============================================================================

function M.extract(art, max_colors)
  max_colors = max_colors or 5
  if max_colors < 1 then max_colors = 1 end
  if max_colors > 16 then max_colors = 16 end

  local aw = art.width
  local ah = art.height
  local raw_pixels = art.pixels
  if not raw_pixels or aw <= 0 or ah <= 0 then return nil end

  -- Unpack pixels into flat R,G,B array
  local flat = {}
  for y = 0, ah - 1 do
    for x = 0, aw - 1 do
      local packed = raw_pixels[y * aw + x + 1] or 0
      local r, g, b = unpack_rgb(packed)
      local off = (y * aw + x) * 3
      flat[off + 1] = r
      flat[off + 2] = g
      flat[off + 3] = b
    end
  end

  -- Apply Gaussian blur (radius=8)
  local blurred = gaussian_blur(flat, aw, ah, 8)

  -- 4x4 grid sampling with color dedup
  local grid_size = 4
  local min_dist_sq = 30 * 30
  local palette = {}

  for gy = 0, grid_size - 1 do
    for gx = 0, grid_size - 1 do
      local sx = math_floor((gx * aw) / grid_size + aw / (grid_size * 2))
      local sy = math_floor((gy * ah) / grid_size + ah / (grid_size * 2))
      if sx >= aw then sx = aw - 1 end
      if sy >= ah then sy = ah - 1 end

      local off = (sy * aw + sx) * 3
      local r = blurred[off + 1] or 0
      local g = blurred[off + 2] or 0
      local b = blurred[off + 3] or 0

      -- Skip very dark pixels
      local maxc = r
      if g > maxc then maxc = g end
      if b > maxc then maxc = b end
      if maxc < 24 then
        goto continue_grid
      end

      -- Find closest existing palette entry
      local closest_idx = -1
      local closest_dist = 0x7FFFFFFF
      for i = 1, #palette do
        local p = palette[i]
        local dr = p[1] - r
        local dg = p[2] - g
        local db = p[3] - b
        local dist = dr * dr + dg * dg + db * db
        if dist < closest_dist then
          closest_dist = dist
          closest_idx = i
        end
      end

      if closest_idx > 0 and closest_dist < min_dist_sq then
        palette[closest_idx][4] = palette[closest_idx][4] + 1.0
      elseif #palette < max_colors then
        palette[#palette + 1] = { r, g, b, 1.0 }
      end

      ::continue_grid::
    end
  end

  if #palette == 0 then return nil end

  -- Normalize weights
  local total_weight = 0.0
  for i = 1, #palette do
    total_weight = total_weight + palette[i][4]
  end

  if total_weight <= 0.0 then
    local uniform = 1.0 / #palette
    for i = 1, #palette do
      palette[i][4] = uniform
    end
  else
    for i = 1, #palette do
      palette[i][4] = palette[i][4] / total_weight
    end
  end

  -- Sort by weight descending
  table.sort(palette, function(a, b) return a[4] > b[4] end)

  return palette
end

-- ============================================================================
-- Palette sampling (weighted segment interpolation with flow)
-- ============================================================================

function M.sample(palette, pos_01, flow_phase_01)
  if not palette or #palette == 0 then
    return 0, 0, 0
  end

  pos_01 = clamp(pos_01, 0.0, 1.0)
  flow_phase_01 = clamp(flow_phase_01, 0.0, 1.0)

  if #palette == 1 then
    return palette[1][1], palette[1][2], palette[1][3]
  end

  local pos = pos_01 + flow_phase_01
  if pos > 1.0 then pos = pos - 1.0 end

  local cumulative = 0.0
  local index0 = 1
  local index1 = 2
  local local_t = 0.0

  for i = 1, #palette do
    local w = palette[i][4]
    if w <= 0.0 then
      goto continue_segment
    end

    local next_cum = cumulative + w
    if pos <= next_cum or i == #palette then
      index0 = i
      index1 = (i < #palette) and (i + 1) or 1
      local_t = (pos - cumulative) / math_max(w, 0.0001)
      local_t = clamp(local_t, 0.0, 1.0)
      break
    end

    cumulative = next_cum
    ::continue_segment::
  end

  local c0 = palette[index0]
  local c1 = palette[index1]
  return lerp_color(c0[1], c0[2], c0[3], c1[1], c1[2], c1[3], local_t)
end

-- ============================================================================
-- Palette cache (fingerprint-based change detection)
-- ============================================================================

local cache = {
  checksum = nil,
  palette = nil,
}

local function art_fingerprint(art)
  local px = art.pixels
  if not px then return nil end
  local n = #px
  if n == 0 then return nil end
  local h = 0x55555555
  local step = math_max(1, math_floor(n / 16))
  for i = 1, n, step do
    h = ((h * 31) + (px[i] or 0)) % 0x7FFFFFFF
  end
  return h
end

function M.get_cached(art, max_colors)
  local fp = art_fingerprint(art)
  if fp and fp == cache.checksum and cache.palette then
    return cache.palette
  end
  local pal = M.extract(art, max_colors or 8)
  cache.checksum = fp
  cache.palette = pal
  return pal
end

return M
