using System.Text;
using Zar;

// Creates a small archive, inspects it, extracts it, and reads a file back in memory.
var work = Directory.CreateTempSubdirectory("zar-basic-").FullName;
Directory.SetCurrentDirectory(work);
try
{
    Directory.CreateDirectory("input");
    File.WriteAllText("input/hello.txt", "Hello from C#");

    Console.WriteLine(ZarLibrary.CreateArchive(new { archive = "demo.zar", paths = new[] { "input" } }));
    Console.WriteLine(ZarLibrary.ListArchive(new { archive = "demo.zar" }));
    Console.WriteLine(ZarLibrary.CheckArchive(new { archive = "demo.zar" }));
    Console.WriteLine(ZarLibrary.InfoArchive(new { archive = "demo.zar" }));
    ZarLibrary.ExtractArchive(new { archive = "demo.zar", destination = "restored" });

    var files = ZarLibrary.ExtractArchiveToMemory(new { archive = "demo.zar" });
    Console.WriteLine($"in memory: {Encoding.UTF8.GetString(files["input/hello.txt"])}");
}
finally
{
    Directory.SetCurrentDirectory("/");
    Directory.Delete(work, true);
}
