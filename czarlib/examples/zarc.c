#include "czarlib.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct buffer {
    char *data;
    size_t length;
    size_t capacity;
};

static int append(struct buffer *buffer, const char *text, size_t length) {
    if (length > (size_t)-1 - buffer->length - 1) {
        return 0;
    }
    size_t needed = buffer->length + length + 1;
    if (needed > buffer->capacity) {
        size_t capacity = buffer->capacity ? buffer->capacity : 128;
        while (capacity < needed) {
            if (capacity > (size_t)-1 / 2) {
                return 0;
            }
            capacity *= 2;
        }
        char *data = realloc(buffer->data, capacity);
        if (!data) {
            return 0;
        }
        buffer->data = data;
        buffer->capacity = capacity;
    }
    memcpy(buffer->data + buffer->length, text, length);
    buffer->length += length;
    buffer->data[buffer->length] = '\0';
    return 1;
}

static int append_text(struct buffer *buffer, const char *text) {
    return append(buffer, text, strlen(text));
}

static int append_json_string(struct buffer *buffer, const char *text) {
    if (!append_text(buffer, "\"")) {
        return 0;
    }
    for (const unsigned char *cursor = (const unsigned char *)text; *cursor; ++cursor) {
        char escaped[7];
        switch (*cursor) {
        case '"':
            if (!append_text(buffer, "\\\"")) return 0;
            break;
        case '\\':
            if (!append_text(buffer, "\\\\")) return 0;
            break;
        case '\b':
            if (!append_text(buffer, "\\b")) return 0;
            break;
        case '\f':
            if (!append_text(buffer, "\\f")) return 0;
            break;
        case '\n':
            if (!append_text(buffer, "\\n")) return 0;
            break;
        case '\r':
            if (!append_text(buffer, "\\r")) return 0;
            break;
        case '\t':
            if (!append_text(buffer, "\\t")) return 0;
            break;
        default:
            if (*cursor < 0x20) {
                (void)snprintf(escaped, sizeof(escaped), "\\u%04x", *cursor);
                if (!append_text(buffer, escaped)) return 0;
            } else if (!append(buffer, (const char *)cursor, 1)) {
                return 0;
            }
        }
    }
    return append_text(buffer, "\"");
}

static const char *operation_name(const char *command) {
    if (!strcmp(command, "create") || !strcmp(command, "c")) return "create_archive";
    if (!strcmp(command, "add") || !strcmp(command, "a")) return "add_archive";
    if (!strcmp(command, "remove") || !strcmp(command, "r")) return "remove_archive";
    if (!strcmp(command, "join") || !strcmp(command, "j")) return "join_archive";
    if (!strcmp(command, "check") || !strcmp(command, "k")) return "check_archive";
    if (!strcmp(command, "info") || !strcmp(command, "i")) return "info_archive";
    if (!strcmp(command, "list") || !strcmp(command, "l")) return "list_archive";
    if (!strcmp(command, "extract") || !strcmp(command, "e")) return "extract_archive";
    return NULL;
}

static void usage(const char *program) {
    fprintf(stderr,
            "Usage: %s COMMAND ...\n"
            "  create|c ARCHIVE PATH...\n"
            "  add|a ARCHIVE PATH...\n"
            "  remove|r ARCHIVE ARCHIVE_PATH...\n"
            "  join|j ARCHIVE --from ID --through ID\n"
            "  check|k ARCHIVE\n"
            "  info|i ARCHIVE\n"
            "  list|l ARCHIVE\n"
            "  extract|e ARCHIVE [DESTINATION] [--dry-run]\n",
            program);
}

int main(int argc, char **argv) {
    if (argc < 2 || !strcmp(argv[1], "--help") || !strcmp(argv[1], "-h")) {
        usage(argv[0]);
        return argc < 2 ? 2 : 0;
    }

    const char *operation = operation_name(argv[1]);
    if (!operation || argc < 3) {
        usage(argv[0]);
        return 2;
    }

    struct buffer request = {0};
    int valid = append_text(&request, "{\"archive\":") &&
                append_json_string(&request, argv[2]);
    if (!valid) {
        free(request.data);
        fputs("zarc-c: unable to build request\n", stderr);
        return 2;
    }

    if (!strcmp(operation, "create_archive") || !strcmp(operation, "add_archive") ||
        !strcmp(operation, "remove_archive")) {
        if (argc < 4 || !append_text(&request, ",\"paths\":[")) {
            free(request.data);
            usage(argv[0]);
            return 2;
        }
        for (int index = 3; index < argc; ++index) {
            if (index > 3 && !append_text(&request, ",")) valid = 0;
            if (!append_json_string(&request, argv[index])) valid = 0;
        }
        if (!append_text(&request, "]")) valid = 0;
    } else if (!strcmp(operation, "join_archive")) {
        if (argc != 7 || strcmp(argv[3], "--from") || strcmp(argv[5], "--through")) {
            free(request.data);
            usage(argv[0]);
            return 2;
        }
        valid = valid && append_text(&request, ",\"from_id\":") &&
                append_json_string(&request, argv[4]) &&
                append_text(&request, ",\"through_id\":") &&
                append_json_string(&request, argv[6]);
    } else if (!strcmp(operation, "extract_archive")) {
        int dry_run = 0;
        const char *destination = "restored";
        for (int index = 3; index < argc; ++index) {
            if (!strcmp(argv[index], "--dry-run")) {
                dry_run = 1;
            } else {
                destination = argv[index];
            }
        }
        valid = valid && append_text(&request, ",\"destination\":") &&
                append_json_string(&request, destination) &&
                append_text(&request, dry_run ? ",\"dry_run\":true" : "");
    }
    valid = valid && append_text(&request, "}");
    if (!valid) {
        free(request.data);
        fputs("zarc-c: request is too large or could not be allocated\n", stderr);
        return 2;
    }

    char *response = NULL;
    char *error = NULL;
    int status = czarlib_execute_json(operation, request.data, &response, &error);
    free(request.data);
    if (status != 0) {
        fprintf(stderr, "zarc-c: %s\n", error ? error : "wrapper error");
        czarlib_string_free(error);
        return 1;
    }
    puts(response ? response : "{}");
    czarlib_string_free(response);
    return 0;
}
