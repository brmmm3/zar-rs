package org.zar.jarlib.jni;

import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.Objects;

/** JNI binding to the czarlib JSON operation API. */
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

    public static String joinArchive(String requestJson) throws ZarException {
        return executeJson("join_archive", requestJson);
    }

    public static String checkArchive(String requestJson) throws ZarException {
        return executeJson("check_archive", requestJson);
    }

    public static String infoArchive(String requestJson) throws ZarException {
        return executeJson("info_archive", requestJson);
    }

    public static String listArchive(String requestJson) throws ZarException {
        return executeJson("list_archive", requestJson);
    }

    public static String extractArchive(String requestJson) throws ZarException {
        return executeJson("extract_archive", requestJson);
    }

    public static String extractArchiveToMemory(String requestJson) throws ZarException {
        return executeJson("extract_archive_to_memory", requestJson);
    }
}
