#include <lua.h>
#include <lauxlib.h>
#include "czarlib.h"

#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

_Static_assert(LUA_MAXINTEGER >= INT64_MAX, "Lua must use 64-bit integers");

typedef struct {
    lua_State *lua;
    int index;
    unsigned char *read_buffer;
    const unsigned char *write_buffer;
    size_t size;
    size_t count;
    uint64_t position;
    int32_t origin;
    int64_t offset;
    uint64_t written;
    const char *method;
    char error[512];
} stream_context;

static const char *checked_string(lua_State *lua, int index, const char *name) {
    size_t length;
    luaL_checktype(lua, index, LUA_TSTRING);
    const char *value = luaL_checklstring(lua, index, &length);
    if (memchr(value, '\0', length))
        luaL_error(lua, "%s must not contain NUL", name);
    return value;
}

static int finish(lua_State *lua, int status, char *message) {
    if (status == 0) {
        czarlib_string_free(message);
        lua_pushboolean(lua, 1);
        return 1;
    }
    lua_pushnil(lua);
    if (message) {
        lua_pushstring(lua, message);
        czarlib_string_free(message);
    } else {
        lua_pushfstring(lua, "czarlib failed with status %d", status);
    }
    return 2;
}

static int execute_json(lua_State *lua) {
    const char *operation = checked_string(lua, 1, "operation");
    const char *request = checked_string(lua, 2, "request");
    char *response = NULL;
    char *error = NULL;
    int status = czarlib_execute_json(operation, request, &response, &error);
    if (status != 0 || !response) {
        czarlib_string_free(response);
        return finish(lua, status == 0 ? 2 : status, error);
    }
    czarlib_string_free(error);
    lua_pushstring(lua, response);
    czarlib_string_free(response);
    return 1;
}

/* All Lua calls made by a stream callback are protected by lua_pcall. */
static int callback_call(lua_State *lua) {
    stream_context *ctx = lua_touserdata(lua, 2);
    const char *method = ctx->method;
    lua_getfield(lua, 1, method);
    if (!lua_isfunction(lua, -1))
        return luaL_error(lua, "stream has no %s method", method);
    lua_pushvalue(lua, 1);
    if (strcmp(method, "read") == 0) {
        lua_pushinteger(lua, (lua_Integer)ctx->size);
        lua_call(lua, 2, 2);
        if (!lua_isnil(lua, -1)) {
            const char *error = lua_tostring(lua, -1);
            return luaL_error(lua, "read: %s", error ? error : "failed");
        }
        if (lua_isnil(lua, -2)) {
            ctx->count = 0;
        } else {
            size_t length;
            if (lua_type(lua, -2) != LUA_TSTRING)
                return luaL_error(lua, "read must return a string or nil");
            const char *data = lua_tolstring(lua, -2, &length);
            if (length > ctx->size)
                return luaL_error(lua, "read returned too many bytes");
            memcpy(ctx->read_buffer, data, length);
            ctx->count = length;
        }
    } else if (strcmp(method, "write") == 0) {
        if (ctx->size > (uint64_t)LUA_MAXINTEGER ||
            ctx->written > (uint64_t)LUA_MAXINTEGER - ctx->size)
            return luaL_error(lua, "written byte count exceeds Lua integer range");
        lua_pushlstring(lua, (const char *)ctx->write_buffer, ctx->size);
        lua_call(lua, 2, 2);
        if (!lua_isnil(lua, -1) || lua_isnil(lua, -2) ||
            (lua_isboolean(lua, -2) && !lua_toboolean(lua, -2))) {
            const char *error = lua_tostring(lua, -1);
            return luaL_error(lua, "write: %s", error ? error : "failed");
        }
        if (lua_isinteger(lua, -2)) {
            lua_Integer count = lua_tointeger(lua, -2);
            if (count < 0 || (uint64_t)count > ctx->size)
                return luaL_error(lua, "write returned an invalid byte count");
            ctx->count = (size_t)count;
        } else if (lua_isboolean(lua, -2) || lua_istable(lua, -2) || lua_isuserdata(lua, -2)) {
            ctx->count = ctx->size;
        } else {
            return luaL_error(lua, "write must return a byte count or success value");
        }
        ctx->written += ctx->count;
    } else {
        static const char *origins[] = {"set", "cur", "end"};
        if (ctx->origin < 0 || ctx->origin > 2)
            return luaL_error(lua, "invalid seek origin");
        lua_pushstring(lua, origins[ctx->origin]);
        lua_pushinteger(lua, (lua_Integer)ctx->offset);
        lua_call(lua, 3, 2);
        if (!lua_isinteger(lua, -2) || lua_tointeger(lua, -2) < 0) {
            const char *error = lua_tostring(lua, -1);
            return luaL_error(lua, "seek: %s", error ? error : "invalid position");
        }
        ctx->position = (uint64_t)lua_tointeger(lua, -2);
    }
    return 0;
}

static int callback(stream_context *ctx, const char *method) {
    lua_State *lua = ctx->lua;
    ctx->method = method;
    lua_pushcfunction(lua, callback_call);
    lua_pushvalue(lua, ctx->index);
    lua_pushlightuserdata(lua, ctx);
    if (lua_pcall(lua, 2, 0, 0) != LUA_OK) {
        const char *error = lua_type(lua, -1) == LUA_TSTRING ? lua_tostring(lua, -1) : NULL;
        snprintf(ctx->error, sizeof(ctx->error), "%s", error ? error : "stream callback failed");
        lua_pop(lua, 1);
        return 1;
    }
    return 0;
}

static int stream_read(void *context, unsigned char *buffer, size_t capacity, size_t *count) {
    stream_context *ctx = context;
    ctx->read_buffer = buffer;
    ctx->size = capacity > (size_t)LUA_MAXINTEGER ? (size_t)LUA_MAXINTEGER : capacity;
    ctx->count = 0;
    int status = callback(ctx, "read");
    *count = ctx->count;
    return status;
}

static int stream_write(void *context, const unsigned char *buffer, size_t length, size_t *count) {
    stream_context *ctx = context;
    ctx->write_buffer = buffer;
    ctx->size = length;
    ctx->count = 0;
    int status = callback(ctx, "write");
    *count = ctx->count;
    return status;
}

static int stream_seek(void *context, int32_t origin, int64_t offset, uint64_t *position) {
    stream_context *ctx = context;
    ctx->origin = origin;
    ctx->offset = offset;
    int status = callback(ctx, "seek");
    *position = ctx->position;
    return status;
}

static czarlib_stream stream(stream_context *ctx, lua_State *lua, int index,
                             int read, int write, int seek) {
    memset(ctx, 0, sizeof(*ctx));
    ctx->lua = lua;
    ctx->index = lua_absindex(lua, index);
    return (czarlib_stream){ctx, read ? stream_read : NULL,
                            write ? stream_write : NULL, seek ? stream_seek : NULL};
}

static int stream_finish(lua_State *lua, int status, char *message,
                         stream_context *contexts, size_t count, uint64_t written,
                         int return_count) {
    if (status == 0 && return_count) {
        czarlib_string_free(message);
        if (written > (uint64_t)LUA_MAXINTEGER)
            return luaL_error(lua, "written byte count exceeds Lua integer range");
        lua_pushinteger(lua, (lua_Integer)written);
        return 1;
    }
    if (status != 0) {
        for (size_t i = 0; i < count; i++) {
            if (contexts[i].error[0]) {
                czarlib_string_free(message);
                lua_pushnil(lua);
                lua_pushstring(lua, contexts[i].error);
                if (return_count) {
                    lua_pushinteger(lua, (lua_Integer)contexts[count - 1].written);
                    return 3;
                }
                return 2;
            }
        }
        if (return_count) {
            finish(lua, status, message);
            lua_pushinteger(lua, (lua_Integer)contexts[count - 1].written);
            return 3;
        }
    }
    return finish(lua, status, message);
}

static int create_from_readers(lua_State *lua) {
    luaL_checktype(lua, 2, LUA_TTABLE);
    const char *options = checked_string(lua, 3, "options");
    size_t count = lua_rawlen(lua, 2);
    if (count > (size_t)(INT_MAX - 16) / 3 ||
        count > (SIZE_MAX / sizeof(czarlib_stream_input)) ||
        count + 1 > (SIZE_MAX / sizeof(stream_context)))
        return luaL_error(lua, "too many inputs");
    luaL_checkstack(lua, (int)(3 * count + 16), "too many inputs");
    stream_context *contexts = lua_newuserdatauv(lua, (count + 1) * sizeof(*contexts), 0);
    czarlib_stream_input *inputs = lua_newuserdatauv(lua, count * sizeof(*inputs), 0);
    czarlib_stream archive = stream(&contexts[0], lua, 1, 0, 1, 1);
    for (size_t i = 0; i < count; i++) {
        lua_rawgeti(lua, 2, (lua_Integer)i + 1);
        luaL_checktype(lua, -1, LUA_TTABLE);
        lua_getfield(lua, -1, "path");
        inputs[i].path = checked_string(lua, -1, "input path");
        lua_getfield(lua, -2, "reader");
        if (!lua_istable(lua, -1) && !lua_isuserdata(lua, -1))
            return luaL_error(lua, "input reader must be a stream");
        inputs[i].stream = stream(&contexts[i + 1], lua, -1, 1, 0, 0);
    }
    char *error = NULL;
    int status = czarlib_create_archive_from_readers(&archive, inputs, count, options, &error);
    return stream_finish(lua, status, error, contexts, count + 1, 0, 0);
}

static int extract_to_writer(lua_State *lua) {
    const char *path = checked_string(lua, 2, "path");
    const char *options = checked_string(lua, 4, "options");
    luaL_checkstack(lua, 16, "stream callback");
    stream_context contexts[2];
    czarlib_stream archive = stream(&contexts[0], lua, 1, 1, 0, 1);
    czarlib_stream output = stream(&contexts[1], lua, 3, 0, 1, 0);
    uint64_t written = 0;
    char *error = NULL;
    int status = czarlib_extract_single_file_to_writer(
        &archive, path, &output, options, &written, &error);
    return stream_finish(lua, status, error, contexts, 2, written, 1);
}

int luaopen__luazarlib(lua_State *lua) {
    static const luaL_Reg functions[] = {
        {"execute_json", execute_json},
        {"create_archive_from_readers", create_from_readers},
        {"extract_single_file_to_writer", extract_to_writer},
        {NULL, NULL},
    };
    luaL_newlib(lua, functions);
    return 1;
}
