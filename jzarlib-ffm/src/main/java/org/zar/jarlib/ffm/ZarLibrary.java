package org.zar.jarlib.ffm;

import java.lang.foreign.Arena;
import java.lang.foreign.AddressLayout;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.file.Path;
import java.util.Objects;

/** Java 25+ FFM binding to the czarlib JSON operation API. */
public final class ZarLibrary {
    private static final AddressLayout ADDRESS = ValueLayout.ADDRESS;
    private static final Linker LINKER = Linker.nativeLinker();
    private static final Arena LIBRARY_ARENA = Arena.ofShared();
    private static final SymbolLookup SYMBOLS = loadLibrary();
    private static final MethodHandle EXECUTE = downcall(
            "czarlib_execute_json",
            FunctionDescriptor.of(ValueLayout.JAVA_INT, ADDRESS, ADDRESS, ADDRESS, ADDRESS));
    private static final MethodHandle FREE = downcall(
            "czarlib_string_free",
            FunctionDescriptor.ofVoid(ADDRESS));

    private ZarLibrary() {}

    private static SymbolLookup loadLibrary() {
        String configured = System.getProperty("zar.library");
        Path library = configured == null || configured.isBlank()
                ? Path.of("..", "target", "debug", System.mapLibraryName("czarlib"))
                        .toAbsolutePath()
                : Path.of(configured).toAbsolutePath();
        return SymbolLookup.libraryLookup(library, LIBRARY_ARENA);
    }

    private static MethodHandle downcall(String symbol, FunctionDescriptor descriptor) {
        MemorySegment address = SYMBOLS.find(symbol)
                .orElseThrow(() -> new ExceptionInInitializerError(
                        "missing czarlib symbol: " + symbol));
        return LINKER.downcallHandle(address, descriptor);
    }

    /**
     * Executes an operation with a JSON object request and returns its JSON
     * response. All operation names and fields match the czarlib C API.
     */
    public static String executeJson(String operation, String requestJson)
            throws ZarException {
        Objects.requireNonNull(operation, "operation");
        Objects.requireNonNull(requestJson, "requestJson");
        if (operation.isEmpty() || operation.indexOf('\0') >= 0
                || requestJson.indexOf('\0') >= 0) {
            throw new ZarException("operation and request must be non-empty C-compatible strings");
        }

        try (Arena arena = Arena.ofConfined()) {
            MemorySegment responseSlot = arena.allocate(ADDRESS);
            MemorySegment errorSlot = arena.allocate(ADDRESS);
            responseSlot.set(ADDRESS, 0, MemorySegment.NULL);
            errorSlot.set(ADDRESS, 0, MemorySegment.NULL);
            int status = (int) EXECUTE.invokeExact(
                    arena.allocateFrom(operation),
                    arena.allocateFrom(requestJson),
                    responseSlot,
                    errorSlot);
            MemorySegment response = responseSlot.get(ADDRESS, 0);
            MemorySegment error = errorSlot.get(ADDRESS, 0);
            try {
                if (status == 0) {
                    if (response.equals(MemorySegment.NULL)) {
                        throw new ZarException("czarlib returned success without a response");
                    }
                    return response.reinterpret(Long.MAX_VALUE).getString(0);
                }
                String message = error.equals(MemorySegment.NULL)
                        ? "czarlib operation failed without an error message"
                        : error.reinterpret(Long.MAX_VALUE).getString(0);
                throw new ZarException(switch (status) {
                    case 1 -> message;
                    case 2 -> "czarlib rejected its arguments or could not allocate output";
                    case 3 -> "czarlib caught a Rust panic: " + message;
                    default -> "czarlib returned unknown status " + status + ": " + message;
                });
            } finally {
                if (!response.equals(MemorySegment.NULL)) {
                    FREE.invokeExact(response);
                }
                if (!error.equals(MemorySegment.NULL)) {
                    FREE.invokeExact(error);
                }
            }
        } catch (ZarException exception) {
            throw exception;
        } catch (Throwable throwable) {
            throw new ZarException("FFM call to czarlib failed", throwable);
        }
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
