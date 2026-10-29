package org.zar.jarlib.jni.examples;

import org.zar.jarlib.jni.ZarLibrary;

public final class ZarcExample {
    private ZarcExample() {}

    public static void main(String[] args) {
        if (args.length == 1 && (args[0].equals("--help") || args[0].equals("-h"))) {
            usage();
            return;
        }
        if (args.length < 2) {
            usage();
            System.exit(2);
        }
        String operation = switch (args[0]) {
            case "create", "c" -> "create_archive";
            case "add", "a" -> "add_archive";
            case "remove", "r" -> "remove_archive";
            case "join", "j" -> "join_archive";
            case "check", "k" -> "check_archive";
            case "info", "i" -> "info_archive";
            case "list", "l" -> "list_archive";
            case "extract", "e" -> "extract_archive";
            case "memory" -> "extract_archive_to_memory";
            default -> "";
        };
        if (operation.isEmpty()) {
            usage();
            System.exit(2);
        }
        try {
            System.out.println(ZarLibrary.executeJson(operation, request(operation, args)));
        } catch (Exception exception) {
            System.err.println("zarc-java-jni: " + exception.getMessage());
            System.exit(1);
        }
    }

    private static String request(String operation, String[] args) {
        StringBuilder request = new StringBuilder("{\"archive\":")
                .append(json(args[1]));
        if (operation.equals("create_archive") || operation.equals("add_archive")
                || operation.equals("remove_archive")) {
            if (args.length < 3) {
                throw new IllegalArgumentException("operation requires one or more paths");
            }
            request.append(",\"paths\":[");
            for (int index = 2; index < args.length; index++) {
                if (index > 2) request.append(',');
                request.append(json(args[index]));
            }
            request.append(']');
        } else if (operation.equals("join_archive")) {
            if (args.length != 6 || !args[2].equals("--from")
                    || !args[4].equals("--through")) {
                throw new IllegalArgumentException("join requires --from ID --through ID");
            }
            request.append(",\"from_id\":").append(json(args[3]))
                    .append(",\"through_id\":").append(json(args[5]));
        } else if (operation.equals("extract_archive")) {
            String destination = "restored";
            boolean dryRun = false;
            for (int index = 2; index < args.length; index++) {
                if (args[index].equals("--dry-run")) {
                    dryRun = true;
                } else {
                    destination = args[index];
                }
            }
            request.append(",\"destination\":").append(json(destination));
            if (dryRun) request.append(",\"dry_run\":true");
        }
        return request.append('}').toString();
    }

    private static String json(String value) {
        return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    private static void usage() {
        System.err.println("Usage: ZarcExample COMMAND ...");
        System.err.println("  create|c ARCHIVE PATH...");
        System.err.println("  add|a ARCHIVE PATH...");
        System.err.println("  remove|r ARCHIVE ARCHIVE_PATH...");
        System.err.println("  join|j ARCHIVE --from ID --through ID");
        System.err.println("  check|k ARCHIVE");
        System.err.println("  info|i ARCHIVE");
        System.err.println("  list|l ARCHIVE");
        System.err.println("  extract|e ARCHIVE [DESTINATION] [--dry-run]");
        System.err.println("  memory ARCHIVE");
    }
}
