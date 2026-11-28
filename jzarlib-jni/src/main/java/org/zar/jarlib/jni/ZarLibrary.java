package org.zar.jarlib.jni;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.CharBuffer;
import java.nio.charset.StandardCharsets;
import java.nio.charset.CharacterCodingException;
import java.nio.channels.SeekableByteChannel;
import java.nio.file.Path;
import java.util.List;
import java.util.Objects;

/** JNI binding to the czarlib archive APIs. */
public final class ZarLibrary {
    static {
        String configured = System.getProperty("zar.jni.library");
        String library = configured == null || configured.isBlank()
                ? "target/native/" + System.mapLibraryName("zarlib_jni")
                : configured;
        System.load(Path.of(library).toAbsolutePath().normalize().toString());
    }

    private ZarLibrary() {}

    private static native String[] executeNative(byte[] operation, byte[] requestJson);

    private static native String createFromReadersNative(
            StreamBridge archive, byte[][] paths, StreamBridge[] inputs, byte[] options);
    private static native String extractToWriterNative(
            StreamBridge archive, byte[] path, StreamBridge output, byte[] options, long[] written);

    /** A regular file to archive; its stream is consumed but not closed. */
    public record StreamInput(String path, InputStream input) {
        public StreamInput {
            Objects.requireNonNull(path, "path");
            Objects.requireNonNull(input, "input");
        }
    }

    /**
     * Creates an archive in an empty, seekable channel from regular-file streams.
     * The channel must support reading, writing and seeking. No resource is closed or flushed.
     * On failure the destination may contain a partial archive.
     */
    public static void createArchiveFromReaders(SeekableByteChannel archive,
            List<StreamInput> inputs, String optionsJson) throws IOException, ZarException {
        Objects.requireNonNull(archive, "archive");
        Objects.requireNonNull(inputs, "inputs");
        byte[][] paths = new byte[inputs.size()][];
        StreamBridge[] streams = new StreamBridge[inputs.size()];
        for (int i = 0; i < inputs.size(); i++) {
            StreamInput input = Objects.requireNonNull(inputs.get(i), "input");
            paths[i] = cString(input.path(), "path");
            streams[i] = new StreamBridge(input.input());
        }
        String error = createFromReadersNative(new StreamBridge(archive), paths, streams,
                cString(optionsJson, "optionsJson"));
        if (error != null) throw new ZarException(error);
    }

    /**
     * Extracts one archived file to a stream and returns its byte count.
     * The archive must be readable and seekable. No resource is closed or flushed.
     * On failure the output may contain partial data.
     */
    public static long extractSingleFileToWriter(SeekableByteChannel archive, String path,
            OutputStream output, String optionsJson) throws IOException, ZarException {
        Objects.requireNonNull(archive, "archive");
        Objects.requireNonNull(output, "output");
        long[] written = new long[1];
        String error = extractToWriterNative(new StreamBridge(archive), cString(path, "path"),
                new StreamBridge(output), cString(optionsJson, "optionsJson"), written);
        if (error != null) throw new ZarException(error);
        return written[0];
    }

    private static byte[] cString(String value, String name) {
        Objects.requireNonNull(value, name);
        if (value.indexOf('\0') >= 0) throw new IllegalArgumentException(name + " contains NUL");
        try {
            ByteBuffer encoded = StandardCharsets.UTF_8.newEncoder().encode(CharBuffer.wrap(value));
            byte[] result = new byte[encoded.remaining()];
            encoded.get(result);
            return result;
        } catch (CharacterCodingException e) {
            throw new IllegalArgumentException(name + " contains invalid Unicode", e);
        }
    }

    /* Calls are synchronous. Each bridge and its resources remain owned by Java. */
    private static final class StreamBridge {
        private final InputStream input;
        private final OutputStream output;
        private final SeekableByteChannel channel;

        StreamBridge(InputStream input) { this.input = input; output = null; channel = null; }
        StreamBridge(OutputStream output) { input = null; this.output = output; channel = null; }
        StreamBridge(SeekableByteChannel channel) { input = null; output = null; this.channel = channel; }

        private int read(byte[] bytes) throws IOException {
            int n = input != null ? input.read(bytes) : channel.read(ByteBuffer.wrap(bytes));
            if (n == 0 && bytes.length != 0) throw new IOException("stream read made no progress");
            return n < 0 ? 0 : n;
        }

        private int write(byte[] bytes) throws IOException {
            if (output != null) {
                output.write(bytes);
                return bytes.length;
            }
            int n = channel.write(ByteBuffer.wrap(bytes));
            if (n == 0 && bytes.length != 0) throw new IOException("stream write made no progress");
            return n;
        }

        private long seek(int origin, long offset) throws IOException {
            long base = switch (origin) {
                case 0 -> 0;
                case 1 -> channel.position();
                case 2 -> channel.size();
                default -> throw new IOException("invalid seek origin");
            };
            try {
                long position = Math.addExact(base, offset);
                if (position < 0) throw new IOException("negative seek position");
                channel.position(position);
                return channel.position();
            } catch (ArithmeticException e) {
                throw new IOException("seek position overflow", e);
            }
        }
    }

    /** Executes an operation with a JSON object request and returns its JSON response. */
    public static String executeJson(String operation, String requestJson)
            throws ZarException {
        Objects.requireNonNull(operation, "operation");
        Objects.requireNonNull(requestJson, "requestJson");
        if (operation.isEmpty() || operation.indexOf('\0') >= 0
                || requestJson.indexOf('\0') >= 0) {
            throw new ZarException("operation and request must be non-empty C-compatible strings");
        }
        String[] result = executeNative(
                operation.getBytes(StandardCharsets.UTF_8),
                requestJson.getBytes(StandardCharsets.UTF_8));
        if (result == null || result.length != 2) {
            throw new ZarException("JNI bridge returned an invalid result");
        }
        if (result[1] != null) {
            throw new ZarException(result[1]);
        }
        if (result[0] == null) {
            throw new ZarException("JNI bridge returned success without a response");
        }
        return result[0];
    }

    public static String createArchive(String requestJson) throws ZarException {
        return executeJson("create_archive", requestJson);
    }

    public static String addArchive(String requestJson) throws ZarException {
        return executeJson("add_archive", requestJson);
    }

    public static String removeArchive(String requestJson) throws ZarException {
        return executeJson("remove_archive", requestJson);
    }

    public static String readCustomMetadata(String requestJson) throws ZarException {
        return executeJson("read_custom_metadata", requestJson);
    }

    public static String updateCustomMetadata(String requestJson) throws ZarException {
        return executeJson("update_custom_metadata", requestJson);
    }

    public static String joinArchive(String requestJson) throws ZarException {
        return executeJson("join_archive", requestJson);
    }

    public static String undoArchive(String requestJson) throws ZarException {
        return executeJson("undo_archive", requestJson);
    }

    public static String encryptArchiveCommits(String requestJson) throws ZarException {
        return executeJson("encrypt_archive_commits", requestJson);
    }

    public static String decryptArchiveCommits(String requestJson) throws ZarException {
        return executeJson("decrypt_archive_commits", requestJson);
    }

    public static String checkArchive(String requestJson) throws ZarException {
        return executeJson("check_archive", requestJson);
    }

    public static String infoArchive(String requestJson) throws ZarException {
        return executeJson("info_archive", requestJson);
    }

    public static String crossCommitReferences(String requestJson) throws ZarException {
        return executeJson("cross_commit_references", requestJson);
    }

    public static String listArchive(String requestJson) throws ZarException {
        return executeJson("list_archive", requestJson);
    }

    public static String diffArchive(String requestJson) throws ZarException {
        return executeJson("diff_archive", requestJson);
    }

    public static String grepArchive(String requestJson) throws ZarException {
        return executeJson("grep_archive", requestJson);
    }

    public static String extractArchive(String requestJson) throws ZarException {
        return executeJson("extract_archive", requestJson);
    }

    public static String extractArchiveToMemory(String requestJson) throws ZarException {
        return executeJson("extract_archive_to_memory", requestJson);
    }
}
