using System.Text;
using Zar;

using var source = new MemoryStream(Encoding.UTF8.GetBytes("Hello from a stream!"));
using var archive = new MemoryStream();
ZarLibrary.CreateArchiveFromReaders(archive, [new("greetings/こんにちは.txt", source)],
    new { encoding_options = new { block_size = 65536 } });

archive.Position = 0;
using var output = new MemoryStream();
var written = ZarLibrary.ExtractSingleFileToWriter(archive, "greetings/こんにちは.txt", output);
Console.WriteLine($"{written} bytes: {Encoding.UTF8.GetString(output.ToArray())}");
