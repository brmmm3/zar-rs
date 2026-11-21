package org.zar.jarlib.jni;

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
    private static final String OPTIONS = "{\"encoding_options\":{\"block_size\":65536}}";

    private static SeekableByteChannel channel(Path path) throws IOException {
        return Files.newByteChannel(path, StandardOpenOption.CREATE, StandardOpenOption.READ,
                StandardOpenOption.WRITE, StandardOpenOption.TRUNCATE_EXISTING);
    }

    private static class ShortChannel implements SeekableByteChannel {
        final SeekableByteChannel delegate;
        ShortChannel(SeekableByteChannel delegate) { this.delegate = delegate; }
        public int read(ByteBuffer dst) throws IOException {
            int limit = dst.limit();
            dst.limit(Math.min(limit, dst.position() + 37));
            try { return delegate.read(dst); } finally { dst.limit(limit); }
        }
        public int write(ByteBuffer src) throws IOException {
            int limit = src.limit();
            src.limit(Math.min(limit, src.position() + 41));
            try { return delegate.write(src); } finally { src.limit(limit); }
        }
        public long position() throws IOException { return delegate.position(); }
        public SeekableByteChannel position(long p) throws IOException { delegate.position(p); return this; }
        public long size() throws IOException { return delegate.size(); }
        public SeekableByteChannel truncate(long n) throws IOException { delegate.truncate(n); return this; }
        public boolean isOpen() { return delegate.isOpen(); }
        public void close() throws IOException { delegate.close(); }
    }

    @Test
    void streamsShortIoBinaryUnicodeAndEmptyFiles() throws Exception {
        Path file = Files.createTempFile(Path.of("."), "stream-test-", ".zar");
        try (SeekableByteChannel archive = new ShortChannel(channel(file))) {
            byte[] data = new byte[150_000];
            new Random(42).nextBytes(data);
            InputStream shortInput = new ByteArrayInputStream(data) {
                @Override public synchronized int read(byte[] b, int off, int len) {
                    return super.read(b, off, Math.min(len, 113));
                }
            };
            String name = "dir/日本語-😀.bin";
            ZarLibrary.createArchiveFromReaders(archive, List.of(
                    new ZarLibrary.StreamInput(name, shortInput),
                    new ZarLibrary.StreamInput("empty", new ByteArrayInputStream(new byte[0]))), OPTIONS);
            assertTrue(archive.isOpen());
            ByteArrayOutputStream result = new ByteArrayOutputStream();
            assertEquals(data.length, ZarLibrary.extractSingleFileToWriter(archive, name, result, "{}"));
            assertArrayEquals(data, result.toByteArray());
            assertEquals(0, ZarLibrary.extractSingleFileToWriter(archive, "empty",
                    new ByteArrayOutputStream(), "{}"));
            assertThrows(ZarException.class, () -> ZarLibrary.extractSingleFileToWriter(
                    archive, "missing", new ByteArrayOutputStream(), "{}"));
        } finally {
            Files.deleteIfExists(file);
        }
    }

    @Test
    void encryptedArchiveAndBadOptions() throws Exception {
        Path file = Files.createTempFile(Path.of("."), "stream-test-", ".zar");
        try (SeekableByteChannel archive = channel(file)) {
            ZarLibrary.createArchiveFromReaders(archive, List.of(
                    new ZarLibrary.StreamInput("x", new ByteArrayInputStream(new byte[] {0, -1, 42}))),
                    "{\"encoding_options\":{\"block_size\":65536},\"password\":\"秘密😀\","
                            + "\"aead\":\"chacha20-poly1305\",\"encryption_scope\":\"header-and-data\"}");
            assertThrows(ZarException.class, () -> ZarLibrary.extractSingleFileToWriter(
                    archive, "x", new ByteArrayOutputStream(), "{\"password\":\"wrong\"}"));
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            assertEquals(3, ZarLibrary.extractSingleFileToWriter(
                    archive, "x", output, "{\"password\":\"秘密😀\"}"));
            assertArrayEquals(new byte[] {0, -1, 42}, output.toByteArray());
            assertThrows(ZarException.class, () -> ZarLibrary.extractSingleFileToWriter(
                    archive, "x", output, "{invalid"));
            assertThrows(IllegalArgumentException.class, () -> ZarLibrary.extractSingleFileToWriter(
                    archive, "x\0y", output, "{}"));
            assertThrows(IllegalArgumentException.class, () -> ZarLibrary.extractSingleFileToWriter(
                    archive, "\ud800", output, "{}"));
        } finally {
            Files.deleteIfExists(file);
        }
    }

    @Test
    void preservesCallbackExceptions() throws Exception {
        Path file = Files.createTempFile(Path.of("."), "stream-test-", ".zar");
        try (SeekableByteChannel archive = channel(file)) {
            IOException readFailure = new IOException("read failure");
            InputStream failed = new InputStream() {
                public int read() throws IOException { throw readFailure; }
            };
            assertSame(readFailure, assertThrows(IOException.class, () ->
                    ZarLibrary.createArchiveFromReaders(archive,
                            List.of(new ZarLibrary.StreamInput("x", failed)), OPTIONS)));
            archive.truncate(0).position(0);
            ZarLibrary.createArchiveFromReaders(archive, List.of(
                    new ZarLibrary.StreamInput("x", new ByteArrayInputStream(new byte[] {1, 2}))), OPTIONS);
            IOException writeFailure = new IOException("write failure");
            OutputStream failedOutput = new OutputStream() {
                public void write(int b) throws IOException { throw writeFailure; }
            };
            assertSame(writeFailure, assertThrows(IOException.class, () ->
                    ZarLibrary.extractSingleFileToWriter(archive, "x", failedOutput, "{}")));
            RuntimeException seekFailure = new IllegalStateException("seek failure");
            SeekableByteChannel failedSeek = new ShortChannel(archive) {
                @Override public SeekableByteChannel position(long p) { throw seekFailure; }
            };
            assertSame(seekFailure, assertThrows(IllegalStateException.class, () ->
                    ZarLibrary.extractSingleFileToWriter(failedSeek, "x", new ByteArrayOutputStream(), "{}")));
            AssertionError fatal = new AssertionError("callback error");
            OutputStream errorOutput = new OutputStream() {
                public void write(int b) { throw fatal; }
            };
            assertSame(fatal, assertThrows(AssertionError.class, () ->
                    ZarLibrary.extractSingleFileToWriter(archive, "x", errorOutput, "{}")));
            InputStream noProgress = new InputStream() {
                public int read() { return 0; }
                @Override public int read(byte[] b, int off, int len) { return 0; }
            };
            archive.truncate(0).position(0);
            assertThrows(IOException.class, () -> ZarLibrary.createArchiveFromReaders(archive,
                    List.of(new ZarLibrary.StreamInput("x", noProgress)), OPTIONS));
        } finally {
            Files.deleteIfExists(file);
        }
    }
}
