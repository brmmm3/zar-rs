local zar = require("luazarlib")

local archive = assert(io.tmpfile())
local input = assert(io.tmpfile())
local output = assert(io.tmpfile())
assert(input:write("Hello from Lua!\n"))
assert(input:seek("set", 0))

assert(zar.create_archive_from_readers(archive, {
    { path = "hello.txt", reader = input },
}, { block_size = 65536 }))
local written = assert(zar.extract_single_file_to_writer(archive, "hello.txt", output))
assert(output:seek("set", 0))
io.write(assert(output:read("*a")))
print("Bytes extracted:", written)

assert(input:close())
assert(output:close())
assert(archive:close())
