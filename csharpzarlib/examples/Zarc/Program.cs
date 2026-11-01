using System.Text.Json;
using Zar;

const string Usage = """
    Usage: zarc COMMAND ...
      create|c ARCHIVE PATH...
      add|a ARCHIVE PATH...
      remove|r ARCHIVE ARCHIVE_PATH...
      join|j ARCHIVE --from ID --through ID
      check|k ARCHIVE
      info|i ARCHIVE
      list|l ARCHIVE
      extract|e ARCHIVE [DESTINATION] [--dry-run]
      memory ARCHIVE
    """;

if (args.Length == 0)
{
    Console.Error.WriteLine(Usage);
    return 2;
}
if (args[0] is "-h" or "--help")
{
    Console.WriteLine(Usage);
    return 0;
}

var operation = args[0] switch
{
    "create" or "c" => "create_archive",
    "add" or "a" => "add_archive",
    "remove" or "r" => "remove_archive",
    "join" or "j" => "join_archive",
    "check" or "k" => "check_archive",
    "info" or "i" => "info_archive",
    "list" or "l" => "list_archive",
    "extract" or "e" => "extract_archive",
    "memory" => "extract_archive_to_memory",
    _ => null,
};
if (operation is null || args.Length < 2)
{
    Console.Error.WriteLine(Usage);
    return 2;
}

var request = new Dictionary<string, object?> { ["archive"] = args[1] };
var rest = args[2..];
switch (operation)
{
    case "create_archive" or "add_archive" or "remove_archive":
        if (rest.Length == 0)
        {
            Console.Error.WriteLine(Usage);
            return 2;
        }
        request["paths"] = rest;
        break;
    case "join_archive":
        if (rest.Length != 4 || rest[0] != "--from" || rest[2] != "--through")
        {
            Console.Error.WriteLine(Usage);
            return 2;
        }
        request["from_id"] = rest[1];
        request["through_id"] = rest[3];
        break;
    case "extract_archive":
        var destination = "restored";
        foreach (var arg in rest)
        {
            if (arg == "--dry-run")
            {
                request["dry_run"] = true;
            }
            else
            {
                destination = arg;
            }
        }
        request["destination"] = destination;
        break;
}

try
{
    var response = ZarLibrary.Execute(operation, request);
    using var document = JsonDocument.Parse(response);
    Console.WriteLine(JsonSerializer.Serialize(document.RootElement, new JsonSerializerOptions { WriteIndented = true }));
    return 0;
}
catch (ZarException ex)
{
    Console.Error.WriteLine($"zarc-cs: {ex.Message}");
    return 1;
}
