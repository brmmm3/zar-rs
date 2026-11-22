import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;

/** Shared TCP framing for the JNI and FFM examples; use only on trusted networks. */
final class TcpStreaming {
    private static final int MAX_CHUNK = 65536;

    @FunctionalInterface
    interface Extractor {
        long extract(Path archive, String entry, OutputStream output) throws Exception;
    }

    private static final class FramedOutput extends OutputStream {
        private final DataOutputStream socket;
        FramedOutput(DataOutputStream socket) { this.socket = socket; }
        @Override public void write(int value) throws IOException {
            write(new byte[] {(byte) value}, 0, 1);
        }
        @Override public void write(byte[] bytes, int offset, int length) throws IOException {
            while (length > 0) {
                int size = Math.min(length, MAX_CHUNK);
                socket.writeInt(size);
                socket.write(bytes, offset, size);
                offset += size;
                length -= size;
            }
        }
    }

    private static void serve(String host, int port, Path archive, Extractor extractor) throws IOException {
        try (ServerSocket server = new ServerSocket()) {
            server.bind(new InetSocketAddress(host, port));
            System.out.println("Listening on " + server.getLocalSocketAddress());
            while (true) {
                try (Socket client = server.accept()) {
                    try {
                        DataInputStream input = new DataInputStream(client.getInputStream());
                        DataOutputStream output = new DataOutputStream(client.getOutputStream());
                        int size = input.readInt();
                        if (size <= 0 || size > MAX_CHUNK)
                            throw new IOException("invalid entry path length");
                        byte[] name = new byte[size];
                        input.readFully(name);
                        String entry = StandardCharsets.UTF_8.newDecoder()
                                .decode(java.nio.ByteBuffer.wrap(name)).toString();
                        long count = extractor.extract(archive, entry, new FramedOutput(output));
                        output.writeInt(0);
                        output.flush();
                        System.out.println(entry + ": " + count + " bytes");
                    } catch (Exception error) {
                        System.err.println("Request failed: " + error);
                    }
                }
            }
        }
    }

    private static void receive(String host, int port, String entry, Path path) throws IOException {
        byte[] name = entry.getBytes(StandardCharsets.UTF_8);
        if (name.length == 0 || name.length > MAX_CHUNK)
            throw new IOException("invalid entry path length");
        try (OutputStream file = Files.newOutputStream(path, StandardOpenOption.CREATE_NEW,
                StandardOpenOption.WRITE)) {
            try (Socket socket = new Socket(host, port)) {
                DataOutputStream output = new DataOutputStream(socket.getOutputStream());
                DataInputStream input = new DataInputStream(socket.getInputStream());
                output.writeInt(name.length);
                output.write(name);
                output.flush();
                byte[] buffer = new byte[MAX_CHUNK];
                while (true) {
                    int size = input.readInt();
                    if (size == 0) break;
                    if (size < 0 || size > MAX_CHUNK)
                        throw new IOException("invalid data frame length");
                    input.readFully(buffer, 0, size);
                    file.write(buffer, 0, size);
                }
            } catch (IOException | RuntimeException error) {
                try { file.close(); } catch (IOException closeError) { error.addSuppressed(closeError); }
                try { Files.delete(path); } catch (IOException deleteError) { error.addSuppressed(deleteError); }
                throw error;
            }
        }
    }

    static void run(String[] args, Extractor extractor) throws Exception {
        if (args.length == 4 && args[0].equals("server"))
            serve(args[1], Integer.parseInt(args[2]), Path.of(args[3]), extractor);
        else if (args.length == 5 && args[0].equals("client"))
            receive(args[1], Integer.parseInt(args[2]), args[3], Path.of(args[4]));
        else
            throw new IllegalArgumentException("server HOST PORT ARCHIVE | client HOST PORT ENTRY OUTPUT");
    }

    private TcpStreaming() {}
}
