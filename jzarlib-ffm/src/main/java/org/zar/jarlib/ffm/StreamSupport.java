package org.zar.jarlib.ffm;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.lang.foreign.AddressLayout;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemoryLayout;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.nio.channels.SeekableByteChannel;
import java.util.List;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicReference;

final class StreamSupport {
    private static final Linker LINKER = Linker.nativeLinker();
    private static final ValueLayout SIZE = (ValueLayout) LINKER.canonicalLayouts().get("size_t");
    private static final AddressLayout ADDRESS = ValueLayout.ADDRESS;
    private static final MemoryLayout STREAM = MemoryLayout.structLayout(
            ADDRESS.withName("context"), ADDRESS.withName("read"),
            ADDRESS.withName("write"), ADDRESS.withName("seek"));
    private static final MemoryLayout INPUT = MemoryLayout.structLayout(
            ADDRESS.withName("path"), STREAM.withName("stream"));
    private static final long INPUT_STREAM = INPUT.byteOffset(MemoryLayout.PathElement.groupElement("stream"));
    private static final FunctionDescriptor IO_DESCRIPTOR = FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ADDRESS, ADDRESS, SIZE, ADDRESS);
    private static final FunctionDescriptor SEEK_DESCRIPTOR = FunctionDescriptor.of(
            ValueLayout.JAVA_INT, ADDRESS, ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG, ADDRESS);
    private static final int BUFFER_SIZE = 64 * 1024;

    private StreamSupport() {}

    static void create(SeekableByteChannel archive, List<ZarLibrary.StreamInput> inputs,
            String options) throws ZarException {
        Objects.requireNonNull(archive, "archive");
        List<ZarLibrary.StreamInput> entries = List.copyOf(inputs);
        cStringCheck(options, "options");
        for (var entry : entries) cStringCheck(entry.path(), "path");
        try {
            if (archive.size() != 0 || archive.position() != 0) {
                throw new ZarException("archive channel must be empty and positioned at zero");
            }
        } catch (IOException e) {
            throw new ZarException("cannot inspect archive channel", e);
        }
        try (Arena arena = Arena.ofShared()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            MemorySegment target = stream(arena, new Callbacks(archive, null, null, failure));
            MemorySegment array = entries.isEmpty() ? MemorySegment.NULL
                    : arena.allocate(INPUT, entries.size());
            for (int i = 0; i < entries.size(); i++) {
                var entry = entries.get(i);
                MemorySegment item = array.asSlice(i * INPUT.byteSize(), INPUT.byteSize());
                item.set(ADDRESS, 0, arena.allocateFrom(entry.path()));
                MemorySegment source = stream(arena, new Callbacks(null, entry.stream(), null, failure));
                MemorySegment.copy(source, 0, item, INPUT_STREAM, STREAM.byteSize());
            }
            MemorySegment error = arena.allocate(ADDRESS);
            MethodHandle call = ZarLibrary.downcall("czarlib_create_archive_from_readers",
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ADDRESS, ADDRESS, SIZE, ADDRESS, ADDRESS));
            MemorySegment opts = cString(arena, options);
            int status = SIZE.byteSize() == Long.BYTES
                    ? (int) call.invokeExact(target, array, (long) entries.size(), opts, error)
                    : (int) call.invokeExact(target, array, entries.size(), opts, error);
            check(status, error, failure);
        } catch (ZarException e) {
            throw e;
        } catch (Throwable e) {
            throw new ZarException("FFM streaming call failed", e);
        }
    }

    static long extract(SeekableByteChannel archive, String path, OutputStream output,
            String options) throws ZarException {
        Objects.requireNonNull(archive, "archive");
        Objects.requireNonNull(path, "path");
        Objects.requireNonNull(output, "output");
        cStringCheck(path, "path");
        cStringCheck(options, "options");
        try (Arena arena = Arena.ofShared()) {
            AtomicReference<Throwable> failure = new AtomicReference<>();
            MemorySegment source = stream(arena, new Callbacks(archive, null, null, failure));
            MemorySegment target = stream(arena, new Callbacks(null, null, output, failure));
            MemorySegment written = arena.allocate(ValueLayout.JAVA_LONG);
            MemorySegment error = arena.allocate(ADDRESS);
            MethodHandle call = ZarLibrary.downcall("czarlib_extract_single_file_to_writer",
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ADDRESS, ADDRESS, ADDRESS,
                            ADDRESS, ADDRESS, ADDRESS));
            int status = (int) call.invokeExact(source, arena.allocateFrom(path), target,
                    cString(arena, options), written, error);
            check(status, error, failure);
            long count = written.get(ValueLayout.JAVA_LONG, 0);
            if (count < 0) throw new ZarException("extracted byte count exceeds Java long");
            return count;
        } catch (ZarException e) {
            throw e;
        } catch (Throwable e) {
            throw new ZarException("FFM streaming call failed", e);
        }
    }

    private static void cStringCheck(String value, String name) throws ZarException {
        if (value != null && value.indexOf('\0') >= 0)
            throw new ZarException(name + " contains a NUL character");
    }

    private static MemorySegment cString(Arena arena, String value) {
        return arena.allocateFrom(value == null ? "{}" : value);
    }

    private static void check(int status, MemorySegment errorSlot,
            AtomicReference<Throwable> failure) throws Throwable {
        MemorySegment error = errorSlot.get(ADDRESS, 0);
        try {
            Throwable cause = failure.get();
            if (status != 0 || cause != null) {
                String message = error.equals(MemorySegment.NULL)
                        ? "czarlib streaming operation failed (status " + status + ")"
                        : error.reinterpret(Long.MAX_VALUE).getString(0);
                throw cause == null ? new ZarException(message) : new ZarException(message, cause);
            }
        } finally {
            if (!error.equals(MemorySegment.NULL)) ZarLibrary.freeString(error);
        }
    }

    private static MemorySegment stream(Arena arena, Callbacks callbacks) throws Throwable {
        MemorySegment result = arena.allocate(STREAM);
        if (callbacks.channel != null || callbacks.input != null)
            result.set(ADDRESS, ADDRESS.byteSize(), upcall(arena, callbacks, "read", IO_DESCRIPTOR));
        if (callbacks.channel != null || callbacks.output != null)
            result.set(ADDRESS, 2 * ADDRESS.byteSize(), upcall(arena, callbacks, "write", IO_DESCRIPTOR));
        if (callbacks.channel != null)
            result.set(ADDRESS, 3 * ADDRESS.byteSize(), upcall(arena, callbacks, "seek", SEEK_DESCRIPTOR));
        return result;
    }

    private static MemorySegment upcall(Arena arena, Callbacks callbacks, String name,
            FunctionDescriptor descriptor) throws Throwable {
        MethodType type = name.equals("seek")
                ? MethodType.methodType(int.class, MemorySegment.class, int.class, long.class, MemorySegment.class)
                : MethodType.methodType(int.class, MemorySegment.class, MemorySegment.class, long.class, MemorySegment.class);
        MethodHandle handle = MethodHandles.lookup().findVirtual(Callbacks.class, name, type).bindTo(callbacks);
        if (!name.equals("seek") && SIZE.byteSize() == Integer.BYTES) {
            MethodHandle unsigned = MethodHandles.lookup().findStatic(StreamSupport.class, "unsigned",
                    MethodType.methodType(long.class, int.class));
            handle = MethodHandles.filterArguments(handle, 2, unsigned);
        }
        return LINKER.upcallStub(handle, descriptor, arena);
    }

    private static long unsigned(int value) {
        return Integer.toUnsignedLong(value);
    }

    private static void setCount(MemorySegment pointer, long count) {
        if (SIZE.byteSize() == Long.BYTES)
            pointer.reinterpret(Long.BYTES).set(ValueLayout.JAVA_LONG, 0, count);
        else
            pointer.reinterpret(Integer.BYTES).set(ValueLayout.JAVA_INT, 0, (int) count);
    }

    private static final class Callbacks {
        private final SeekableByteChannel channel;
        private final InputStream input;
        private final OutputStream output;
        private final AtomicReference<Throwable> failure;
        private final byte[] buffer = new byte[BUFFER_SIZE];

        Callbacks(SeekableByteChannel channel, InputStream input, OutputStream output,
                AtomicReference<Throwable> failure) {
            this.channel = channel;
            this.input = input;
            this.output = output;
            this.failure = failure;
        }

        private synchronized int read(MemorySegment context, MemorySegment destination, long length, MemorySegment count) {
            try {
                setCount(count, 0);
                if (length < 0) throw new IOException("read length exceeds Java long");
                int size = (int) Math.min(length, BUFFER_SIZE);
                if (size == 0) return 0;
                int n;
                if (channel != null) {
                    n = channel.read(destination.reinterpret(size).asByteBuffer());
                } else {
                    n = input.read(buffer, 0, size);
                    if (n > 0) MemorySegment.copy(MemorySegment.ofArray(buffer), 0,
                            destination.reinterpret(size), 0, n);
                }
                if (n == 0 || n < -1 || n > size) throw new IOException("invalid or zero read count");
                setCount(count, Math.max(n, 0));
                return 0;
            } catch (Throwable e) {
                failure.compareAndSet(null, e);
                return 1;
            }
        }

        private synchronized int write(MemorySegment context, MemorySegment source, long length, MemorySegment count) {
            try {
                setCount(count, 0);
                if (length < 0) throw new IOException("write length exceeds Java long");
                int size = (int) Math.min(length, BUFFER_SIZE);
                if (size == 0) return 0;
                int n;
                if (channel != null) {
                    n = channel.write(source.reinterpret(size).asByteBuffer());
                    if (n <= 0) throw new IOException("write made no progress");
                } else {
                    MemorySegment.copy(source.reinterpret(size), 0, MemorySegment.ofArray(buffer), 0, size);
                    output.write(buffer, 0, size);
                    n = size;
                }
                setCount(count, n);
                return 0;
            } catch (Throwable e) {
                failure.compareAndSet(null, e);
                return 1;
            }
        }

        private synchronized int seek(MemorySegment context, int origin, long offset, MemorySegment position) {
            try {
                long base = switch (origin) {
                    case 0 -> 0;
                    case 1 -> channel.position();
                    case 2 -> channel.size();
                    default -> throw new IOException("invalid seek origin: " + origin);
                };
                long next = Math.addExact(base, offset);
                if (next < 0) throw new IOException("negative seek position");
                channel.position(next);
                position.reinterpret(Long.BYTES).set(ValueLayout.JAVA_LONG_UNALIGNED, 0, channel.position());
                return 0;
            } catch (Throwable e) {
                failure.compareAndSet(null, e);
                return 1;
            }
        }
    }
}
