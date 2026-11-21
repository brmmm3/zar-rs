#include <stdlib.h>
#include "stream_bridge.h"

extern int goStreamRead(uintptr_t, unsigned char *, size_t, size_t *);
extern int goStreamWrite(uintptr_t, unsigned char *, size_t, size_t *);
extern int goStreamSeek(uintptr_t, int32_t, int64_t, uint64_t *);

static int stream_read(void *context, unsigned char *buf, size_t len, size_t *count) {
    return goStreamRead(*(uintptr_t *)context, buf, len, count);
}

static int stream_write(void *context, const unsigned char *buf, size_t len, size_t *count) {
    return goStreamWrite(*(uintptr_t *)context, (unsigned char *)buf, len, count);
}

static int stream_seek(void *context, int32_t origin, int64_t offset, uint64_t *position) {
    return goStreamSeek(*(uintptr_t *)context, origin, offset, position);
}

czarlib_stream go_stream_new(uintptr_t handle, int readable, int writable, int seekable) {
    czarlib_stream stream = {0};
    uintptr_t *context = malloc(sizeof(*context));
    if (!context) return stream;
    *context = handle;
    stream.context = context;
    stream.read = readable ? stream_read : NULL;
    stream.write = writable ? stream_write : NULL;
    stream.seek = seekable ? stream_seek : NULL;
    return stream;
}

void go_stream_free(czarlib_stream *stream) {
    free(stream->context);
    stream->context = NULL;
}
