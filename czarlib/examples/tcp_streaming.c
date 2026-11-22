#define _POSIX_C_SOURCE 200809L
#include "czarlib.h"
#include "../../examples/tcp-streaming/native.h"

static int read_file(void *ctx, unsigned char *buffer, size_t capacity, size_t *count) {
    FILE *file = ctx;
    *count = fread(buffer, 1, capacity, file);
    return ferror(file) ? 1 : 0;
}

static int seek_file(void *ctx, int32_t origin, int64_t offset, uint64_t *position) {
    FILE *file = ctx;
    const int origins[] = {SEEK_SET, SEEK_CUR, SEEK_END};
    if (origin < 0 || origin > 2 || (int64_t)(off_t)offset != offset ||
        fseeko(file, (off_t)offset, origins[origin])) return 1;
    off_t result = ftello(file);
    if (result < 0) return 1;
    *position = (uint64_t)result;
    return 0;
}

static int write_socket(void *ctx, const unsigned char *data, size_t length, size_t *count) {
    if (zar_frame(*(int *)ctx, data, length)) { *count = 0; return 1; }
    *count = length;
    return 0;
}

static int extract(const char *path, const char *entry, int socket_fd) {
    FILE *file = fopen(path, "rb");
    if (!file) { perror("open archive"); return 1; }
    czarlib_stream archive = {file, read_file, NULL, seek_file};
    czarlib_stream output = {&socket_fd, NULL, write_socket, NULL};
    char *error = NULL;
    uint64_t count;
    int status = czarlib_extract_single_file_to_writer(&archive, entry, &output, "{}", &count, &error);
    if (status) fprintf(stderr, "Extraction failed: %s\n", error ? error : "unknown error");
    else fprintf(stderr, "%s: %llu bytes\n", entry, (unsigned long long)count);
    czarlib_string_free(error);
    if (fclose(file)) { perror("close archive"); return 1; }
    return status;
}

int main(int argc, char **argv) {
    return zar_tcp_main(argc, argv, extract);
}
