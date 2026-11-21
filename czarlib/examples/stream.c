#include "czarlib.h"

#include <limits.h>
#include <stdio.h>

static int file_read(void *context, unsigned char *buffer, size_t capacity,
                     size_t *count) {
    FILE *file = context;
    *count = fread(buffer, 1, capacity, file);
    return ferror(file) ? 1 : 0;
}

static int file_write(void *context, const unsigned char *buffer, size_t length,
                      size_t *count) {
    FILE *file = context;
    *count = fwrite(buffer, 1, length, file);
    return ferror(file) ? 1 : 0;
}

static int file_seek(void *context, int32_t origin, int64_t offset,
                     uint64_t *position) {
    FILE *file = context;
    if (origin < 0 || origin > 2 || offset < LONG_MIN || offset > LONG_MAX)
        return 1;
    const int origins[] = {SEEK_SET, SEEK_CUR, SEEK_END};
    if (fseek(file, (long)offset, origins[origin]) != 0)
        return 1;
    long result = ftell(file);
    if (result < 0)
        return 1;
    *position = (uint64_t)result;
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 5) {
        fprintf(stderr, "usage: %s INPUT ARCHIVE ENTRY OUTPUT\n", argv[0]);
        return 1;
    }
    FILE *input = fopen(argv[1], "rb");
    if (!input) {
        perror("open input");
        return 1;
    }
    FILE *archive = fopen(argv[2], "w+b");
    if (!archive) {
        perror("open archive");
        fclose(input);
        return 1;
    }
    FILE *output = fopen(argv[4], "wb");
    if (!output) {
        perror("open output");
        fclose(input);
        fclose(archive);
        return 1;
    }

    czarlib_stream archive_stream = {archive, file_read, file_write, file_seek};
    czarlib_stream_input source = {argv[3], {input, file_read, NULL, NULL}};
    czarlib_stream destination = {output, NULL, file_write, NULL};
    char *error = NULL;
    uint64_t written = 0;
    int status = czarlib_create_archive_from_readers(
        &archive_stream, &source, 1, "{\"block_size\":65536}", &error);
    if (status == 0 && fflush(archive) != 0) {
        perror("flush archive");
        status = 1;
    }
    if (status == 0)
        status = czarlib_extract_single_file_to_writer(
            &archive_stream, argv[3], &destination, "{}", &written, &error);
    if (status != 0)
        fprintf(stderr, "streaming failed: %s\n", error ? error : "I/O or ABI error");
    czarlib_string_free(error);
    int close_error = fclose(input);
    close_error |= fclose(archive);
    close_error |= fclose(output);
    if (close_error != 0) {
        perror("close stream");
        return 1;
    }
    if (status != 0)
        return 1;
    printf("Extracted %llu bytes\n", (unsigned long long)written);
    return 0;
}
