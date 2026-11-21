#ifndef CXXZARLIB_STREAM_HPP
#define CXXZARLIB_STREAM_HPP

#include <cstdint>
#include <functional>
#include <iosfwd>
#include <string>
#include <vector>

namespace cxxzarlib {

struct StreamInput {
    std::string path;
    std::reference_wrapper<std::istream> reader;

    StreamInput(std::string path, std::istream &reader);
};

// The archive must initially be empty, seekable, and not opened in append mode.
void create_archive_from_readers(
    std::ostream &archive, const std::vector<StreamInput> &inputs,
    const std::string &options_json = "{}");

// The archive must be seekable and start at offset zero.
std::uint64_t extract_single_file_to_writer(
    std::istream &archive, const std::string &path, std::ostream &output,
    const std::string &options_json = "{}");

} // namespace cxxzarlib

#endif
