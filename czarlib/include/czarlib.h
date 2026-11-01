#ifndef CZARLIB_H
#define CZARLIB_H

#ifdef __cplusplus
extern "C" {
#endif

/*
 * Run a pyzarlib-compatible operation using a UTF-8 JSON object as input.
 *
 * Supported operation names:
 *   create_archive, add_archive, remove_archive, join_archive,
 *   check_archive, info_archive, list_archive, extract_archive,
 *   extract_archive_to_memory
 *
 * Returns 0 on success and stores an allocated JSON response in response_json.
 * Returns 1 for an operation/request error and stores the message in
 * error_message. Returns 2 for invalid pointers or an output allocation error,
 * and 3 if a Rust panic is caught. The two output slots are initialized to
 * NULL. Release any returned string with czarlib_string_free().
 *
 * Request/response schemas and C examples are documented in README.md.
 */
int czarlib_execute_json(
    const char *operation,
    const char *request_json,
    char **response_json,
    char **error_message
);

/* Free a string returned by czarlib_execute_json(). NULL is accepted. */
void czarlib_string_free(char *value);

#ifdef __cplusplus
}
#endif

#endif
