local renderer = require("lib/renderer")
local border = require("lib/border")

local plugin = {}

local config = {
  smoothness = 80,
  brightness = 1.0,
  saturation = 1.0,
  gamma = 1.0,
  colorTemperature = 6500,
  blur = 0,
  autoCrop = false,
  bbThreshold = 5,
  bbMode = 0,
  bbBorderFrameCnt = 50,
  bbUnknownFrameCnt = 600,
  bbMaxInconsistentCnt = 10,
  bbBlurRemoveCnt = 1,
  redCalibration = 1.0,
  greenCalibration = 1.0,
  blueCalibration = 1.0,
}

local state = {
  previous_buffer = {},
  border_processor = border.new(),
}

function plugin.on_init()
  -- no-op
end

function plugin.on_params(p)
  if type(p) ~= "table" then
    return
  end

  renderer.apply_params(config, p)
end

function plugin.on_tick(t, buffer, width, height)
  local n = buffer:len()
  if n <= 0 then
    return
  end

  if type(width) ~= "number" or width <= 0 then
    width = n
  end
  if type(height) ~= "number" or height <= 0 then
    height = 1
  end

  if not screen or type(screen.capture) ~= "function" then
    state.previous_buffer = {}
    if state.border_processor and type(state.border_processor.reset_state) == "function" then
      state.border_processor:reset_state()
    end
    renderer.fill_black(buffer)
    return
  end

  -- Capture on a fixed-ish grid so auto-crop can work even for 1D layouts.
  local cap_w = math.max(width, 64)
  local cap_h = math.max(height, 36)
  cap_w = math.min(cap_w, 256)
  cap_h = math.min(cap_h, 256)

  local frame = screen.capture(cap_w, cap_h)
  if not frame then
    state.previous_buffer = {}
    if state.border_processor and type(state.border_processor.reset_state) == "function" then
      state.border_processor:reset_state()
    end
    renderer.fill_black(buffer)
    return
  end

  renderer.render(frame, buffer, width, height, state, config, t)
end

function plugin.on_shutdown()
  -- no-op
end

return plugin

