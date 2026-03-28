local M = {}

-- Greet a person by name.
function M.greet(name)
    return "Hello, " .. name
end

local function private_helper()
    return 42
end

function global_fn()
    return true
end

local another_private = function()
    return false
end

local result = M.greet("world")

return M
