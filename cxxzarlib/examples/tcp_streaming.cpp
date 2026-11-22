#define _POSIX_C_SOURCE 200809L
#include "cxxzarlib/stream.hpp"
#include "../../examples/tcp-streaming/native.h"

#include <fstream>
#include <iostream>
#include <streambuf>

class SocketBuffer : public std::streambuf {
    int fd_;
public:
    explicit SocketBuffer(int fd) : fd_(fd) {}
    std::streamsize xsputn(const char *data, std::streamsize length) override {
        return zar_frame(fd_, reinterpret_cast<const unsigned char *>(data),
                         static_cast<size_t>(length)) ? 0 : length;
    }
    int_type overflow(int_type value) override {
        if (traits_type::eq_int_type(value, traits_type::eof())) return traits_type::not_eof(value);
        char byte = traits_type::to_char_type(value);
        return xsputn(&byte, 1) == 1 ? value : traits_type::eof();
    }
};

static int extract(const char *path, const char *entry, int socket_fd) {
    try {
        std::ifstream archive(path, std::ios::binary);
        if (!archive) throw std::runtime_error("cannot open archive");
        SocketBuffer buffer(socket_fd);
        std::ostream output(&buffer);
        output.exceptions(std::ios::badbit | std::ios::failbit);
        auto count = cxxzarlib::extract_single_file_to_writer(archive, entry, output);
        std::cerr << entry << ": " << count << " bytes\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "Extraction failed: " << error.what() << '\n';
        return 1;
    }
}

int main(int argc, char **argv) {
    return zar_tcp_main(argc, argv, extract);
}
