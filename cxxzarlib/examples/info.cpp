#include "cxxzarlib/src/lib.rs.h"
#include "rust/cxx.h"

#include <cstdio>
#include <iostream>
#include <string>

namespace {
std::string json_quote(const char *text) {
    std::string result = "\"";
    for (const unsigned char value : std::string(text)) {
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
}

int main(int argc, char **argv) {
    if (argc != 2) {
        std::cerr << "Usage: " << argv[0] << " ARCHIVE\n";
        return 2;
    }

    const std::string request = "{\"archive\":" + json_quote(argv[1]) + "}";
    try {
        auto response = cxxzarlib::execute_json("info_archive", request);
        std::cout << response.c_str() << '\n';
    } catch (const rust::Error &error) {
        std::cerr << "zarlib: " << error.what() << '\n';
        return 1;
    }
}
