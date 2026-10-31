#include "cxxzarlib/src/lib.rs.h"
#include "rust/cxx.h"

#include <array>
#include <chrono>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

namespace fs = std::filesystem;

namespace {

void require(bool condition, const std::string &message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

std::string invoke(const std::string &operation, const std::string &request) {
    try {
        auto response = cxxzarlib::execute_json(operation, request);
        return static_cast<std::string>(response);
    } catch (const rust::Error &error) {
        throw std::runtime_error(operation + " failed: " + error.what());
    }
}

void expect_error(const std::string &operation, const std::string &request,
                  const std::string &message_fragment) {
    try {
        (void)cxxzarlib::execute_json(operation, request);
        throw std::runtime_error(operation + " unexpectedly succeeded");
    } catch (const rust::Error &error) {
        require(std::string(error.what()).find(message_fragment) != std::string::npos,
                "expected error containing '" + message_fragment + "', got '" +
                    error.what() + "'");
    }
}

void test_request_validation() {
    expect_error("not_an_operation", "{}", "unknown operation");
    expect_error("info_archive", "{", "invalid request JSON");
    expect_error("info_archive", "[]", "request JSON must be an object");
}

void write_file(const fs::path &path, const std::string &contents) {
    std::ofstream file(path, std::ios::binary);
    require(file.good(), "could not open file for writing: " + path.string());
    file.write(contents.data(), static_cast<std::streamsize>(contents.size()));
    require(file.good(), "could not write file: " + path.string());
}

std::string read_file(const fs::path &path) {
    std::ifstream file(path, std::ios::binary);
    require(file.good(), "could not open file for reading: " + path.string());
    return std::string(std::istreambuf_iterator<char>(file), {});
}

void copy_file(const fs::path &source, const fs::path &destination) {
    fs::copy_file(source, destination);
}

void require_same_file(const fs::path &left, const fs::path &right) {
    require(read_file(left) == read_file(right),
            "files differ: " + left.string() + " and " + right.string());
}

void create_fixture() {
    fs::create_directories("input/nested");
    write_file("input/alpha.txt", "alpha contents");
    std::string bytes;
    for (unsigned index = 0; index < 128; ++index) {
        bytes.push_back(static_cast<char>(index));
    }
    write_file("input/nested/beta.bin", bytes);
    write_file("input/empty.txt", "");
}

void test_create_list_check_info_extract() {
    auto created = invoke(
        "create_archive",
        R"({"archive":"sample.zar","paths":["input"],"block_size":65536})");
    require(created.find("\"block_size\":65536") != std::string::npos,
            "create response omitted block size");

    auto listing = invoke("list_archive", R"({"archive":"sample.zar"})");
    require(listing.find("\"path\":\"input/alpha.txt\"") != std::string::npos,
            "list omitted alpha file");
    require(listing.find("\"size\":14") != std::string::npos,
            "list omitted file size");
    require(listing.find("\"path\":\"input/nested\"") != std::string::npos,
            "list omitted nested directory");
    require(listing.find("\"path\":\"input/empty.txt\"") != std::string::npos,
            "list omitted empty file");

    auto report = invoke("check_archive", R"({"archive":"sample.zar"})");
    require(report.find("\"commits\":1") != std::string::npos,
            "check reported wrong commit count");
    require(report.find("\"logical_state_readable\":true") != std::string::npos,
            "check did not report readable logical state");
    require(report.find("\"encrypted_blocks\":0") != std::string::npos,
            "check reported encrypted blocks for plain archive");

    auto info = invoke("info_archive", R"({"archive":"sample.zar"})");
    require(info.find("\"id\":\"00000001\"") != std::string::npos,
            "info omitted commit ID");

    auto plan = invoke(
        "extract_archive",
        R"({"archive":"sample.zar","destination":"dry","dry_run":true})");
    require(plan.find("\"dry_run\":true") != std::string::npos,
            "extract dry-run response is missing flag");
    require(plan.find("dry/input/alpha.txt") != std::string::npos,
            "extract dry-run omitted output path");
    require(!fs::exists("dry"), "extract dry-run created its destination");

    (void)invoke(
        "extract_archive",
        R"({"archive":"sample.zar","destination":"restored"})");
    require(read_file("restored/input/alpha.txt") == "alpha contents",
            "extracted text file did not match");
    std::string expected_bytes;
    for (unsigned index = 0; index < 128; ++index) {
        expected_bytes.push_back(static_cast<char>(index));
    }
    require(read_file("restored/input/nested/beta.bin") == expected_bytes,
            "extracted binary file did not match");
    require(fs::exists("restored/input/empty.txt"),
            "extracted empty file is missing");
}

void test_options_filters_and_atomic_failures() {
    write_file("keep.txt", "keep");
    write_file("drop.bin", "drop");
    expect_error(
        "create_archive",
        R"({"archive":"bad-size.zar","paths":["keep.txt"],"block_size":1})",
        "block_size");
    expect_error(
        "create_archive",
        R"({"archive":"bad-level.zar","paths":["keep.txt"],"compression_level":23})",
        "compression_level");

    (void)invoke(
        "create_archive",
        R"({"archive":"filtered.zar","paths":["keep.txt","drop.bin"],"block_size":65536,"include_files":["*.txt"]})");
    auto listing = invoke("list_archive", R"({"archive":"filtered.zar"})");
    require(listing.find("\"path\":\"keep.txt\"") != std::string::npos,
            "filter omitted included file");
    require(listing.find("\"path\":\"drop.bin\"") == std::string::npos,
            "filter retained excluded file");

    expect_error(
        "create_archive",
        R"({"archive":"filtered.zar","paths":["keep.txt"],"block_size":65536})",
        "already exists");
    (void)invoke(
        "create_archive",
        R"({"archive":"filtered.zar","paths":["keep.txt","drop.bin"],"block_size":65536,"overwrite":true})");
    listing = invoke("list_archive", R"({"archive":"filtered.zar"})");
    require(listing.find("\"path\":\"drop.bin\"") != std::string::npos,
            "overwrite did not replace archive");
}

void test_add_replace_remove_and_through() {
    write_file("input/alpha.txt", "replaced");
    write_file("input/nested/new.txt", "new");
    (void)invoke(
        "add_archive",
        R"({"archive":"sample.zar","paths":["input/alpha.txt","input/nested/new.txt"]})");

    auto files = invoke(
        "extract_archive_to_memory", R"({"archive":"sample.zar"})");
    require(files.find("\"input/alpha.txt\":[114,101,112,108,97,99,101,100]") !=
                std::string::npos,
            "memory extraction omitted replaced file contents");
    require(files.find("\"input/nested/new.txt\":[110,101,119]") !=
                std::string::npos,
            "memory extraction omitted added file contents");

    auto info = invoke("info_archive", R"({"archive":"sample.zar"})");
    require(info.find("\"id\":\"00000001\"") != std::string::npos,
            "add unexpectedly changed first commit ID");
    auto through = invoke(
        "list_archive",
        R"({"archive":"sample.zar","through":"00000001"})");
    require(through.find("input/nested/new.txt") == std::string::npos,
            "through listing included a later commit");
    expect_error(
        "list_archive",
        R"({"archive":"sample.zar","through":"ffffffff"})",
        "not found");

    copy_file("sample.zar", "sample-before-failed-remove.zar");
    expect_error(
        "remove_archive",
        R"({"archive":"sample.zar","paths":["missing"]})",
        "not found");
    require_same_file("sample.zar", "sample-before-failed-remove.zar");

    auto through_plan = invoke(
        "extract_archive",
        R"({"archive":"sample.zar","destination":"through","through":"00000001","dry_run":true})");
    require(through_plan.find("new.txt") == std::string::npos,
            "through extraction plan included a later commit");
    (void)invoke(
        "remove_archive",
        R"({"archive":"sample.zar","paths":["input/nested"]})");
    auto listing = invoke("list_archive", R"({"archive":"sample.zar"})");
    require(listing.find("input/nested") == std::string::npos,
            "recursive remove retained nested entries");
    require(listing.find("input/alpha.txt") != std::string::npos,
            "remove deleted an unrelated file");
    auto report = invoke("check_archive", R"({"archive":"sample.zar"})");
    require(report.find("\"commits\":3") != std::string::npos,
            "remove did not append its commit");
}

void test_join_and_suffix_preservation() {
    write_file("input/one.txt", "one");
    (void)invoke(
        "add_archive",
        R"({"archive":"sample.zar","paths":["input/one.txt"]})");
    write_file("input/two.txt", "two");
    (void)invoke(
        "add_archive",
        R"({"archive":"sample.zar","paths":["input/two.txt"]})");

    auto before = invoke("info_archive", R"({"archive":"sample.zar"})");
    require(before.find("\"id\":\"00000005\"") != std::string::npos,
            "fixture did not have expected suffix commit");
    (void)invoke(
        "join_archive",
        R"({"archive":"sample.zar","from_id":"00000003","through_id":"00000004"})");
    auto after = invoke("info_archive", R"({"archive":"sample.zar"})");
    require(after.find("\"id\":\"00000005\"") != std::string::npos,
            "join failed to preserve suffix commit");
    auto listing = invoke("list_archive", R"({"archive":"sample.zar"})");
    require(listing.find("input/one.txt") != std::string::npos,
            "join lost first joined file");
    require(listing.find("input/two.txt") != std::string::npos,
            "join lost second joined file");
}

void test_extract_policies_memory_and_encryption() {
    fs::create_directories("existing/input");
    write_file("existing/input/alpha.txt", "keep existing");
    expect_error(
        "extract_archive",
        R"({"archive":"sample.zar","destination":"existing"})",
        "already exists");

    (void)invoke(
        "extract_archive",
        R"({"archive":"sample.zar","destination":"existing","existing":"skip"})");
    require(read_file("existing/input/alpha.txt") == "keep existing",
            "skip policy overwrote an existing file");
    (void)invoke(
        "extract_archive",
        R"({"archive":"sample.zar","destination":"existing","existing":"overwrite"})");
    require(read_file("existing/input/alpha.txt") == "replaced",
            "overwrite policy did not replace existing file");

    auto plan = invoke(
        "extract_archive",
        R"({"archive":"sample.zar","destination":"filtered-dry","dry_run":true,"include_files":["*.txt"]})");
    require(plan.find("input/alpha.txt") != std::string::npos,
            "extract filter omitted included file");
    require(plan.find("beta.bin") == std::string::npos,
            "extract filter retained excluded file");

    auto memory = invoke(
        "extract_archive_to_memory",
        R"({"archive":"sample.zar","offset":1,"size_limit":3,"include_files":["*.txt"],"exclude_files":["input/empty.txt"]})");
    require(memory.find("\"input/alpha.txt\":[101,112,108]") !=
                std::string::npos,
            "memory offset/size limit was not applied");
    memory = invoke(
        "extract_archive_to_memory",
        R"({"archive":"sample.zar","offset":1000})");
    require(memory.find("\"input/alpha.txt\":[]") != std::string::npos,
            "large memory offset did not return empty content");
    require(memory.find("\"input/empty.txt\":[]") != std::string::npos,
            "empty file was not returned by memory extraction");

    auto encrypted = invoke(
        "create_archive",
        R"({"archive":"encrypted.zar","paths":["input/alpha.txt"],"block_size":65536,"password":"secret"})");
    require(encrypted.find("\"aead\":\"aes-256-gcm\"") != std::string::npos,
            "encrypted create response omitted default AEAD");
    auto checked = invoke(
        "check_archive",
        R"({"archive":"encrypted.zar","password":"secret"})");
    require(checked.find("\"commits\":1") != std::string::npos,
            "encrypted check failed");
    auto unlocked_list = invoke(
        "list_archive",
        R"({"archive":"encrypted.zar","password":"secret"})");
    require(unlocked_list.find("input/alpha.txt") != std::string::npos,
            "encrypted listing failed");
    auto unlocked_memory = invoke(
        "extract_archive_to_memory",
        R"({"archive":"encrypted.zar","password":"secret"})");
    require(unlocked_memory.find(
                "\"input/alpha.txt\":[114,101,112,108,97,99,101,100]") !=
                std::string::npos,
            "encrypted in-memory extraction failed");
    expect_error(
        "check_archive",
        R"({"archive":"encrypted.zar","password":"wrong"})",
        "authentication");
    expect_error(
        "extract_archive",
        R"({"archive":"encrypted.zar","destination":"locked"})",
        "password");
    require(!fs::exists("locked"), "failed encrypted extraction created output");
    copy_file("encrypted.zar", "encrypted-before-failed-add.zar");
    expect_error(
        "add_archive",
        R"({"archive":"encrypted.zar","paths":["keep.txt"]})",
        "encrypted");
    require_same_file("encrypted.zar", "encrypted-before-failed-add.zar");

    encrypted = invoke(
        "create_archive",
        R"({"archive":"chacha.zar","paths":["input/alpha.txt"],"block_size":65536,"password":"secret","aead":"chacha20-poly1305"})");
    require(encrypted.find("\"aead\":\"chacha20-poly1305\"") !=
                std::string::npos,
            "ChaCha create response omitted AEAD");
    checked = invoke(
        "check_archive",
        R"({"archive":"chacha.zar","password":"secret"})");
    require(checked.find("\"commits\":1") != std::string::npos,
            "ChaCha check failed");
    unlocked_memory = invoke(
        "extract_archive_to_memory",
        R"({"archive":"chacha.zar","password":"secret"})");
    require(unlocked_memory.find(
                "\"input/alpha.txt\":[114,101,112,108,97,99,101,100]") !=
                std::string::npos,
            "ChaCha in-memory extraction failed");
    (void)invoke(
        "extract_archive",
        R"({"archive":"chacha.zar","destination":"chacha-restored","password":"secret"})");
    require(read_file("chacha-restored/input/alpha.txt") == "replaced",
            "ChaCha filesystem extraction failed");

    (void)invoke(
        "add_archive",
        R"({"archive":"chacha.zar","paths":["input/alpha.txt"],"block_size":65536,"password":"second-secret","passwords":{"00000001":"secret"},"aead":"aes-256-gcm","encryption_scope":"data"})");
    unlocked_memory = invoke(
        "extract_archive_to_memory",
        R"({"archive":"chacha.zar","passwords":{"00000001":"secret","00000002":"second-secret"}})");
    require(unlocked_memory.find(
                "\"input/alpha.txt\":[114,101,112,108,97,99,101,100]") !=
                std::string::npos,
            "per-commit password extraction failed");
    auto info = invoke("info_archive", R"({"archive":"chacha.zar"})");
    require(info.find("\"data_encrypted\":true") != std::string::npos,
            "info omitted data encryption state");
}

void test_concurrent_calls() {
    std::vector<std::thread> workers;
    std::vector<std::string> failures(4);
    for (unsigned index = 0; index < failures.size(); ++index) {
        workers.emplace_back([index, &failures] {
            try {
                const auto source = "thread-" + std::to_string(index) + ".bin";
                const auto archive = "thread-" + std::to_string(index) + ".zar";
                const auto contents = "payload-" + std::to_string(index);
                write_file(source, contents);
                const auto create_request =
                    "{\"archive\":\"" + archive + "\",\"paths\":[\"" +
                    source + "\"],\"block_size\":65536}";
                (void)invoke("create_archive", create_request);
                auto checked = invoke(
                    "check_archive", "{\"archive\":\"" + archive + "\"}");
                require(checked.find("\"commits\":1") != std::string::npos,
                        "concurrent check reported wrong commit count");
                auto memory = invoke(
                    "extract_archive_to_memory",
                    "{\"archive\":\"" + archive + "\"}");
                std::string expected = "\"" + source + "\":[";
                for (std::size_t offset = 0; offset < contents.size(); ++offset) {
                    if (offset != 0) {
                        expected += ',';
                    }
                    expected += std::to_string(
                        static_cast<unsigned char>(contents[offset]));
                }
                expected += ']';
                require(memory.find(expected) != std::string::npos,
                        "concurrent memory extraction differed");
            } catch (const std::exception &error) {
                failures[index] = error.what();
            }
        });
    }
    for (auto &worker : workers) {
        worker.join();
    }
    for (const auto &failure : failures) {
        require(failure.empty(), failure);
    }
}

}

int main() {
    const auto unique = std::chrono::steady_clock::now()
                           .time_since_epoch()
                           .count();
    const auto temporary =
        fs::temp_directory_path() / ("cxxzarlib-test-" + std::to_string(unique));
    const auto original = fs::current_path();
    try {
        fs::create_directories(temporary);
        fs::current_path(temporary);
        create_fixture();
        test_request_validation();
        test_create_list_check_info_extract();
        test_options_filters_and_atomic_failures();
        test_add_replace_remove_and_through();
        test_join_and_suffix_preservation();
        test_extract_policies_memory_and_encryption();
        test_concurrent_calls();
        fs::current_path(original);
        fs::remove_all(temporary);
        std::cout << "cxxzarlib C++ API tests passed\n";
    } catch (const std::exception &error) {
        fs::current_path(original);
        std::cerr << "cxxzarlib C++ API test failed: " << error.what() << '\n';
        return 1;
    }
    return 0;
}
