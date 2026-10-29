#define _XOPEN_SOURCE 700

#include "czarlib.h"

#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#define REQUIRE(condition)                                                      \
    do {                                                                        \
        if (!(condition)) {                                                     \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #condition); \
            exit(1);                                                            \
        }                                                                       \
    } while (0)

static char *invoke(const char *operation, const char *request) {
    char *response = NULL;
    char *error = NULL;
    int status = czarlib_execute_json(operation, request, &response, &error);
    if (status != 0) {
        fprintf(stderr, "%s failed (%d): %s\n", operation, status,
                error ? error : "(no error message)");
        czarlib_string_free(error);
        exit(1);
    }
    REQUIRE(response != NULL);
    REQUIRE(error == NULL);
    return response;
}

static void expect_error(const char *operation, const char *request,
                         const char *message_fragment) {
    char *response = NULL;
    char *error = NULL;
    int status = czarlib_execute_json(operation, request, &response, &error);
    REQUIRE(status == 1);
    REQUIRE(response == NULL);
    REQUIRE(error != NULL);
    if (strstr(error, message_fragment) == NULL) {
        fprintf(stderr, "expected error containing '%s', got '%s'\n",
                message_fragment, error);
        exit(1);
    }
    czarlib_string_free(error);
}

static void write_file(const char *path, const void *contents, size_t size) {
    FILE *file = fopen(path, "wb");
    REQUIRE(file != NULL);
    REQUIRE(fwrite(contents, 1, size, file) == size);
    REQUIRE(fclose(file) == 0);
}

static void read_file(const char *path, void *contents, size_t size) {
    FILE *file = fopen(path, "rb");
    REQUIRE(file != NULL);
    REQUIRE(fread(contents, 1, size, file) == size);
    REQUIRE(fclose(file) == 0);
}

static void copy_file(const char *source, const char *destination) {
    FILE *input = fopen(source, "rb");
    FILE *output = fopen(destination, "wb");
    REQUIRE(input != NULL);
    REQUIRE(output != NULL);
    char buffer[8192];
    size_t count;
    while ((count = fread(buffer, 1, sizeof(buffer), input)) > 0) {
        REQUIRE(fwrite(buffer, 1, count, output) == count);
    }
    REQUIRE(!ferror(input));
    REQUIRE(fclose(input) == 0);
    REQUIRE(fclose(output) == 0);
}

static void require_same_file(const char *left_path, const char *right_path) {
    FILE *left = fopen(left_path, "rb");
    FILE *right = fopen(right_path, "rb");
    REQUIRE(left != NULL);
    REQUIRE(right != NULL);
    int left_byte;
    int right_byte;
    do {
        left_byte = fgetc(left);
        right_byte = fgetc(right);
        REQUIRE(left_byte == right_byte);
    } while (left_byte != EOF);
    REQUIRE(fclose(left) == 0);
    REQUIRE(fclose(right) == 0);
}

static void create_fixture(void) {
    REQUIRE(mkdir("input", 0700) == 0);
    REQUIRE(mkdir("input/nested", 0700) == 0);
    write_file("input/alpha.txt", "alpha contents", 14);
    const uint8_t bytes[128] = {
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
        16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
        32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
        64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
        80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95,
        96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111,
        112, 113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127,
    };
    write_file("input/nested/beta.bin", bytes, sizeof(bytes));
    write_file("input/empty.txt", "", 0);
}

static void test_create_list_check_extract(void) {
    char *created = invoke(
        "create_archive",
        "{\"archive\":\"sample.zar\",\"paths\":[\"input\"],\"block_size\":65536}");
    REQUIRE(strstr(created, "\"block_size\":65536") != NULL);
    czarlib_string_free(created);

    char *listing = invoke("list_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(listing, "\"path\":\"input/alpha.txt\"") != NULL);
    REQUIRE(strstr(listing, "\"size\":14") != NULL);
    REQUIRE(strstr(listing, "\"path\":\"input/nested\"") != NULL);
    REQUIRE(strstr(listing, "\"path\":\"input/empty.txt\"") != NULL);
    czarlib_string_free(listing);

    char *report = invoke("check_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(report, "\"commits\":1") != NULL);
    REQUIRE(strstr(report, "\"logical_state_readable\":true") != NULL);
    REQUIRE(strstr(report, "\"encrypted_blocks\":0") != NULL);
    czarlib_string_free(report);

    char *info = invoke("info_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(info, "\"id\":\"01000000\"") != NULL);
    czarlib_string_free(info);

    char *plan = invoke(
        "extract_archive",
        "{\"archive\":\"sample.zar\",\"destination\":\"dry\",\"dry_run\":true}");
    REQUIRE(strstr(plan, "\"dry_run\":true") != NULL);
    REQUIRE(strstr(plan, "dry/input/alpha.txt") != NULL);
    REQUIRE(access("dry", F_OK) != 0);
    czarlib_string_free(plan);

    plan = invoke(
        "extract_archive",
        "{\"archive\":\"sample.zar\",\"destination\":\"restored\"}");
    czarlib_string_free(plan);
    char contents[14];
    read_file("restored/input/alpha.txt", contents, sizeof(contents));
    REQUIRE(memcmp(contents, "alpha contents", sizeof(contents)) == 0);
    uint8_t bytes[128];
    read_file("restored/input/nested/beta.bin", bytes, sizeof(bytes));
    for (unsigned index = 0; index < sizeof(bytes); ++index) {
        REQUIRE(bytes[index] == index);
    }
    REQUIRE(access("restored/input/empty.txt", F_OK) == 0);
}

static void test_options_filters_and_atomic_failures(void) {
    write_file("keep.txt", "keep", 4);
    write_file("drop.bin", "drop", 4);
    expect_error(
        "create_archive",
        "{\"archive\":\"bad-size.zar\",\"paths\":[\"keep.txt\"],\"block_size\":1}",
        "block_size");
    expect_error(
        "create_archive",
        "{\"archive\":\"bad-level.zar\",\"paths\":[\"keep.txt\"],\"compression_level\":23}",
        "compression_level");

    char *created = invoke(
        "create_archive",
        "{\"archive\":\"filtered.zar\",\"paths\":[\"keep.txt\",\"drop.bin\"],"
        "\"block_size\":65536,\"include_files\":[\"*.txt\"]}");
    czarlib_string_free(created);
    char *listing = invoke("list_archive", "{\"archive\":\"filtered.zar\"}");
    REQUIRE(strstr(listing, "\"path\":\"keep.txt\"") != NULL);
    REQUIRE(strstr(listing, "\"path\":\"drop.bin\"") == NULL);
    czarlib_string_free(listing);

    expect_error(
        "create_archive",
        "{\"archive\":\"filtered.zar\",\"paths\":[\"keep.txt\"],\"block_size\":65536}",
        "already exists");
    created = invoke(
        "create_archive",
        "{\"archive\":\"filtered.zar\",\"paths\":[\"keep.txt\",\"drop.bin\"],"
        "\"block_size\":65536,\"overwrite\":true}");
    czarlib_string_free(created);
    listing = invoke("list_archive", "{\"archive\":\"filtered.zar\"}");
    REQUIRE(strstr(listing, "\"path\":\"drop.bin\"") != NULL);
    czarlib_string_free(listing);
}

static void test_add_replace_remove_and_through(void) {
    write_file("input/alpha.txt", "replaced", 8);
    write_file("input/nested/new.txt", "new", 3);
    char *added = invoke(
        "add_archive",
        "{\"archive\":\"sample.zar\",\"paths\":[\"input/alpha.txt\","
        "\"input/nested/new.txt\"]}");
    czarlib_string_free(added);

    char *files = invoke("extract_archive_to_memory", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(files, "\"input/alpha.txt\":[114,101,112,108,97,99,101,100]") != NULL);
    REQUIRE(strstr(files, "\"input/nested/new.txt\":[110,101,119]") != NULL);
    czarlib_string_free(files);

    char *info = invoke("info_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(info, "\"id\":\"01000000\"") != NULL);
    czarlib_string_free(info);

    char *through = invoke(
        "list_archive",
        "{\"archive\":\"sample.zar\",\"through\":\"01000000\"}");
    REQUIRE(strstr(through, "input/nested/new.txt") == NULL);
    czarlib_string_free(through);
    expect_error(
        "list_archive",
        "{\"archive\":\"sample.zar\",\"through\":\"ffffffff\"}",
        "not found");

    copy_file("sample.zar", "sample-before-failed-remove.zar");
    expect_error(
        "remove_archive",
        "{\"archive\":\"sample.zar\",\"paths\":[\"missing\"]}",
        "not found");
    require_same_file("sample.zar", "sample-before-failed-remove.zar");

    char *through_plan = invoke(
        "extract_archive",
        "{\"archive\":\"sample.zar\",\"destination\":\"through\","
        "\"through\":\"01000000\",\"dry_run\":true}");
    REQUIRE(strstr(through_plan, "new.txt") == NULL);
    czarlib_string_free(through_plan);
    char *removed = invoke(
        "remove_archive",
        "{\"archive\":\"sample.zar\",\"paths\":[\"input/nested\"]}");
    czarlib_string_free(removed);
    char *listing = invoke("list_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(listing, "input/nested") == NULL);
    REQUIRE(strstr(listing, "input/alpha.txt") != NULL);
    czarlib_string_free(listing);
    char *report = invoke("check_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(report, "\"commits\":3") != NULL);
    czarlib_string_free(report);
}

static void test_join_and_suffix_preservation(void) {
    write_file("input/one.txt", "one", 3);
    char *response = invoke(
        "add_archive",
        "{\"archive\":\"sample.zar\",\"paths\":[\"input/one.txt\"]}");
    czarlib_string_free(response);
    write_file("input/two.txt", "two", 3);
    response = invoke(
        "add_archive",
        "{\"archive\":\"sample.zar\",\"paths\":[\"input/two.txt\"]}");
    czarlib_string_free(response);

    char *before = invoke("info_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(before, "\"id\":\"05000000\"") != NULL);
    czarlib_string_free(before);
    response = invoke(
        "join_archive",
        "{\"archive\":\"sample.zar\",\"from_id\":\"03000000\","
        "\"through_id\":\"04000000\"}");
    czarlib_string_free(response);
    char *after = invoke("info_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(after, "\"id\":\"05000000\"") != NULL);
    czarlib_string_free(after);
    char *listing = invoke("list_archive", "{\"archive\":\"sample.zar\"}");
    REQUIRE(strstr(listing, "input/one.txt") != NULL);
    REQUIRE(strstr(listing, "input/two.txt") != NULL);
    czarlib_string_free(listing);
}

static void test_extract_policies_memory_and_encryption(void) {
    REQUIRE(mkdir("existing", 0700) == 0);
    REQUIRE(mkdir("existing/input", 0700) == 0);
    write_file("existing/input/alpha.txt", "keep existing", 13);
    expect_error(
        "extract_archive",
        "{\"archive\":\"sample.zar\",\"destination\":\"existing\"}",
        "already exists");

    char *plan = invoke(
        "extract_archive",
        "{\"archive\":\"sample.zar\",\"destination\":\"existing\","
        "\"existing\":\"skip\"}");
    czarlib_string_free(plan);
    char contents[13];
    read_file("existing/input/alpha.txt", contents, sizeof(contents));
    REQUIRE(memcmp(contents, "keep existing", sizeof(contents)) == 0);
    plan = invoke(
        "extract_archive",
        "{\"archive\":\"sample.zar\",\"destination\":\"existing\","
        "\"existing\":\"overwrite\"}");
    czarlib_string_free(plan);
    read_file("existing/input/alpha.txt", contents, 8);
    REQUIRE(memcmp(contents, "replaced", 8) == 0);

    plan = invoke(
        "extract_archive",
        "{\"archive\":\"sample.zar\",\"destination\":\"filtered-dry\","
        "\"dry_run\":true,\"include_files\":[\"*.txt\"]}");
    REQUIRE(strstr(plan, "input/alpha.txt") != NULL);
    REQUIRE(strstr(plan, "beta.bin") == NULL);
    czarlib_string_free(plan);

    char *memory = invoke(
        "extract_archive_to_memory",
        "{\"archive\":\"sample.zar\",\"offset\":1,\"size_limit\":3,"
        "\"include_files\":[\"*.txt\"],\"exclude_files\":[\"input/empty.txt\"]}");
    REQUIRE(strstr(memory, "\"input/alpha.txt\":[101,112,108]") != NULL);
    czarlib_string_free(memory);
    memory = invoke(
        "extract_archive_to_memory",
        "{\"archive\":\"sample.zar\",\"offset\":1000}");
    REQUIRE(strstr(memory, "\"input/alpha.txt\":[]") != NULL);
    REQUIRE(strstr(memory, "\"input/empty.txt\":[]") != NULL);
    czarlib_string_free(memory);

    char *encrypted = invoke(
        "create_archive",
        "{\"archive\":\"encrypted.zar\",\"paths\":[\"input/alpha.txt\"],"
        "\"block_size\":65536,\"password\":\"secret\"}");
    czarlib_string_free(encrypted);
    char *checked = invoke(
        "check_archive",
        "{\"archive\":\"encrypted.zar\",\"password\":\"secret\"}");
    REQUIRE(strstr(checked, "\"commits\":1") != NULL);
    czarlib_string_free(checked);
    char *unlocked_list = invoke(
        "list_archive",
        "{\"archive\":\"encrypted.zar\",\"password\":\"secret\"}");
    REQUIRE(strstr(unlocked_list, "input/alpha.txt") != NULL);
    czarlib_string_free(unlocked_list);
    char *unlocked_memory = invoke(
        "extract_archive_to_memory",
        "{\"archive\":\"encrypted.zar\",\"password\":\"secret\"}");
    REQUIRE(strstr(unlocked_memory, "\"input/alpha.txt\":[114,101,112,108,97,99,101,100]") != NULL);
    czarlib_string_free(unlocked_memory);
    expect_error(
        "check_archive",
        "{\"archive\":\"encrypted.zar\",\"password\":\"wrong\"}",
        "authentication");
    expect_error(
        "extract_archive",
        "{\"archive\":\"encrypted.zar\",\"destination\":\"locked\"}",
        "password");
    REQUIRE(access("locked", F_OK) != 0);
    copy_file("encrypted.zar", "encrypted-before-failed-add.zar");
    expect_error(
        "add_archive",
        "{\"archive\":\"encrypted.zar\",\"paths\":[\"keep.txt\"]}",
        "encrypted");
    require_same_file("encrypted.zar", "encrypted-before-failed-add.zar");
}

struct thread_args {
    unsigned index;
};

static void *concurrent_worker(void *opaque) {
    struct thread_args *args = opaque;
    char source[64];
    char archive[64];
    char create_request[256];
    char check_request[128];
    char memory_request[128];
    char contents[32];
    (void)snprintf(source, sizeof(source), "thread-%u.bin", args->index);
    (void)snprintf(archive, sizeof(archive), "thread-%u.zar", args->index);
    (void)snprintf(contents, sizeof(contents), "payload-%u", args->index);
    write_file(source, contents, strlen(contents));
    (void)snprintf(create_request, sizeof(create_request),
                   "{\"archive\":\"%s\",\"paths\":[\"%s\"],\"block_size\":65536}",
                   archive, source);
    (void)snprintf(check_request, sizeof(check_request), "{\"archive\":\"%s\"}", archive);
    (void)snprintf(memory_request, sizeof(memory_request), "{\"archive\":\"%s\"}", archive);
    char *response = invoke("create_archive", create_request);
    czarlib_string_free(response);
    response = invoke("check_archive", check_request);
    REQUIRE(strstr(response, "\"commits\":1") != NULL);
    czarlib_string_free(response);
    response = invoke("extract_archive_to_memory", memory_request);
    char expected[160];
    int used = snprintf(expected, sizeof(expected), "\"%s\":[", source);
    REQUIRE(used > 0);
    size_t expected_length = (size_t)used;
    for (size_t index = 0; index < strlen(contents); ++index) {
        used = snprintf(expected + expected_length, sizeof(expected) - expected_length,
                        "%s%u", index ? "," : "", (unsigned char)contents[index]);
        REQUIRE(used > 0);
        expected_length += (size_t)used;
    }
    REQUIRE(expected_length + 2 < sizeof(expected));
    memcpy(expected + expected_length, "]", 2);
    REQUIRE(strstr(response, expected) != NULL);
    czarlib_string_free(response);
    return NULL;
}

static void test_concurrent_calls(void) {
    pthread_t threads[4];
    struct thread_args args[4];
    for (unsigned index = 0; index < 4; ++index) {
        args[index].index = index;
        REQUIRE(pthread_create(&threads[index], NULL, concurrent_worker, &args[index]) == 0);
    }
    for (unsigned index = 0; index < 4; ++index) {
        REQUIRE(pthread_join(threads[index], NULL) == 0);
    }
}

int main(void) {
    char temporary[] = "/tmp/czarlib-test-XXXXXX";
    REQUIRE(mkdtemp(temporary) != NULL);
    REQUIRE(chdir(temporary) == 0);
    create_fixture();
    test_create_list_check_extract();
    test_options_filters_and_atomic_failures();
    test_add_replace_remove_and_through();
    test_join_and_suffix_preservation();
    test_extract_policies_memory_and_encryption();
    test_concurrent_calls();
    puts("czarlib C API tests passed");
    return 0;
}
