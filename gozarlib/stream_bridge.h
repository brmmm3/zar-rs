#include <stdint.h>
#include "czarlib.h"

czarlib_stream go_stream_new(uintptr_t handle, int readable, int writable, int seekable);
void go_stream_free(czarlib_stream *stream);
