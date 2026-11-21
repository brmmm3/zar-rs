local zar = require("luazarlib")

local paths = {}
local function temporary(suffix)
    local name = os.tmpname()
    assert(os.remove(name))
    name = name .. suffix
    paths[#paths + 1] = name
    return name
end

local function check_error(value, message, fragment)
    assert(value == nil, "expected failure")
    assert(type(message) == "string" and message:find(fragment, 1, true), tostring(message))
end

local function run()
    local input = temporary(".txt")
    local second = temporary(".txt")
    local archive = temporary(".zar")
    local file = assert(io.open(input, "wb"))
    assert(file:write("hello\0Lua"))
    assert(file:close())
    file = assert(io.open(second, "wb"))
    assert(file:write("second"))
    assert(file:close())
    local name = input:match("([^/\\]+)$")
    local second_name = second:match("([^/\\]+)$")

    local result = assert(zar.create_archive {
        archive = archive, paths = { input }, block_size = 65536,
    })
    assert(result.block_size == 65536)
    local listing = assert(zar.list_archive { archive = archive })
    assert(#listing == 1 and listing[1].path == name)
    assert(zar.check_archive { archive = archive }.commits == 1)
    local raw = assert(zar.execute_json("check_archive",
        require("dkjson").encode { archive = archive }))
    assert(raw:find('"commits":1', 1, true))
    local contents = assert(zar.extract_archive_to_memory { archive = archive })
    assert(string.char(table.unpack(contents[name])) == "hello\0Lua")

    assert(zar.add_archive { archive = archive, paths = { second } })
    assert(#assert(zar.info_archive { archive = archive }) == 2)
    assert(zar.remove_archive { archive = archive, paths = { second_name } })
    assert(#assert(zar.list_archive { archive = archive }) == 1)
    assert(zar.update_custom_metadata {
        archive = archive, set = { project = "lua", revision = 3 },
    } == zar.null)
    local metadata = assert(zar.read_custom_metadata { archive = archive })
    assert(metadata.project == "lua" and metadata.revision == 3)

end

local function errors_and_streaming()
    local value, err = zar.execute("unknown", {})
    check_error(value, err, "unknown")
    value, err = zar.execute_json("list_archive", "not JSON")
    check_error(value, err, "JSON")
    value, err = zar.execute("list_archive", { [1] = "bad" })
    check_error(value, err, "string keys")
    assert(not pcall(zar.execute_json, "list_archive\0bad", "{}"))
    assert(not pcall(zar.execute_json, "list_archive", "{}\0bad"))
    assert(not pcall(zar.extract_single_file_to_writer, {}, "bad\0path", {}))
    assert(require("dkjson").encode(zar.array({})) == "[]")
    assert(require("dkjson").encode(zar.object({})) == "{}")
    value, err = zar.create_archive_from_readers({}, {}, false)
    check_error(value, err, "options")
    value, err = zar.create_archive_from_readers({}, { [1] = {}, [3] = {} })
    check_error(value, err, "dense array")
    value, err = zar.create_archive_from_readers({}, { unexpected = {} })
    check_error(value, err, "dense array")
    value, err = zar.execute("list_archive", { archive = function() end })
    check_error(value, err, "type")
    local cyclic = {}; cyclic.self = cyclic
    value, err = zar.execute("list_archive", cyclic)
    check_error(value, err, "cycle")
    value, err = zar.update_custom_metadata { archive = "ignored", set = { value = math.huge } }
    check_error(value, err, "finite")

    local archive = assert(io.tmpfile())
    local source = assert(io.tmpfile())
    local content = ("binary\0data"):rep(15000)
    assert(source:write(content))
    assert(source:seek("set", 0))
    assert(zar.create_archive_from_readers(archive, {
        { path = "dir/data.bin", reader = source },
        { path = "empty", reader = { read = function() return nil end } },
    }, { block_size = 65536, password = "secret", aead = "chacha20-poly1305" }))

    local output = assert(io.tmpfile())
    local count = assert(zar.extract_single_file_to_writer(
        archive, "dir/data.bin", output, { password = "secret" }))
    assert(count == #content)
    assert(output:seek("set", 0))
    assert(output:read("*a") == content)
    value, err = zar.extract_single_file_to_writer(archive, "dir/data.bin", output,
        { password = "wrong" })
    assert(value == nil and type(err) == "string")

    local chunks = {}
    local writer = {
        write = function(_, data)
            local n = math.min(#data, 37)
            chunks[#chunks + 1] = data:sub(1, n)
            return n
        end,
    }
    count = assert(zar.extract_single_file_to_writer(
        archive, "dir/data.bin", writer, { password = "secret" }))
    assert(count == #content and table.concat(chunks) == content)

    local written = 0
    local broken = {
        write = function(_, data)
            if written > 0 then error("writer failed deliberately") end
            written = math.min(#data, 11)
            return written
        end,
    }
    local partial
    value, err, partial = zar.extract_single_file_to_writer(
        archive, "dir/data.bin", broken, { password = "secret" })
    check_error(value, err, "writer failed deliberately")
    assert(partial == written and partial == 11)

    value, err = zar.extract_single_file_to_writer(archive, "missing", output,
        { password = "secret" })
    assert(value == nil and type(err) == "string")
    local failing_archive = assert(io.tmpfile())
    value, err = zar.create_archive_from_readers(failing_archive, {
        { path = "broken", reader = { read = function() error("reader failed deliberately") end } },
    }, { block_size = 65536 })
    check_error(value, err, "reader failed deliberately")
    assert(failing_archive:close())
    local mixed_archive = assert(io.tmpfile())
    value, err = zar.create_archive_from_readers(mixed_archive, {
        { path = "broken", reader = { read = function() return "data", "read failed" end } },
    }, { block_size = 65536 })
    check_error(value, err, "read failed")
    assert(mixed_archive:close())
    local oversized_archive = assert(io.tmpfile())
    value, err = zar.create_archive_from_readers(oversized_archive, {
        { path = "broken", reader = { read = function(_, n) return ("x"):rep(n + 1) end } },
    }, { block_size = 65536 })
    check_error(value, err, "too many bytes")
    assert(oversized_archive:close())
    value, err = zar.create_archive_from_readers(
        { seek = function() error("seek failed deliberately") end, write = function() return true end },
        {})
    check_error(value, err, "seek failed deliberately")
    assert(source:close())
    assert(output:close())
    assert(archive:close())
end

local ok, failure = xpcall(function()
    run()
    errors_and_streaming()
end, debug.traceback)
for _, path in ipairs(paths) do os.remove(path) end
if not ok then error(failure) end
print("luazarlib tests passed")
