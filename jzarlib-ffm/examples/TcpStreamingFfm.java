import java.nio.channels.SeekableByteChannel;
import java.nio.file.Files;
import org.zar.jarlib.ffm.ZarLibrary;

public final class TcpStreamingFfm {
    public static void main(String[] args) throws Exception {
        TcpStreaming.run(args, (path, entry, output) -> {
            try (SeekableByteChannel archive = Files.newByteChannel(path)) {
                return ZarLibrary.extractSingleFileToWriter(archive, entry, output, null);
            }
        });
    }
}
