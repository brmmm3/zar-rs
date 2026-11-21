#include "cxxzarlib/stream.hpp"
#include "czarlib.h"

#include <algorithm>
#include <ios>
#include <istream>
#include <limits>
#include <memory>
#include <ostream>
#include <stdexcept>
#include <utility>

namespace cxxzarlib {
namespace {

constexpr std::size_t max_transfer = 64 * 1024;

struct Context {
    std::istream *input = nullptr;
    std::ostream *output = nullptr;
    const char *error = nullptr;
};

int read_stream(void *opaque, unsigned char *buffer, std::size_t capacity,
                std::size_t *count) noexcept {
    auto &ctx = *static_cast<Context *>(opaque);
    *count = 0;
    try {
        const auto size = static_cast<std::streamsize>(
            std::min(capacity, max_transfer));
        try {
            ctx.input->read(reinterpret_cast<char *>(buffer), size);
        } catch (const std::ios_base::failure &) {
            if (!ctx.input->eof() || ctx.input->bad()) throw;
        }
        if (ctx.input->bad() || (ctx.input->fail() && !ctx.input->eof())) {
            ctx.error = "input stream read failed";
            return 1;
        }
        *count = static_cast<std::size_t>(ctx.input->gcount());
        if (ctx.input->eof()) {
            ctx.input->clear(ctx.input->rdstate() &
                             ~(std::ios::eofbit | std::ios::failbit));
        }
        return 0;
    } catch (...) {
        ctx.error = "input stream read failed or threw an exception";
        return 1;
    }
}

int write_stream(void *opaque, const unsigned char *buffer, std::size_t length,
                 std::size_t *count) noexcept {
    auto &ctx = *static_cast<Context *>(opaque);
    *count = 0;
    try {
        const auto size = std::min(length, max_transfer);
        ctx.output->write(reinterpret_cast<const char *>(buffer),
                          static_cast<std::streamsize>(size));
        if (!*ctx.output) {
            ctx.error = "output stream write failed";
            return 1;
        }
        *count = size;
        return 0;
    } catch (...) {
        ctx.error = "output stream write failed or threw an exception";
        return 1;
    }
}

int seek_stream(void *opaque, std::int32_t origin, std::int64_t offset,
                std::uint64_t *position) noexcept {
    auto &ctx = *static_cast<Context *>(opaque);
    try {
        std::ios_base::seekdir direction;
        switch (origin) {
        case 0: direction = std::ios::beg; break;
        case 1: direction = std::ios::cur; break;
        case 2: direction = std::ios::end; break;
        default: throw std::runtime_error("invalid seek origin");
        }
        const auto delta = static_cast<std::streamoff>(offset);
        if (static_cast<std::int64_t>(delta) != offset)
            throw std::runtime_error("seek offset out of range");
        std::streampos result;
        if (ctx.input) {
            ctx.input->seekg(delta, direction);
            result = ctx.input->tellg();
        } else {
            ctx.output->seekp(delta, direction);
            result = ctx.output->tellp();
        }
        const auto absolute = static_cast<std::streamoff>(result);
        if (absolute < 0 ||
            static_cast<std::uintmax_t>(absolute) >
                static_cast<std::uintmax_t>(std::numeric_limits<std::int64_t>::max()))
            throw std::runtime_error("invalid seek result");
        *position = static_cast<std::uint64_t>(absolute);
        return 0;
    } catch (...) {
        ctx.error = "archive stream seek failed or threw an exception";
        return 1;
    }
}

void check_c_string(const std::string &value, const char *what) {
    if (value.find('\0') != std::string::npos)
        throw std::invalid_argument(std::string(what) + " contains a NUL byte");
}

void check_result(int status, char *error, const Context &archive,
                  const Context *output, const std::vector<Context> *inputs) {
    std::unique_ptr<char, decltype(&czarlib_string_free)> message(
        error, &czarlib_string_free);
    if (status == 0) return;
    std::string detail = message ? message.get() : "streaming operation failed";
    const char *callback_error = archive.error;
    if (!callback_error && output) callback_error = output->error;
    if (!callback_error && inputs) {
        for (const auto &input : *inputs) {
            if (input.error) {
                callback_error = input.error;
                break;
            }
        }
    }
    if (callback_error) detail += std::string(": ") + callback_error;
    throw std::runtime_error(detail);
}

} // namespace

StreamInput::StreamInput(std::string path_value, std::istream &reader_value)
    : path(std::move(path_value)), reader(reader_value) {}

void create_archive_from_readers(std::ostream &archive,
                                 const std::vector<StreamInput> &inputs,
                                 const std::string &options_json) {
    check_c_string(options_json, "options JSON");
    Context archive_ctx;
    archive_ctx.output = &archive;
    const czarlib_stream archive_stream = {
        &archive_ctx, nullptr, write_stream, seek_stream};
    std::vector<Context> contexts(inputs.size());
    std::vector<czarlib_stream_input> c_inputs;
    c_inputs.reserve(inputs.size());
    for (std::size_t i = 0; i < inputs.size(); ++i) {
        check_c_string(inputs[i].path, "input path");
        contexts[i].input = &inputs[i].reader.get();
        c_inputs.push_back({inputs[i].path.c_str(),
                            {&contexts[i], read_stream, nullptr, nullptr}});
    }
    char *error = nullptr;
    const int status = czarlib_create_archive_from_readers(
        &archive_stream, c_inputs.data(), c_inputs.size(),
        options_json.c_str(), &error);
    check_result(status, error, archive_ctx, nullptr, &contexts);
}

std::uint64_t extract_single_file_to_writer(std::istream &archive,
                                           const std::string &path,
                                           std::ostream &output,
                                           const std::string &options_json) {
    check_c_string(path, "archive path");
    check_c_string(options_json, "options JSON");
    Context archive_ctx;
    archive_ctx.input = &archive;
    Context output_ctx;
    output_ctx.output = &output;
    const czarlib_stream archive_stream = {
        &archive_ctx, read_stream, nullptr, seek_stream};
    const czarlib_stream output_stream = {
        &output_ctx, nullptr, write_stream, nullptr};
    std::uint64_t written = 0;
    char *error = nullptr;
    const int status = czarlib_extract_single_file_to_writer(
        &archive_stream, path.c_str(), &output_stream, options_json.c_str(),
        &written, &error);
    check_result(status, error, archive_ctx, &output_ctx, nullptr);
    return written;
}

} // namespace cxxzarlib
