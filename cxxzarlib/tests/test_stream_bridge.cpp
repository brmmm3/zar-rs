#include "cxxzarlib/stream.hpp"
#include "cxxzarlib/src/lib.rs.h"
#include "rust/cxx.h"

#include <cassert>
#include <sstream>

int main() {
    bool bridge_error = false;
    try {
        cxxzarlib::execute_json("not_an_operation", "{}");
    } catch (const rust::Error &) {
        bridge_error = true;
    }
    assert(bridge_error);

    std::istringstream input("bridge and streams");
    std::stringstream archive(std::ios::in | std::ios::out | std::ios::binary);
    cxxzarlib::create_archive_from_readers(archive, {{"file", input}});
    std::ostringstream output;
    assert(cxxzarlib::extract_single_file_to_writer(archive, "file", output) == 18);
    assert(output.str() == "bridge and streams");
}
