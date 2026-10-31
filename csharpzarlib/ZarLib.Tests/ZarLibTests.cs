using System.Text;
using Xunit;
using Zar;

namespace Zar.Tests;

// All tests live in one class: they change the process working directory, so they must run serially.
public sealed class ZarLibTests : IDisposable
{
    private readonly string _oldDirectory = Directory.GetCurrentDirectory();
    private readonly string _temp = Directory.CreateTempSubdirectory("zar-cs-").FullName;

    public ZarLibTests() => Directory.SetCurrentDirectory(_temp);

    public void Dispose()
    {
        Directory.SetCurrentDirectory(_oldDirectory);
        Directory.Delete(_temp, true);
    }

    private static void Write(string name, byte[] contents)
    {
        var dir = Path.GetDirectoryName(name);
        if (!string.IsNullOrEmpty(dir))
        {
            Directory.CreateDirectory(dir);
        }
        File.WriteAllBytes(name, contents);
    }

    private static void Write(string name, string contents) => Write(name, Encoding.UTF8.GetBytes(contents));

    private static void Fixture()
    {
        Write("input/alpha.txt", "alpha contents");
        Write("input/nested/beta.bin", Enumerable.Range(0, 128).Select(i => (byte)i).ToArray());
        Write("input/empty.txt", Array.Empty<byte>());
    }

    private static void ExpectError(Func<string> action, string fragment)
    {
        var ex = Assert.Throws<ZarException>(action);
        Assert.Contains(fragment, ex.Message);
    }

    private static void AssertFile(Dictionary<string, byte[]> files, string path, string expected)
    {
        Assert.True(files.ContainsKey(path), $"memory extraction omitted {path}");
        Assert.Equal(expected, Encoding.UTF8.GetString(files[path]));
    }

    [Fact]
    public void FullArchiveOperations()
    {
        Fixture();
        var created = ZarLibrary.CreateArchive(new { archive = "sample.zar", paths = new[] { "input" }, block_size = 65536 });
        Assert.Contains("\"block_size\":65536", created);

        var listing = ZarLibrary.ListArchive(new { archive = "sample.zar" });
        Assert.Contains("\"path\":\"input/alpha.txt\"", listing);
        Assert.Contains("\"path\":\"input/nested\"", listing);
        Assert.Contains("\"path\":\"input/empty.txt\"", listing);
        Assert.Contains("\"size\":14", listing);

        var report = ZarLibrary.CheckArchive(new { archive = "sample.zar" });
        Assert.Contains("\"commits\":1", report);
        Assert.Contains("\"logical_state_readable\":true", report);
        Assert.Contains("\"encrypted_blocks\":0", report);
        Assert.Contains("\"id\":\"00000001\"", ZarLibrary.InfoArchive(new { archive = "sample.zar" }));

        var plan = ZarLibrary.ExtractArchive(new { archive = "sample.zar", destination = "dry", dry_run = true });
        Assert.Contains("\"dry_run\":true", plan);
        Assert.Contains("dry/input/alpha.txt", plan);
        Assert.False(Directory.Exists("dry"));
        ZarLibrary.ExtractArchive(new { archive = "sample.zar", destination = "restored" });
        Assert.Equal("alpha contents", File.ReadAllText("restored/input/alpha.txt"));

        Write("keep.txt", "keep");
        Write("drop.bin", "drop");
        ExpectError(() => ZarLibrary.CreateArchive(new { archive = "bad-size.zar", paths = new[] { "keep.txt" }, block_size = 1 }), "block_size");
        ExpectError(() => ZarLibrary.CreateArchive(new { archive = "bad-level.zar", paths = new[] { "keep.txt" }, compression_level = 23 }), "compression_level");
        ZarLibrary.CreateArchive(new { archive = "filtered.zar", paths = new[] { "keep.txt", "drop.bin" }, block_size = 65536, include_files = new[] { "*.txt" } });
        listing = ZarLibrary.ListArchive(new { archive = "filtered.zar" });
        Assert.Contains("\"path\":\"keep.txt\"", listing);
        Assert.DoesNotContain("\"path\":\"drop.bin\"", listing);
        ExpectError(() => ZarLibrary.CreateArchive(new { archive = "filtered.zar", paths = new[] { "keep.txt" }, block_size = 65536 }), "already exists");
        ZarLibrary.CreateArchive(new { archive = "filtered.zar", paths = new[] { "keep.txt", "drop.bin" }, block_size = 65536, overwrite = true });

        Write("input/alpha.txt", "replaced");
        Write("input/nested/new.txt", "new");
        ZarLibrary.AddArchive(new { archive = "sample.zar", paths = new[] { "input/alpha.txt", "input/nested/new.txt" } });
        var memory = ZarLibrary.ExtractArchiveToMemory(new { archive = "sample.zar" });
        AssertFile(memory, "input/alpha.txt", "replaced");
        AssertFile(memory, "input/nested/new.txt", "new");
        Assert.DoesNotContain("input/nested/new.txt", ZarLibrary.ListArchive(new { archive = "sample.zar", through = "00000001" }));
        ExpectError(() => ZarLibrary.ListArchive(new { archive = "sample.zar", through = "ffffffff" }), "not found");

        var before = File.ReadAllBytes("sample.zar");
        ExpectError(() => ZarLibrary.RemoveArchive(new { archive = "sample.zar", paths = new[] { "missing" } }), "not found");
        Assert.Equal(before, File.ReadAllBytes("sample.zar"));
        ZarLibrary.RemoveArchive(new { archive = "sample.zar", paths = new[] { "input/nested" } });
        listing = ZarLibrary.ListArchive(new { archive = "sample.zar" });
        Assert.DoesNotContain("input/nested", listing);
        Assert.Contains("input/alpha.txt", listing);
        Assert.Contains("\"commits\":3", ZarLibrary.CheckArchive(new { archive = "sample.zar" }));

        Write("input/one.txt", "one");
        ZarLibrary.AddArchive(new { archive = "sample.zar", paths = new[] { "input/one.txt" } });
        Write("input/two.txt", "two");
        ZarLibrary.AddArchive(new { archive = "sample.zar", paths = new[] { "input/two.txt" } });
        Assert.Contains("\"id\":\"00000005\"", ZarLibrary.InfoArchive(new { archive = "sample.zar" }));
        ZarLibrary.JoinArchive(new { archive = "sample.zar", from_id = "00000003", through_id = "00000004" });
        listing = ZarLibrary.ListArchive(new { archive = "sample.zar" });
        Assert.Contains("input/one.txt", listing);
        Assert.Contains("input/two.txt", listing);
        Assert.Contains("\"id\":\"00000005\"", ZarLibrary.InfoArchive(new { archive = "sample.zar" }));

        Write("existing/input/alpha.txt", "keep existing");
        ExpectError(() => ZarLibrary.ExtractArchive(new { archive = "sample.zar", destination = "existing" }), "already exists");
        ZarLibrary.ExtractArchive(new { archive = "sample.zar", destination = "existing", existing = "skip" });
        Assert.Equal("keep existing", File.ReadAllText("existing/input/alpha.txt"));
        ZarLibrary.ExtractArchive(new { archive = "sample.zar", destination = "existing", existing = "overwrite" });
        Assert.Equal("replaced", File.ReadAllText("existing/input/alpha.txt"));

        plan = ZarLibrary.ExtractArchive(new { archive = "sample.zar", destination = "filtered-dry", dry_run = true, include_files = new[] { "*.txt" } });
        Assert.Contains("input/alpha.txt", plan);
        Assert.DoesNotContain("beta.bin", plan);
        memory = ZarLibrary.ExtractArchiveToMemory(new
        {
            archive = "sample.zar", offset = 1, size_limit = 3,
            include_files = new[] { "*.txt" }, exclude_files = new[] { "input/empty.txt" },
        });
        AssertFile(memory, "input/alpha.txt", "epl");
        memory = ZarLibrary.ExtractArchiveToMemory(new { archive = "sample.zar", offset = 1000 });
        AssertFile(memory, "input/alpha.txt", "");
        AssertFile(memory, "input/empty.txt", "");
    }

    [Fact]
    public void EncryptionAndPerCommitPasswords()
    {
        Fixture();
        Write("keep.txt", "keep");
        var created = ZarLibrary.CreateArchive(new { archive = "encrypted.zar", paths = new[] { "input/alpha.txt" }, block_size = 65536, password = "secret" });
        Assert.Contains("\"aead\":\"aes-256-gcm\"", created);
        Assert.Contains("\"commits\":1", ZarLibrary.CheckArchive(new { archive = "encrypted.zar", password = "secret" }));
        Assert.Contains("input/alpha.txt", ZarLibrary.ListArchive(new { archive = "encrypted.zar", password = "secret" }));
        AssertFile(ZarLibrary.ExtractArchiveToMemory(new { archive = "encrypted.zar", password = "secret" }), "input/alpha.txt", "alpha contents");
        ExpectError(() => ZarLibrary.CheckArchive(new { archive = "encrypted.zar", password = "wrong" }), "authentication");
        ExpectError(() => ZarLibrary.ExtractArchive(new { archive = "encrypted.zar", destination = "locked" }), "password");
        Assert.False(Directory.Exists("locked"));

        var before = File.ReadAllBytes("encrypted.zar");
        ExpectError(() => ZarLibrary.AddArchive(new { archive = "encrypted.zar", paths = new[] { "keep.txt" } }), "encrypted");
        Assert.Equal(before, File.ReadAllBytes("encrypted.zar"));

        created = ZarLibrary.CreateArchive(new { archive = "chacha.zar", paths = new[] { "input/alpha.txt" }, block_size = 65536, password = "secret", aead = "chacha20-poly1305" });
        Assert.Contains("\"aead\":\"chacha20-poly1305\"", created);
        ZarLibrary.CheckArchive(new { archive = "chacha.zar", password = "secret" });
        ZarLibrary.ExtractArchive(new { archive = "chacha.zar", destination = "chacha-restored", password = "secret" });
        ZarLibrary.AddArchive(new
        {
            archive = "chacha.zar", paths = new[] { "input/alpha.txt" }, block_size = 65536,
            password = "second-secret", passwords = new Dictionary<string, string> { ["00000001"] = "secret" },
            aead = "aes-256-gcm", encryption_scope = "data",
        });
        var memory = ZarLibrary.ExtractArchiveToMemory(new
        {
            archive = "chacha.zar",
            passwords = new Dictionary<string, string> { ["00000001"] = "secret", ["00000002"] = "second-secret" },
        });
        AssertFile(memory, "input/alpha.txt", "alpha contents");
        Assert.Contains("\"data_encrypted\":true", ZarLibrary.InfoArchive(new { archive = "chacha.zar" }));
    }

    [Fact]
    public void ConcurrentCalls()
    {
        Parallel.For(0, 4, new ParallelOptions { MaxDegreeOfParallelism = 4 }, index =>
        {
            var source = $"thread-{index}.bin";
            var archive = $"thread-{index}.zar";
            var contents = $"payload-{index}";
            Write(source, contents);
            ZarLibrary.CreateArchive(new { archive, paths = new[] { source }, block_size = 65536 });
            Assert.Contains("\"commits\":1", ZarLibrary.CheckArchive(new { archive }));
            AssertFile(ZarLibrary.ExtractArchiveToMemory(new { archive }), source, contents);
        });
    }

    [Theory]
    [InlineData("", "{}", "operation must not be empty")]
    [InlineData("info_archive", "{", "invalid request JSON")]
    [InlineData("info_archive", "[]", "must be an object")]
    [InlineData("unknown", "{}", "unknown operation")]
    [InlineData("info_archive\0", "{}", "NUL")]
    public void ExecuteJsonValidation(string operation, string request, string fragment)
    {
        ExpectError(() => ZarLibrary.ExecuteJson(operation, request), fragment);
    }
}
