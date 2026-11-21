local zar = require("luazarlib")

local archive, input = arg[1], arg[2]
if not archive or not input then
    error("usage: lua examples/basic.lua ARCHIVE INPUT")
end

assert(zar.create_archive { archive = archive, paths = { input } })
for _, entry in ipairs(assert(zar.list_archive { archive = archive })) do
    print(entry.type, entry.path, entry.size or "")
end
local report = assert(zar.check_archive { archive = archive })
print("commits:", report.commits, "checked blocks:", report.checked_blocks)
