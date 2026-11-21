using System.Buffers;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Text.Json;

namespace Zar;

/// <summary>A regular file to add to an archive from a readable stream.</summary>
public sealed record ZarStreamInput(string Path, Stream Stream);

public static partial class ZarLibrary
{
    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int ReadCallback(IntPtr context, IntPtr buffer, nuint length, out nuint count);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int WriteCallback(IntPtr context, IntPtr buffer, nuint length, out nuint count);

    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    private delegate int SeekCallback(IntPtr context, int origin, long offset, out ulong position);

    private static readonly ReadCallback ReadDelegate = ReadStream;
    private static readonly WriteCallback WriteDelegate = WriteStream;
    private static readonly SeekCallback SeekDelegate = SeekStream;
    private static readonly IntPtr ReadPointer = Marshal.GetFunctionPointerForDelegate(ReadDelegate);
    private static readonly IntPtr WritePointer = Marshal.GetFunctionPointerForDelegate(WriteDelegate);
    private static readonly IntPtr SeekPointer = Marshal.GetFunctionPointerForDelegate(SeekDelegate);

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeStream
    {
        public IntPtr Context;
        public IntPtr Read;
        public IntPtr Write;
        public IntPtr Seek;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct NativeInput
    {
        public IntPtr Path;
        public NativeStream Stream;
    }

    [DllImport(NativeName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int czarlib_create_archive_from_readers(
        in NativeStream archive, [In] NativeInput[] inputs, nuint count,
        byte[] optionsJson, out IntPtr errorMessage);

    [DllImport(NativeName, CallingConvention = CallingConvention.Cdecl)]
    private static extern int czarlib_extract_single_file_to_writer(
        in NativeStream archive, byte[] path, in NativeStream destination,
        byte[] optionsJson, out ulong written, out IntPtr errorMessage);

    private sealed class StreamCall : IDisposable
    {
        private readonly List<GCHandle> _handles = new();
        public ExceptionDispatchInfo? Failure { get; set; }

        public NativeStream Add(Stream stream)
        {
            ArgumentNullException.ThrowIfNull(stream);
            var handle = GCHandle.Alloc(new StreamContext(this, stream));
            _handles.Add(handle);
            return new NativeStream
            {
                Context = GCHandle.ToIntPtr(handle),
                Read = stream.CanRead ? ReadPointer : IntPtr.Zero,
                Write = stream.CanWrite ? WritePointer : IntPtr.Zero,
                Seek = stream.CanSeek ? SeekPointer : IntPtr.Zero,
            };
        }

        public void Complete(int status, IntPtr error)
        {
            var message = TakeString(error);
            Failure?.Throw();
            if (status != 0)
            {
                throw new ZarException(status, message ?? $"czarlib failed with status {status}");
            }
        }

        public void Dispose()
        {
            foreach (var handle in _handles)
            {
                handle.Free();
            }
        }
    }

    private sealed record StreamContext(StreamCall Call, Stream Stream);

    private static int ReadStream(IntPtr context, IntPtr buffer, nuint length, out nuint count)
    {
        count = 0;
        StreamContext? state = null;
        try
        {
            state = (StreamContext)GCHandle.FromIntPtr(context).Target!;
            if (state.Call.Failure != null) return 1;
            var size = (int)Math.Min((ulong)length, 65536UL);
            if (size == 0) return 0;
            var bytes = ArrayPool<byte>.Shared.Rent(size);
            try
            {
                var read = state.Stream.Read(bytes, 0, size);
                Marshal.Copy(bytes, 0, buffer, read);
                count = (nuint)read;
            }
            finally { ArrayPool<byte>.Shared.Return(bytes); }
            return 0;
        }
        catch (Exception ex)
        {
            if (state != null) state.Call.Failure ??= ExceptionDispatchInfo.Capture(ex);
            return 1;
        }
    }

    private static int WriteStream(IntPtr context, IntPtr buffer, nuint length, out nuint count)
    {
        count = 0;
        StreamContext? state = null;
        try
        {
            state = (StreamContext)GCHandle.FromIntPtr(context).Target!;
            if (state.Call.Failure != null) return 1;
            var size = (int)Math.Min((ulong)length, 65536UL);
            if (size == 0) return 0;
            var bytes = ArrayPool<byte>.Shared.Rent(size);
            try
            {
                Marshal.Copy(buffer, bytes, 0, size);
                state.Stream.Write(bytes, 0, size);
                count = (nuint)size;
            }
            finally { ArrayPool<byte>.Shared.Return(bytes); }
            return 0;
        }
        catch (Exception ex)
        {
            if (state != null) state.Call.Failure ??= ExceptionDispatchInfo.Capture(ex);
            return 1;
        }
    }

    private static int SeekStream(IntPtr context, int origin, long offset, out ulong position)
    {
        position = 0;
        StreamContext? state = null;
        try
        {
            state = (StreamContext)GCHandle.FromIntPtr(context).Target!;
            if (state.Call.Failure != null) return 1;
            var seekOrigin = origin switch
            {
                0 => SeekOrigin.Begin,
                1 => SeekOrigin.Current,
                2 => SeekOrigin.End,
                _ => throw new IOException("Invalid native seek origin"),
            };
            position = checked((ulong)state.Stream.Seek(offset, seekOrigin));
            return 0;
        }
        catch (Exception ex)
        {
            if (state != null) state.Call.Failure ??= ExceptionDispatchInfo.Capture(ex);
            return 1;
        }
    }

    private static byte[] StreamOptions(object? options) =>
        Utf8Z(options is string json ? json : JsonSerializer.Serialize(options ?? new { }, JsonOptions));

    /// <summary>Creates an archive from readable streams. The output must be empty, writable and seekable.</summary>
    public static void CreateArchiveFromReaders(Stream archive, IEnumerable<ZarStreamInput> inputs, object? options = null)
    {
        ArgumentNullException.ThrowIfNull(archive);
        ArgumentNullException.ThrowIfNull(inputs);
        var optionsJson = StreamOptions(options);
        using var call = new StreamCall();
        var nativeArchive = call.Add(archive);
        var nativeInputs = new List<NativeInput>();
        try
        {
            foreach (var input in inputs)
            {
                ArgumentNullException.ThrowIfNull(input);
                ArgumentNullException.ThrowIfNull(input.Path);
                Utf8Z(input.Path); // Check for embedded NUL before allocating the native string.
                var nativeStream = call.Add(input.Stream);
                var path = Marshal.StringToCoTaskMemUTF8(input.Path);
                try { nativeInputs.Add(new NativeInput { Path = path, Stream = nativeStream }); }
                catch { Marshal.FreeCoTaskMem(path); throw; }
            }
            var array = nativeInputs.ToArray();
            var status = czarlib_create_archive_from_readers(
                in nativeArchive, array, (nuint)array.Length, optionsJson, out var error);
            call.Complete(status, error);
        }
        finally
        {
            foreach (var input in nativeInputs) Marshal.FreeCoTaskMem(input.Path);
        }
    }

    /// <summary>Extracts one archive path to a writable stream, returning the number of bytes written.</summary>
    public static ulong ExtractSingleFileToWriter(Stream archive, string path, Stream destination, object? options = null)
    {
        ArgumentNullException.ThrowIfNull(archive);
        ArgumentNullException.ThrowIfNull(path);
        ArgumentNullException.ThrowIfNull(destination);
        var nativePath = Utf8Z(path);
        var optionsJson = StreamOptions(options);
        using var call = new StreamCall();
        var nativeArchive = call.Add(archive);
        var nativeDestination = call.Add(destination);
        var status = czarlib_extract_single_file_to_writer(
            in nativeArchive, nativePath, in nativeDestination, optionsJson, out var written, out var error);
        call.Complete(status, error);
        return written;
    }
}
