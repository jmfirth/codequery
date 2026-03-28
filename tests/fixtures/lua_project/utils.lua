local utils = require("main")

local function format_name(first, last)
    return first .. " " .. last
end

function utils.add(a, b)
    return a + b
end

return utils
