using System.Text;
using Xunit;
using Zar;

namespace Zar.Tests;

public sealed partial class ZarLibTests
{
    private sealed class ShortReader(byte[] bytes) : MemoryStream(bytes)
    {
        private int _reads;
        public override int Read(byte[] buffer, int offset, int count)
        {
            if (++_reads == 2) GC.Collect();
            return base.Read(buffer, offset, Math.Min(count, 257));
        }
    }

    private sealed class NonSeekableWriter : Stream
    {
        public readonly MemoryStream Contents = new();
        public override bool CanRead => false;
        public override bool CanSeek => false;
        public override bool CanWrite => true;
        public override long Length => throw new NotSupportedException();
        public override long Position { get => throw new NotSupportedException(); set => throw new NotSupportedException(); }
        public override void Flush() => Contents.Flush();
        public override int Read(byte[] buffer, int offset, int count) => throw new NotSupportedException();
        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
        public override void SetLength(long value) => throw new NotSupportedException();
        public override void Write(byte[] buffer, int offset, int count) => Contents.Write(buffer, offset, count);
    }

    private sealed class FailingReader(Exception failure) : MemoryStream(new byte[8])
    {
        public override int Read(byte[] buffer, int offset, int count) => throw failure;
    }

    private sealed class FailingWriter(Exception failure) : MemoryStream
    {
        public override void Write(byte[] buffer, int offset, int count)
        {
            base.Write(buffer, offset, Math.Min(3, count));
            throw failure;
        }
    }

    private sealed class FailingSeek(Exception failure) : MemoryStream
    {
        public override long Seek(long offset, SeekOrigin origin) => throw failure;
    }

    [Fact]
    public void StreamingRoundTripWithShortReadsAndUnicodePaths()
    {
        var binary = Enumerable.Range(0, 180000).Select(i => (byte)(i * 37)).ToArray();
        using var source = new ShortReader(binary);
        using var empty = new MemoryStream();
        using var archive = new MemoryStream();
        ZarLibrary.CreateArchiveFromReaders(archive,
        [
            new("資料/é.bin", source),
            new("empty", empty),
        ], new { encoding_options = new { block_size = 65536, compression_level = 1 } });
        Assert.True(source.CanRead);
        using var destination = new NonSeekableWriter();
        archive.Position = 0;
        Assert.Equal((ulong)binary.Length, ZarLibrary.ExtractSingleFileToWriter(archive, "資料/é.bin", destination));
        Assert.Equal(binary, destination.Contents.ToArray());
        using var emptyDestination = new MemoryStream();
        archive.Position = 0;
        Assert.Equal(0UL, ZarLibrary.ExtractSingleFileToWriter(archive, "empty", emptyDestination));
        Assert.Empty(emptyDestination.ToArray());
        Assert.True(archive.CanRead);
    }

    [Fact]
    public void StreamingEncryptionAndNativeErrors()
    {
        using var source = new MemoryStream(Encoding.UTF8.GetBytes("secret text"));
        using var archive = new MemoryStream();
        ZarLibrary.CreateArchiveFromReaders(archive, [new("secret", source)],
            new { password = "secret", aead = "chacha20-poly1305", encryption_scope = "data" });
        using var destination = new MemoryStream();
        archive.Position = 0;
        Assert.Throws<ZarException>(() => ZarLibrary.ExtractSingleFileToWriter(archive, "secret", destination,
            new { password = "wrong" }));
        archive.Position = 0;
        Assert.Equal(11UL, ZarLibrary.ExtractSingleFileToWriter(archive, "secret", destination,
            new { password = "secret" }));
        Assert.Equal("secret text", Encoding.UTF8.GetString(destination.ToArray()));
        archive.Position = 0;
        Assert.Throws<ZarException>(() => ZarLibrary.ExtractSingleFileToWriter(archive, "missing", destination,
            new { password = "secret" }));
    }

    [Fact]
    public void StreamingPropagatesOriginalStreamFailuresAndKeepsPartialOutput()
    {
        var readFailure = new IOException("source failure");
        using var archive = new MemoryStream();
        using var source = new FailingReader(readFailure);
        Assert.Same(readFailure, Assert.Throws<IOException>(() =>
            ZarLibrary.CreateArchiveFromReaders(archive, [new("file", source)])));
        Assert.True(archive.Length > 0);

        var seekFailure = new IOException("seek failure");
        using var brokenArchive = new FailingSeek(seekFailure);
        Assert.Same(seekFailure, Assert.Throws<IOException>(() =>
            ZarLibrary.CreateArchiveFromReaders(brokenArchive, Array.Empty<ZarStreamInput>())));

        using var goodArchive = new MemoryStream();
        using var data = new MemoryStream(new byte[100]);
        ZarLibrary.CreateArchiveFromReaders(goodArchive, [new("file", data)]);
        goodArchive.Position = 0;
        var writeFailure = new IOException("writer failure");
        using var writer = new FailingWriter(writeFailure);
        Assert.Same(writeFailure, Assert.Throws<IOException>(() =>
            ZarLibrary.ExtractSingleFileToWriter(goodArchive, "file", writer)));
        Assert.Equal(3, writer.Length);
    }

    [Fact]
    public void StreamingRejectsNulPathsAndOptions()
    {
        using var archive = new MemoryStream();
        using var input = new MemoryStream();
        Assert.Throws<ZarException>(() => ZarLibrary.CreateArchiveFromReaders(archive, [new("bad\0path", input)]));
        Assert.Throws<ZarException>(() => ZarLibrary.ExtractSingleFileToWriter(archive, "bad\0path", input));
        Assert.Throws<ZarException>(() => ZarLibrary.CreateArchiveFromReaders(archive, [], "{\0}"));
    }
}
