#include "cxxzarlib/stream.hpp"

#include <algorithm>
#include <cassert>
#include <cstring>
#include <iostream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

namespace {

class ShortReader : public std::streambuf {
public:
    explicit ShortReader(const std::string &value) : value_(value) {}
    std::streamsize largest_request = 0;

protected:
    std::streamsize xsgetn(char *destination, std::streamsize requested) override {
        largest_request = std::max(largest_request, requested);
        const auto count = std::min<std::size_t>(
            std::min<std::size_t>(static_cast<std::size_t>(requested), 4093),
            value_.size() - position_);
        std::memcpy(destination, value_.data() + position_, count);
        position_ += count;
        return static_cast<std::streamsize>(count);
    }

private:
    const std::string &value_;
    std::size_t position_ = 0;
};

class FailBuffer : public std::streambuf {
protected:
    std::streamsize xsputn(const char *, std::streamsize) override { return 0; }
    std::streamsize xsgetn(char *, std::streamsize) override {
        throw std::runtime_error("injected read failure");
    }
};

class RecordingWriter : public std::streambuf {
public:
    std::string value;
    std::streamsize largest_request = 0;

protected:
    std::streamsize xsputn(const char *data, std::streamsize count) override {
        largest_request = std::max(largest_request, count);
        value.append(data, static_cast<std::size_t>(count));
        return count;
    }
};

template <typename F>
void expect_error(F &&function, const std::string &expected = "") {
    try {
        function();
    } catch (const std::exception &error) {
        assert(expected.empty() || std::string(error.what()).find(expected) != std::string::npos);
        return;
    }
    throw std::runtime_error("expected an exception");
}

} // namespace

int main() {
    std::string data;
    for (int i = 0; i < 300000; ++i)
        data.push_back(static_cast<char>((i * 13 + i / 256) % 256));
    ShortReader buffer(data);
    std::istream input(&buffer);
    input.exceptions(std::ios::failbit | std::ios::badbit);
    std::istringstream empty("");
    std::stringstream archive(std::ios::in | std::ios::out | std::ios::binary);
    cxxzarlib::create_archive_from_readers(
        archive, {{"folder/data.bin", input}, {"empty", empty}},
        R"({"encoding_options":{"block_size":65536},"password":"secret","aead":"aes-256-gcm","encryption_scope":"header-and-data"})");
    assert(buffer.largest_request <= 65536);

    RecordingWriter output_buffer;
    std::ostream output(&output_buffer);
    const auto count = cxxzarlib::extract_single_file_to_writer(
        archive, "folder/data.bin", output, R"({"password":"secret"})");
    assert(count == data.size() && output_buffer.value == data);
    assert(output_buffer.largest_request <= 65536);
    std::ostringstream empty_output;
    assert(cxxzarlib::extract_single_file_to_writer(
               archive, "empty", empty_output, R"({"password":"secret"})") == 0);
    expect_error([&] {
        std::ostringstream ignored;
        cxxzarlib::extract_single_file_to_writer(
            archive, "folder/data.bin", ignored, R"({"password":"wrong"})");
    });
    expect_error([&] {
        std::ostringstream ignored;
        cxxzarlib::extract_single_file_to_writer(
            archive, "missing", ignored, R"({"password":"secret"})");
    });
    expect_error([&] {
        std::ostringstream ignored;
        cxxzarlib::extract_single_file_to_writer(archive, std::string("a\0b", 3), ignored);
    }, "NUL");
    expect_error([&] {
        std::ostringstream ignored;
        cxxzarlib::extract_single_file_to_writer(archive, "empty", ignored, "{");
    });

    FailBuffer failing_buffer;
    std::ostream failing_output(&failing_buffer);
    failing_output.exceptions(std::ios::badbit | std::ios::failbit);
    expect_error([&] {
        cxxzarlib::extract_single_file_to_writer(
            archive, "folder/data.bin", failing_output, R"({"password":"secret"})");
    }, "output stream write failed");

    std::istream failing_input(&failing_buffer);
    failing_input.exceptions(std::ios::badbit);
    std::stringstream incomplete(std::ios::in | std::ios::out | std::ios::binary);
    expect_error([&] {
        cxxzarlib::create_archive_from_readers(incomplete, {{"fail", failing_input}});
    }, "input stream read failed");

    std::istringstream normal("x");
    std::ostream non_seekable(&failing_buffer);
    expect_error([&] {
        cxxzarlib::create_archive_from_readers(non_seekable, {{"file", normal}});
    }, "seek failed");
    std::istream non_seekable_input(&failing_buffer);
    expect_error([&] {
        std::ostringstream ignored;
        cxxzarlib::extract_single_file_to_writer(non_seekable_input, "file", ignored);
    }, "seek failed");
    std::cout << "C++ streaming tests passed\n";
}
