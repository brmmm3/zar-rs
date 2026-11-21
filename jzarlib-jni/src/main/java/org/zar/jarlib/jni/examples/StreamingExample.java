package org.zar.jarlib.jni.examples;

import java.io.InputStream;
import java.io.OutputStream;
import java.nio.channels.SeekableByteChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.List;
import org.zar.jarlib.jni.ZarLibrary;

/** Creates an archive from a file stream, then extracts it to another file stream. */
public final class StreamingExample {
    public static void main(String[] args) throws Exception {
        if (args.length != 3) {
            System.err.println("Usage: StreamingExample <input> <new-archive> <output>");
            System.exit(2);
        }
        Path inputPath = Path.of(args[0]);
        String archivePath = inputPath.getFileName().toString();
        try (InputStream input = Files.newInputStream(inputPath);
             SeekableByteChannel archive = Files.newByteChannel(Path.of(args[1]),
                     StandardOpenOption.CREATE_NEW, StandardOpenOption.READ, StandardOpenOption.WRITE)) {
            ZarLibrary.createArchiveFromReaders(archive,
                    List.of(new ZarLibrary.StreamInput(archivePath, input)),
                    "{\"encoding_options\":{\"block_size\":65536}}");
            try (OutputStream output = Files.newOutputStream(Path.of(args[2]),
                    StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE)) {
                long written = ZarLibrary.extractSingleFileToWriter(archive, archivePath, output, "{}");
                System.out.println("Extracted " + written + " bytes");
            }
        }
    }

    private StreamingExample() {}
}
