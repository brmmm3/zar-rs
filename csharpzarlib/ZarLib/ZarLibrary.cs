using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

namespace Zar;

/// <summary>
/// C# wrapper for the ZAR archive library. All operations are forwarded to the
/// czarlib JSON C ABI. Requests may be JSON strings or any object serializable
/// to a JSON object (for example a dictionary or an anonymous type).
/// </summary>
public static class ZarLibrary
{
    private const string NativeName = "czarlib";

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
    };

    [DllImport(NativeName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int czarlib_execute_json(byte[] operation, byte[] requestJson, out IntPtr responseJson, out IntPtr errorMessage);

    [DllImport(NativeName, CallingConvention = CallingConvention.Cdecl)]
    private static extern void czarlib_string_free(IntPtr value);

    static ZarLibrary()
    {
        NativeLibrary.SetDllImportResolver(typeof(ZarLibrary).Assembly, Resolve);
    }

    private static IntPtr Resolve(string name, Assembly assembly, DllImportSearchPath? path)
    {
        if (name != NativeName)
        {
            return IntPtr.Zero;
        }
        foreach (var candidate in Candidates())
        {
            if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var handle))
            {
                return handle;
            }
        }
        return NativeLibrary.TryLoad(NativeName, assembly, path, out var fallback) ? fallback : IntPtr.Zero;
    }

    private static IEnumerable<string> Candidates()
    {
        var file = RuntimeInformation.IsOSPlatform(OSPlatform.Windows) ? "czarlib.dll"
            : RuntimeInformation.IsOSPlatform(OSPlatform.OSX) ? "libczarlib.dylib"
            : "libczarlib.so";

        var explicitPath = Environment.GetEnvironmentVariable("ZAR_LIBRARY");
        if (!string.IsNullOrEmpty(explicitPath))
        {
            yield return explicitPath;
        }
        yield return Path.Combine(AppContext.BaseDirectory, file);
        foreach (var start in new[] { AppContext.BaseDirectory, Directory.GetCurrentDirectory() })
        {
            for (var dir = new DirectoryInfo(start); dir != null; dir = dir.Parent)
            {
                yield return Path.Combine(dir.FullName, "target", "debug", file);
                yield return Path.Combine(dir.FullName, "target", "release", file);
            }
        }
    }

    private static byte[] Utf8Z(string value)
    {
        if (value.Contains('\0'))
        {
            throw new ZarException(0, "string arguments must not contain NUL");
        }
        var bytes = new byte[Encoding.UTF8.GetByteCount(value) + 1];
        Encoding.UTF8.GetBytes(value, 0, value.Length, bytes, 0);
        return bytes;
    }

    private static string? TakeString(IntPtr pointer)
    {
        if (pointer == IntPtr.Zero)
        {
            return null;
        }
        try
        {
            return Marshal.PtrToStringUTF8(pointer);
        }
        finally
        {
            czarlib_string_free(pointer);
        }
    }

    /// <summary>Executes one operation with a JSON request and returns the JSON response.</summary>
    public static string ExecuteJson(string operation, string requestJson)
    {
        ArgumentNullException.ThrowIfNull(operation);
        ArgumentNullException.ThrowIfNull(requestJson);
        if (operation.Length == 0)
        {
            throw new ZarException(0, "operation must not be empty");
        }
        var status = czarlib_execute_json(Utf8Z(operation), Utf8Z(requestJson), out var response, out var error);
        var responseText = TakeString(response);
        var errorText = TakeString(error);
        if (status != 0)
        {
            throw new ZarException(status, errorText ?? $"czarlib failed with status {status}");
        }
        return responseText ?? throw new ZarException(2, "czarlib returned no response");
    }

    /// <summary>Executes one operation. The request is a JSON string or an object serialized to JSON.</summary>
    public static string Execute(string operation, object request) =>
        ExecuteJson(operation, request as string ?? JsonSerializer.Serialize(request, JsonOptions));

    public static string CreateArchive(object request) => Execute("create_archive", request);
    public static string AddArchive(object request) => Execute("add_archive", request);
    public static string RemoveArchive(object request) => Execute("remove_archive", request);
    public static string JoinArchive(object request) => Execute("join_archive", request);
    public static string CheckArchive(object request) => Execute("check_archive", request);
    public static string InfoArchive(object request) => Execute("info_archive", request);
    public static string ListArchive(object request) => Execute("list_archive", request);
    public static string ExtractArchive(object request) => Execute("extract_archive", request);

    /// <summary>Extracts into memory and returns archive path to file bytes.</summary>
    public static Dictionary<string, byte[]> ExtractArchiveToMemory(object request)
    {
        using var document = JsonDocument.Parse(Execute("extract_archive_to_memory", request));
        var files = new Dictionary<string, byte[]>();
        foreach (var property in document.RootElement.EnumerateObject())
        {
            var bytes = new byte[property.Value.GetArrayLength()];
            var index = 0;
            foreach (var value in property.Value.EnumerateArray())
            {
                bytes[index++] = value.GetByte();
            }
            files[property.Name] = bytes;
        }
        return files;
    }
}
