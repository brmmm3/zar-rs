package org.zar.jarlib.jni.examples;

import org.zar.jarlib.jni.ZarLibrary;

public final class BasicExample {
    private BasicExample() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 2) {
            System.err.println("Usage: BasicExample ARCHIVE INPUT_PATH");
            System.exit(2);
        }
        String archive = json(args[0]);
        String input = json(args[1]);
        System.out.println("create: " + ZarLibrary.createArchive(
                "{\"archive\":" + archive + ",\"paths\":[" + input + "]}"));
        System.out.println("list: " + ZarLibrary.listArchive(
                "{\"archive\":" + archive + "}"));
        System.out.println("check: " + ZarLibrary.checkArchive(
                "{\"archive\":" + archive + "}"));
        System.out.println("memory: " + ZarLibrary.extractArchiveToMemory(
                "{\"archive\":" + archive + "}"));
        System.out.println("info: " + ZarLibrary.infoArchive(
                "{\"archive\":" + archive + "}"));
    }

    private static String json(String value) {
        return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }
}
