local json = require("dkjson")
local native = require("_luazarlib")

local M = { null = json.null }

local function validate_numbers(value, seen)
    if type(value) == "number" then
        if value ~= value or value == math.huge or value == -math.huge then
            return nil, "JSON numbers must be finite"
        end
    elseif type(value) == "table" and value ~= json.null then
        if seen[value] then return nil, "reference cycle in request" end
        seen[value] = true
        for _, item in pairs(value) do
            local ok, err = validate_numbers(item, seen)
            if not ok then return nil, err end
        end
        seen[value] = nil
    end
    return true
end

-- dkjson otherwise treats an empty table as an array.
local function object(value, name)
    if type(value) ~= "table" then
        return nil, name .. " must be a table representing a JSON object"
    end
    local copy = {}
    for key, item in pairs(value) do
        if type(key) ~= "string" then
            return nil, name .. " must have string keys"
        end
        copy[key] = item
    end
    return setmetatable(copy, { __jsontype = "object" })
end

local function encode_object(value, name)
    local obj, err = object(value, name)
    if not obj then return nil, err end
    local valid
    valid, err = validate_numbers(obj, {})
    if not valid then return nil, err end
    local ok, encoded, message = pcall(json.encode, obj)
    if not ok then return nil, tostring(encoded) end
    if not encoded then return nil, message end
    return encoded
end

function M.array(values)
    if type(values) ~= "table" then
        error("array requires a table", 2)
    end
    return setmetatable(values, { __jsontype = "array" })
end

function M.object(values)
    local result, err = object(values, "object")
    if not result then error(err, 2) end
    return result
end

function M.execute_json(operation, request)
    return native.execute_json(operation, request)
end

function M.execute(operation, request)
    local encoded, err = encode_object(request, "request")
    if not encoded then return nil, err end
    local response
    response, err = native.execute_json(operation, encoded)
    if not response then return nil, err end
    local value, _, decode_error = json.decode(response, 1, json.null)
    if decode_error then return nil, "invalid response JSON: " .. decode_error end
    return value
end

local operations = {
    "create_archive", "add_archive", "remove_archive", "read_custom_metadata",
    "update_custom_metadata", "join_archive", "undo_archive",
    "encrypt_archive_commits", "decrypt_archive_commits", "check_archive",
    "info_archive", "cross_commit_references", "list_archive", "grep_archive",
    "extract_archive", "extract_archive_to_memory",
}
for _, operation in ipairs(operations) do
    M[operation] = function(request) return M.execute(operation, request) end
end

function M.create_archive_from_readers(archive, inputs, options)
    local encoded, err = encode_object(options == nil and {} or options, "options")
    if not encoded then return nil, err end
    if type(inputs) ~= "table" then return nil, "inputs must be an array" end
    local count, maximum = 0, 0
    for key in next, inputs do
        if math.type(key) ~= "integer" or key < 1 then
            return nil, "inputs must be a dense array"
        end
        count = count + 1
        maximum = math.max(maximum, key)
    end
    if count ~= maximum then return nil, "inputs must be a dense array" end
    return native.create_archive_from_readers(archive, inputs, encoded)
end

function M.extract_single_file_to_writer(archive, path, output, options)
    local encoded, err = encode_object(options == nil and {} or options, "options")
    if not encoded then return nil, err end
    return native.extract_single_file_to_writer(archive, path, output, encoded)
end

return M
