package org.zar.jarlib.ffm;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import org.junit.jupiter.api.Test;

final class ZarLibraryTest {
    private static Path testDirectory() throws IOException {
        return Files.createTempDirectory(Path.of("").toAbsolutePath(), "ffm-test-");
    }

    private static String quote(String value) {
        return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    @Test
    void exercisesAllOperationsAndEncryption() throws Exception {
        Path directory = testDirectory();
        try {
            Path input = Files.createDirectories(directory.resolve("input"));
            Files.writeString(input.resolve("alpha.txt"), "alpha contents");
            Files.write(input.resolve("beta.bin"), new byte[] {0, 1, 2, 3});
            Path archive = directory.resolve("sample.zar");
            String createRequest = "{\"archive\":" + quote(archive.toString())
                    + ",\"paths\":[" + quote(input.toString())
                    + "],\"block_size\":65536}";
            assertTrue(ZarLibrary.createArchive(createRequest).contains("\"block_size\":65536"));
            assertTrue(ZarLibrary.listArchive("{\"archive\":" + quote(archive.toString()) + "}")
                    .contains("alpha.txt"));
            assertTrue(ZarLibrary.infoArchive("{\"archive\":" + quote(archive.toString()) + "}")
                    .contains("\"id\":\"00000001\""));
            assertTrue(ZarLibrary.checkArchive("{\"archive\":" + quote(archive.toString()) + "}")
                    .contains("\"commits\":1"));

            String memory = ZarLibrary.extractArchiveToMemory(
                    "{\"archive\":" + quote(archive.toString())
                            + ",\"offset\":1,\"size_limit\":2}");
            assertTrue(memory.contains("[108,112]"), memory);

            Path destination = directory.resolve("restored");
            assertTrue(ZarLibrary.extractArchive(
                    "{\"archive\":" + quote(archive.toString())
                            + ",\"destination\":" + quote(destination.toString())
                            + ",\"dry_run\":true}").contains("\"dry_run\":true"));
            assertFalse(Files.exists(destination));
            ZarLibrary.extractArchive(
                    "{\"archive\":" + quote(archive.toString())
                            + ",\"destination\":" + quote(destination.toString()) + "}");
            assertThrows(ZarException.class, () -> ZarLibrary.extractArchive(
                    "{\"archive\":" + quote(archive.toString())
                            + ",\"destination\":" + quote(destination.toString()) + "}"));
            ZarLibrary.extractArchive(
                    "{\"archive\":" + quote(archive.toString())
                            + ",\"destination\":" + quote(destination.toString())
                            + ",\"existing\":\"skip\"}");
            try (var extracted = Files.walk(destination)) {
                assertTrue(extracted.anyMatch(path -> path.getFileName().toString().equals("alpha.txt")));
            }
            ZarLibrary.extractArchive(
                    "{\"archive\":" + quote(archive.toString())
                            + ",\"destination\":" + quote(destination.toString())
                            + ",\"existing\":\"overwrite\"}");

            Path second = directory.resolve("second.txt");
            Files.writeString(second, "second");
            ZarLibrary.addArchive("{\"archive\":" + quote(archive.toString())
                    + ",\"paths\":[" + quote(second.toString()) + "]}");
            assertTrue(ZarLibrary.listArchive("{\"archive\":" + quote(archive.toString()) + "}")
                    .contains("second.txt"));
            ZarLibrary.joinArchive("{\"archive\":" + quote(archive.toString())
                    + ",\"from_id\":\"00000001\",\"through_id\":\"00000002\"}");
            ZarLibrary.removeArchive("{\"archive\":" + quote(archive.toString())
                    + ",\"paths\":[" + quote(directory.getFileName() + "/second.txt") + "]}");
            assertFalse(ZarLibrary.listArchive("{\"archive\":" + quote(archive.toString()) + "}")
                    .contains("second.txt"));
            assertThrows(ZarException.class, () -> ZarLibrary.createArchive(
                    "{\"archive\":" + quote(directory.resolve("bad.zar").toString())
                            + ",\"paths\":[" + quote(second.toString())
                            + "],\"block_size\":1}"));
            Path filtered = directory.resolve("filtered.zar");
            ZarLibrary.createArchive("{\"archive\":" + quote(filtered.toString())
                    + ",\"paths\":[" + quote(input.toString())
                    + "],\"include_files\":[\"*.txt\"]}");
            String filteredListing = ZarLibrary.listArchive(
                    "{\"archive\":" + quote(filtered.toString()) + "}");
            assertTrue(filteredListing.contains("alpha.txt"));
            assertFalse(filteredListing.contains("beta.bin"));

            Path encrypted = directory.resolve("encrypted.zar");
            ZarLibrary.createArchive("{\"archive\":" + quote(encrypted.toString())
                    + ",\"paths\":[" + quote(input.resolve("alpha.txt").toString())
                    + "],\"password\":\"secret\",\"aead\":\"chacha20-poly1305\"}");
            assertTrue(ZarLibrary.checkArchive("{\"archive\":" + quote(encrypted.toString())
                    + ",\"password\":\"secret\"}").contains("\"commits\":1"));
            assertTrue(ZarLibrary.extractArchiveToMemory(
                    "{\"archive\":" + quote(encrypted.toString())
                            + ",\"password\":\"secret\"}")
                    .contains("[97,108,112,104,97,32,99,111,110,116,101,110,116,115]"));
            assertThrows(ZarException.class, () -> ZarLibrary.checkArchive(
                    "{\"archive\":" + quote(encrypted.toString())
                            + ",\"password\":\"wrong\"}"));

            Path perCommit = directory.resolve("per-commit.zar");
            ZarLibrary.createArchive("{\"archive\":" + quote(perCommit.toString())
                    + ",\"paths\":[" + quote(input.resolve("alpha.txt").toString())
                    + "],\"password\":\"first\",\"encryption_scope\":\"data\"}");
            ZarLibrary.addArchive("{\"archive\":" + quote(perCommit.toString())
                    + ",\"paths\":[" + quote(input.resolve("alpha.txt").toString())
                    + "],\"password\":\"second\","
                    + "\"passwords\":{\"00000001\":\"first\"},"
                    + "\"encryption_scope\":\"data\"}");
            String unlocked = ZarLibrary.extractArchiveToMemory(
                    "{\"archive\":" + quote(perCommit.toString())
                            + ",\"passwords\":{\"00000001\":\"first\","
                            + "\"00000002\":\"second\"}}");
            assertTrue(unlocked.contains("alpha.txt"));
        } finally {
            deleteTree(directory);
        }
    }

    @Test
    void validatesRequestsAndSupportsConcurrentCalls() throws Exception {
        assertThrows(ZarException.class, () -> ZarLibrary.executeJson("info_archive", "{"));
        assertThrows(ZarException.class, () -> ZarLibrary.executeJson("unknown", "{}"));
        assertThrows(ZarException.class, () -> ZarLibrary.executeJson("info_archive", "[]"));

        Path directory = testDirectory();
        try (var executor = Executors.newFixedThreadPool(4)) {
            Future<?>[] calls = new Future<?>[4];
            for (int index = 0; index < calls.length; index++) {
                final int current = index;
                calls[index] = executor.submit(() -> {
                    try {
                        Path input = directory.resolve("thread-" + current + ".txt");
                        Path archive = directory.resolve("thread-" + current + ".zar");
                        Files.writeString(input, "data-" + current);
                        ZarLibrary.createArchive("{\"archive\":" + quote(archive.toString())
                                + ",\"paths\":[" + quote(input.toString()) + "]}");
                        assertTrue(ZarLibrary.checkArchive(
                                "{\"archive\":" + quote(archive.toString()) + "}")
                                .contains("\"commits\":1"));
                    } catch (Exception exception) {
                        throw new RuntimeException(exception);
                    }
                });
            }
            for (Future<?> call : calls) {
                call.get();
            }
        } finally {
            deleteTree(directory);
        }
    }

    private static void deleteTree(Path directory) throws IOException {
        if (Files.exists(directory)) {
            try (var paths = Files.walk(directory)) {
                paths.sorted(java.util.Comparator.reverseOrder()).forEach(path -> {
                    try {
                        Files.deleteIfExists(path);
                    } catch (IOException exception) {
                        throw new RuntimeException(exception);
                    }
                });
            }
        }
    }
}
