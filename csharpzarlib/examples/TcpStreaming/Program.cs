using System.Buffers.Binary;
using System.Net;
using System.Net.Sockets;
using System.Text;
using Zar;

const int MaxChunk = 65536;

static void WriteLength(Stream stream, int length)
{
    Span<byte> bytes = stackalloc byte[4];
    BinaryPrimitives.WriteInt32BigEndian(bytes, length);
    stream.Write(bytes);
}

static int ReadLength(Stream stream)
{
    Span<byte> bytes = stackalloc byte[4];
    stream.ReadExactly(bytes);
    return BinaryPrimitives.ReadInt32BigEndian(bytes);
}

static void Handle(TcpClient client, string archivePath)
{
    using (client)
    using (var socket = client.GetStream())
    {
        int length = ReadLength(socket);
        if (length <= 0 || length > MaxChunk)
            throw new InvalidDataException("Invalid entry path length");
        var name = new byte[length];
        socket.ReadExactly(name);
        string entry = new UTF8Encoding(false, true).GetString(name);
        using var archive = File.OpenRead(archivePath);
        var count = ZarLibrary.ExtractSingleFileToWriter(archive, entry, new FramedWriter(socket));
        WriteLength(socket, 0);
        Console.WriteLine($"{entry}: {count} bytes");
    }
}

static void Serve(string host, int port, string archive)
{
    var listener = new TcpListener(IPAddress.Parse(host), port);
    listener.Start();
    Console.WriteLine($"Listening on {listener.LocalEndpoint}");
    try
    {
        while (true)
        {
            try { Handle(listener.AcceptTcpClient(), archive); }
            catch (Exception error) when (error is IOException or SocketException
                or ArgumentException or ZarException)
            {
                Console.Error.WriteLine($"Request failed: {error.Message}");
            }
        }
    }
    finally { listener.Stop(); }
}

static void Receive(string host, int port, string entry, string path)
{
    byte[] name = Encoding.UTF8.GetBytes(entry);
    if (name.Length == 0 || name.Length > MaxChunk)
        throw new ArgumentException("Invalid entry path length");
    using var output = new FileStream(path, FileMode.CreateNew, FileAccess.Write);
    try
    {
        using var client = new TcpClient(host, port);
        using var socket = client.GetStream();
        WriteLength(socket, name.Length);
        socket.Write(name);
        byte[] buffer = new byte[MaxChunk];
        while (true)
        {
            int length = ReadLength(socket);
            if (length == 0) break;
            if (length < 0 || length > MaxChunk)
                throw new InvalidDataException("Invalid data frame length");
            socket.ReadExactly(buffer.AsSpan(0, length));
            output.Write(buffer, 0, length);
        }
        output.Flush();
    }
    catch
    {
        output.Dispose();
        File.Delete(path);
        throw;
    }
}

if (args.Length == 4 && args[0] == "server")
    Serve(args[1], int.Parse(args[2]), args[3]);
else if (args.Length == 5 && args[0] == "client")
    Receive(args[1], int.Parse(args[2]), args[3], args[4]);
else
    throw new ArgumentException("usage: TcpStreaming server HOST PORT ARCHIVE | client HOST PORT ENTRY OUTPUT");

sealed class FramedWriter(Stream socket) : Stream
{
    public override bool CanRead => false;
    public override bool CanSeek => false;
    public override bool CanWrite => true;
    public override long Length => throw new NotSupportedException();
    public override long Position { get => throw new NotSupportedException(); set => throw new NotSupportedException(); }
    public override void Flush() => socket.Flush();
    public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
    public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
    public override void SetLength(long value) => throw new NotSupportedException();
    public override void Write(byte[] buffer, int offset, int count)
    {
        Span<byte> header = stackalloc byte[4];
        while (count > 0)
        {
            int length = Math.Min(count, 65536);
            BinaryPrimitives.WriteInt32BigEndian(header, length);
            socket.Write(header);
            socket.Write(buffer, offset, length);
            offset += length;
            count -= length;
        }
    }
}
