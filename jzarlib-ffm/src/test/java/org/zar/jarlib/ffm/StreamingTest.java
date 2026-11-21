package org.zar.jarlib.ffm;

import static org.junit.jupiter.api.Assertions.*;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.channels.SeekableByteChannel;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.List;
import java.util.Random;
import org.junit.jupiter.api.Test;

final class StreamingTest {
    private static SeekableByteChannel open(Path path) throws IOException {
        return Files.newByteChannel(path, StandardOpenOption.CREATE, StandardOpenOption.READ,
                StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING);
    }

    @Test
    void streamsShortReadsAndWritesAndEncryptedEntries() throws Exception {
        Path archive = Files.createTempFile(Path.of("").toAbsolutePath(), "stream-", ".zar");
        byte[] data = new byte[250_003];
        new Random(42).nextBytes(data);
        InputStream shortInput = new ByteArrayInputStream(data) {
            @Override public synchronized int read(byte[] b, int off, int len) {
                return super.read(b, off, Math.min(len, 37));
            }
        };
        try {
            try (SeekableByteChannel channel = new ShortChannel(open(archive))) {
                ZarLibrary.createArchiveFromReaders(channel, List.of(
                        new ZarLibrary.StreamInput("dir/dátá.bin", shortInput),
                        new ZarLibrary.StreamInput("empty", new ByteArrayInputStream(new byte[0]))),
                        "{\"encoding_options\":{\"block_size\":65536},\"password\":\"secret\","
                                + "\"aead\":\"chacha20-poly1305\",\"encryption_scope\":\"data\"}");
                ByteArrayOutputStream restored = new ByteArrayOutputStream();
                assertEquals(data.length, ZarLibrary.extractSingleFileToWriter(channel,
                        "dir/dátá.bin", restored, "{\"password\":\"secret\"}"));
                assertArrayEquals(data, restored.toByteArray());
                assertEquals(0, ZarLibrary.extractSingleFileToWriter(channel,
                        "empty", new ByteArrayOutputStream(), "{\"password\":\"secret\"}"));
                assertThrows(ZarException.class, () -> ZarLibrary.extractSingleFileToWriter(
                        channel, "dir/dátá.bin", new ByteArrayOutputStream(), "{\"password\":\"wrong\"}"));
                assertThrows(ZarException.class, () -> ZarLibrary.extractSingleFileToWriter(
                        channel, "missing", new ByteArrayOutputStream(), "{\"password\":\"secret\"}"));
            }
        } finally {
            Files.deleteIfExists(archive);
        }
    }

    @Test
    void preservesCallbackFailuresAndValidatesArguments() throws Exception {
        Path archive = Files.createTempFile(Path.of("").toAbsolutePath(), "stream-", ".zar");
        IOException readFailure = new IOException("input failure");
        IOException writeFailure = new IOException("output failure");
        try {
            try (SeekableByteChannel channel = open(archive)) {
                InputStream broken = new InputStream() {
                    @Override public int read() throws IOException { throw readFailure; }
                    @Override public int read(byte[] b, int off, int len) throws IOException { throw readFailure; }
                };
                ZarException error = assertThrows(ZarException.class, () ->
                        ZarLibrary.createArchiveFromReaders(channel,
                                List.of(new ZarLibrary.StreamInput("broken", broken)), null));
                assertSame(readFailure, error.getCause());
            }
            try (SeekableByteChannel channel = open(archive)) {
                ZarLibrary.createArchiveFromReaders(channel, List.of(new ZarLibrary.StreamInput(
                        "data", new ByteArrayInputStream(new byte[] {1, 2, 3}))), null);
                OutputStream broken = new OutputStream() {
                    @Override public void write(int b) throws IOException { throw writeFailure; }
                    @Override public void write(byte[] b, int off, int len) throws IOException { throw writeFailure; }
                };
                ZarException error = assertThrows(ZarException.class, () ->
                        ZarLibrary.extractSingleFileToWriter(channel, "data", broken, null));
                assertSame(writeFailure, error.getCause());
                assertThrows(ZarException.class, () -> ZarLibrary.extractSingleFileToWriter(
                        channel, "bad\0path", new ByteArrayOutputStream(), null));
                assertThrows(ZarException.class, () -> ZarLibrary.extractSingleFileToWriter(
                        channel, "data", new ByteArrayOutputStream(), "[]"));
                assertThrows(ZarException.class, () -> ZarLibrary.createArchiveFromReaders(
                        channel, List.of(), "{}"));
                IOException seekFailure = new IOException("seek failure");
                SeekableByteChannel brokenSeek = new ShortChannel(channel) {
                    @Override public SeekableByteChannel position(long value) throws IOException {
                        throw seekFailure;
                    }
                };
                error = assertThrows(ZarException.class, () ->
                        ZarLibrary.extractSingleFileToWriter(
                                brokenSeek, "data", new ByteArrayOutputStream(), null));
                assertSame(seekFailure, error.getCause());
            }
        } finally {
            Files.deleteIfExists(archive);
        }
    }

    private static class ShortChannel implements SeekableByteChannel {
        private final SeekableByteChannel delegate;
        ShortChannel(SeekableByteChannel delegate) { this.delegate = delegate; }
        @Override public int read(ByteBuffer dst) throws IOException {
            int limit = dst.limit();
            dst.limit(Math.min(limit, dst.position() + 113));
            try { return delegate.read(dst); } finally { dst.limit(limit); }
        }
        @Override public int write(ByteBuffer src) throws IOException {
            int limit = src.limit();
            src.limit(Math.min(limit, src.position() + 127));
            try { return delegate.write(src); } finally { src.limit(limit); }
        }
        @Override public long position() throws IOException { return delegate.position(); }
        @Override public SeekableByteChannel position(long value) throws IOException {
            delegate.position(value); return this;
        }
        @Override public long size() throws IOException { return delegate.size(); }
        @Override public SeekableByteChannel truncate(long size) throws IOException {
            delegate.truncate(size); return this;
        }
        @Override public boolean isOpen() { return delegate.isOpen(); }
        @Override public void close() throws IOException { delegate.close(); }
    }
}
