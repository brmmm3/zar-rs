package org.zar.jarlib.ffm.examples;

import java.nio.channels.SeekableByteChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.List;
import org.zar.jarlib.ffm.ZarLibrary;

/** Creates an archive from a stream and extracts its entry without buffering the file. */
public final class StreamingExample {
    private StreamingExample() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 3) {
            System.err.println("Usage: StreamingExample ARCHIVE INPUT OUTPUT");
            System.exit(2);
        }
        Path archive = Path.of(args[0]);
        Path input = Path.of(args[1]);
        Path output = Path.of(args[2]);
        try (SeekableByteChannel channel = Files.newByteChannel(archive,
                StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING,
                StandardOpenOption.READ, StandardOpenOption.WRITE);
             var reader = Files.newInputStream(input);
             var writer = Files.newOutputStream(output)) {
            String entry = input.getFileName().toString();
            ZarLibrary.createArchiveFromReaders(channel,
                    List.of(new ZarLibrary.StreamInput(entry, reader)), null);
            long written = ZarLibrary.extractSingleFileToWriter(channel, entry, writer, null);
            System.out.println("Extracted " + written + " bytes");
        }
    }
}
