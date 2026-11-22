-- Transfer one archive entry over TCP. Requires LuaSocket; use trusted networks only.
local socket = require("socket")
local zar = require("luazarlib")
local max_chunk = 65536

local function send(conn, bytes)
    local offset = 1
    while offset <= #bytes do
        local last, err = conn:send(bytes, offset)
        if not last then error(err) end
        offset = last + 1
    end
end

local function read_exact(conn, length)
    local data, err = conn:receive(length)
    if not data then error(err or "connection closed before transfer completed") end
    return data
end

local function send_length(conn, length)
    send(conn, string.pack(">I4", length))
end

local function read_length(conn)
    return string.unpack(">I4", read_exact(conn, 4))
end

local function handle(conn, archive_path)
    local length = read_length(conn)
    assert(length > 0 and length <= max_chunk, "invalid entry path length")
    local entry = read_exact(conn, length)
    local archive = assert(io.open(archive_path, "rb"))
    local writer = {}
    function writer:write(data)
        for offset = 1, #data, max_chunk do
            local chunk = data:sub(offset, offset + max_chunk - 1)
            send_length(conn, #chunk)
            send(conn, chunk)
        end
        return #data
    end
    local count, err = zar.extract_single_file_to_writer(archive, entry, writer)
    assert(archive:close())
    assert(count, err)
    send_length(conn, 0)
    print(entry .. ": " .. count .. " bytes")
end

local function serve(host, port, archive)
    local server = assert(socket.bind(host, port))
    print("Listening on " .. host .. ":" .. port)
    while true do
        local conn = assert(server:accept())
        local ok, err = pcall(handle, conn, archive)
        conn:close()
        if not ok then io.stderr:write("Request failed: " .. tostring(err) .. "\n") end
    end
end

local function receive(host, port, entry)
    assert(#entry > 0 and #entry <= max_chunk, "invalid entry path length")
    local conn = assert(socket.tcp())
    assert(conn:connect(host, port))
    send_length(conn, #entry)
    send(conn, entry)
    while true do
        local length = read_length(conn)
        if length == 0 then break end
        assert(length <= max_chunk, "invalid data frame length")
        assert(io.stdout:write(read_exact(conn, length)))
    end
    assert(io.stdout:flush())
    conn:close()
end

if arg[1] == "server" and #arg == 4 then
    serve(arg[2], assert(tonumber(arg[3])), arg[4])
elseif arg[1] == "client" and #arg == 4 then
    receive(arg[2], assert(tonumber(arg[3])), arg[4])
else
    error("usage: tcp_streaming.lua server HOST PORT ARCHIVE | client HOST PORT ENTRY > OUTPUT")
end
