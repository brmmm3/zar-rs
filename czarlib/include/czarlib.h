#ifndef CZARLIB_H
#define CZARLIB_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Run a ZAR archive operation using a UTF-8 JSON object as input.
 *
 * Supported operation names:
 *   create_archive, add_archive, remove_archive, read_custom_metadata,
 *   update_custom_metadata, join_archive, undo_archive,
 *   encrypt_archive_commits, decrypt_archive_commits, check_archive,
 *   info_archive, cross_commit_references, list_archive, grep_archive, extract_archive,
 *   extract_archive_to_memory
 *
 * Returns 0 on success and stores an allocated JSON response in response_json.
 * Returns 1 for an operation/request error and stores the message in
 * error_message. Returns 2 for invalid pointers or an output allocation error,
 * and 3 if a Rust panic is caught. The two output slots are initialized to
 * NULL. Release any returned string with czarlib_string_free().
 *
 * read_custom_metadata accepts {"archive":"..."} and returns an object whose
 * values are strings, signed integers, or finite numbers. update_custom_metadata
 * accepts {"archive":"...","set":{"key":"value"},"remove":["key"]} and
 * returns JSON null. See README.md for details and C examples.
 */
int czarlib_execute_json(
    const char *operation,
    const char *request_json,
    char **response_json,
    char **error_message
);

typedef int (*czarlib_read_callback)(void *context, unsigned char *buffer,
                                     size_t capacity, size_t *count);
typedef int (*czarlib_write_callback)(void *context, const unsigned char *buffer,
                                      size_t length, size_t *count);
typedef int (*czarlib_seek_callback)(void *context, int32_t origin,
                                     int64_t offset, uint64_t *position);

typedef struct {
    void *context;
    czarlib_read_callback read;
    czarlib_write_callback write;
    czarlib_seek_callback seek;
} czarlib_stream;

typedef struct {
    const char *path;
    czarlib_stream stream;
} czarlib_stream_input;

/*
 * Synchronous streaming operations. Pointers and callbacks are never retained.
 * The archive must initially be empty for creation and support write+seek; each
 * input requires read. Extraction requires archive read+seek and output write.
 * Paths and options_json are UTF-8; options_json must be a JSON object.
 * inputs may be NULL only if input_count is zero. written is initialized to zero.
 *
 * Callbacks return zero for success, nonzero for failure; read/write set count
 * to at most the supplied capacity/length. Short transfers are allowed; zero
 * read means EOF, while a zero write makes no progress and fails the operation.
 * Seek origins 0, 1, 2 mean start, current, end; position is the resulting
 * absolute offset, at most INT64_MAX. Callbacks must not unwind across the ABI.
 *
 * Returns 0 on success, 1 for request/operation/callback errors, 2 for invalid
 * pointers or callbacks or error allocation failure, and 3 for a caught Rust
 * panic. error_message is a required writable slot, initialized to NULL and
 * freed with czarlib_string_free. On failure destinations may contain partial
 * data; callers own and must flush/close their streams.
 */
int czarlib_create_archive_from_readers(const czarlib_stream *archive,
    const czarlib_stream_input *inputs, size_t input_count,
    const char *options_json, char **error_message);
int czarlib_extract_single_file_to_writer(const czarlib_stream *archive,
    const char *path, const czarlib_stream *output, const char *options_json,
    uint64_t *written, char **error_message);

/* Free a returned error or JSON response. NULL is accepted. */
void czarlib_string_free(char *value);

#ifdef __cplusplus
}
#endif

#endif
