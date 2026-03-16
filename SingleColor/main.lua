local plugin = {}

-- Current color (default: white, matching reference SingleColor default 0x00FFFFFF)
local cur_r, cur_g, cur_b = 255, 255, 255

local function parse_hex_color(value)
    if type(value) ~= "string" then
        return nil
    end

    local hex = value:gsub("%s+", "")
    if hex:sub(1, 1) == "#" then
        hex = hex:sub(2)
    end

    if #hex == 3 then
        hex = hex:sub(1, 1):rep(2)
            .. hex:sub(2, 2):rep(2)
            .. hex:sub(3, 3):rep(2)
    end

    if #hex ~= 6 or hex:find("[^%x]") then
        return nil
    end

    return tonumber(hex:sub(1, 2), 16) or 255,
        tonumber(hex:sub(3, 4), 16) or 255,
        tonumber(hex:sub(5, 6), 16) or 255
end

function plugin.on_params(p)
    if type(p) ~= "table" then
        return
    end

    if type(p.color) == "string" then
        local r, g, b = parse_hex_color(p.color)
        if r then
            cur_r, cur_g, cur_b = r, g, b
        end
    end
end

function plugin.on_tick(t, buffer, width, height)
    local n = buffer:len()
    for i = 1, n do
        buffer:set(i, cur_r, cur_g, cur_b)
    end
end

return plugin
