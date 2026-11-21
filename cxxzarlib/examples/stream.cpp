#include "cxxzarlib/stream.hpp"

#include <iostream>
#include <sstream>
#include <vector>

int main() {
    try {
        std::istringstream input("Hello from a C++ stream!\n");
        std::stringstream archive(
            std::ios::in | std::ios::out | std::ios::binary);
        cxxzarlib::create_archive_from_readers(
            archive, {{"hello.txt", input}},
            R"({"encoding_options":{"block_size":65536}})");

        std::ostringstream output;
        const auto written = cxxzarlib::extract_single_file_to_writer(
            archive, "hello.txt", output);
        std::cout << written << " bytes: " << output.str();
    } catch (const std::exception &error) {
        std::cerr << "streaming failed: " << error.what() << '\n';
        return 1;
    }
}
