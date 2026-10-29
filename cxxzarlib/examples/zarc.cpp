#include "cxxzarlib/src/lib.rs.h"
#include "rust/cxx.h"

#include <cstdio>
#include <iostream>
#include <string>
#include <vector>

namespace {

std::string json_quote(const std::string &text) {
    std::string result = "\"";
    for (const unsigned char value : text) {
        switch (value) {
        case '"':
            result += "\\\"";
            break;
        case '\\':
            result += "\\\\";
            break;
        case '\b':
            result += "\\b";
            break;
        case '\f':
            result += "\\f";
            break;
        case '\n':
            result += "\\n";
            break;
        case '\r':
            result += "\\r";
            break;
        case '\t':
            result += "\\t";
            break;
        default:
            if (value < 0x20) {
                char escaped[7];
                std::snprintf(escaped, sizeof(escaped), "\\u%04x", value);
                result += escaped;
            } else {
                result += static_cast<char>(value);
            }
        }
    }
    result += '"';
    return result;
}

std::string operation_name(const std::string &command) {
    if (command == "create" || command == "c") return "create_archive";
    if (command == "add" || command == "a") return "add_archive";
    if (command == "remove" || command == "r") return "remove_archive";
    if (command == "join" || command == "j") return "join_archive";
    if (command == "check" || command == "k") return "check_archive";
    if (command == "info" || command == "i") return "info_archive";
    if (command == "list" || command == "l") return "list_archive";
    if (command == "extract" || command == "e") return "extract_archive";
    return {};
}

void usage(const char *program) {
    std::cerr << "Usage: " << program << " COMMAND ...\n"
              << "  create|c ARCHIVE PATH...\n"
              << "  add|a ARCHIVE PATH...\n"
              << "  remove|r ARCHIVE ARCHIVE_PATH...\n"
              << "  join|j ARCHIVE --from ID --through ID\n"
              << "  check|k ARCHIVE\n"
              << "  info|i ARCHIVE\n"
              << "  list|l ARCHIVE\n"
              << "  extract|e ARCHIVE [DESTINATION] [--dry-run]\n";
}

}

int main(int argc, char **argv) {
    if (argc < 2 || std::string(argv[1]) == "--help" ||
        std::string(argv[1]) == "-h") {
        usage(argv[0]);
        return argc < 2 ? 2 : 0;
    }

    const auto operation = operation_name(argv[1]);
    if (operation.empty() || argc < 3) {
        usage(argv[0]);
        return 2;
    }

    const std::string archive = argv[2];
    std::string request = "{\"archive\":" + json_quote(archive);
    if (operation == "create_archive" || operation == "add_archive" ||
        operation == "remove_archive") {
        if (argc < 4) {
            usage(argv[0]);
            return 2;
        }
        request += ",\"paths\":[";
        for (int index = 3; index < argc; ++index) {
            if (index > 3) request += ',';
            request += json_quote(argv[index]);
        }
        request += ']';
    } else if (operation == "join_archive") {
        if (argc != 7 || std::string(argv[3]) != "--from" ||
            std::string(argv[5]) != "--through") {
            usage(argv[0]);
            return 2;
        }
        request += ",\"from_id\":" + json_quote(argv[4]) +
                   ",\"through_id\":" + json_quote(argv[6]);
    } else if (operation == "extract_archive") {
        std::string destination = "restored";
        bool dry_run = false;
        for (int index = 3; index < argc; ++index) {
            if (std::string(argv[index]) == "--dry-run") {
                dry_run = true;
            } else {
                destination = argv[index];
            }
        }
        request += ",\"destination\":" + json_quote(destination);
        if (dry_run) request += ",\"dry_run\":true";
    }
    request += '}';

    try {
        auto response = cxxzarlib::execute_json(operation, request);
        std::cout << response.c_str() << '\n';
    } catch (const rust::Error &error) {
        std::cerr << "zarc-cxx: " << error.what() << '\n';
        return 1;
    }
    return 0;
}
