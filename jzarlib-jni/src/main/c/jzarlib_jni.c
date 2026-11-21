#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "czarlib.h"

static char *copy_java_bytes(JNIEnv *env, jbyteArray input) {
    if (input == NULL) {
        return NULL;
    }
    jsize length = (*env)->GetArrayLength(env, input);
    char *result = malloc((size_t)length + 1);
    if (result == NULL) {
        return NULL;
    }
    (*env)->GetByteArrayRegion(env, input, 0, length, (jbyte *)result);
    if ((*env)->ExceptionCheck(env)) {
        free(result);
        return NULL;
    }
    if (memchr(result, '\0', (size_t)length) != NULL) {
        free(result);
        return NULL;
    }
    result[length] = '\0';
    return result;
}

static jstring new_java_string(JNIEnv *env, const char *input) {
    if (input == NULL) {
        return NULL;
    }
    const unsigned char *source = (const unsigned char *)input;
    size_t input_length = strlen(input);
    jchar *utf16 = malloc((input_length + 1) * sizeof(jchar));
    if (utf16 == NULL) {
        return NULL;
    }

    size_t out_length = 0;
    for (size_t index = 0; index < input_length;) {
        uint32_t codepoint;
        unsigned char first = source[index++];
        if (first < 0x80) {
            codepoint = first;
        } else if ((first & 0xe0) == 0xc0 && index < input_length) {
            codepoint = ((uint32_t)(first & 0x1f) << 6)
                | (uint32_t)(source[index++] & 0x3f);
            if (codepoint < 0x80) goto invalid_utf8;
        } else if ((first & 0xf0) == 0xe0 && index + 1 < input_length) {
            codepoint = ((uint32_t)(first & 0x0f) << 12)
                | ((uint32_t)(source[index] & 0x3f) << 6)
                | (uint32_t)(source[index + 1] & 0x3f);
            index += 2;
            if (codepoint < 0x800 || (codepoint >= 0xd800 && codepoint <= 0xdfff)) {
                goto invalid_utf8;
            }
        } else if ((first & 0xf8) == 0xf0 && index + 2 < input_length) {
            codepoint = ((uint32_t)(first & 0x07) << 18)
                | ((uint32_t)(source[index] & 0x3f) << 12)
                | ((uint32_t)(source[index + 1] & 0x3f) << 6)
                | (uint32_t)(source[index + 2] & 0x3f);
            index += 3;
            if (codepoint < 0x10000 || codepoint > 0x10ffff) goto invalid_utf8;
        } else {
            goto invalid_utf8;
        }

        if (codepoint <= 0xffff) {
            utf16[out_length++] = (jchar)codepoint;
        } else {
            codepoint -= 0x10000;
            utf16[out_length++] = (jchar)(0xd800 + (codepoint >> 10));
            utf16[out_length++] = (jchar)(0xdc00 + (codepoint & 0x3ff));
        }
    }
    jstring result = (*env)->NewString(env, utf16, (jsize)out_length);
    free(utf16);
    return result;

invalid_utf8:
    free(utf16);
    return NULL;
}

static jobjectArray make_result(
    JNIEnv *env, const char *response, const char *error) {
    jclass string_class = (*env)->FindClass(env, "java/lang/String");
    if (string_class == NULL) {
        return NULL;
    }
    jobjectArray result = (*env)->NewObjectArray(env, 2, string_class, NULL);
    if (result == NULL) {
        return NULL;
    }
    if (response != NULL) {
        jstring value = new_java_string(env, response);
        if (value == NULL) {
            return NULL;
        }
        (*env)->SetObjectArrayElement(env, result, 0, value);
        (*env)->DeleteLocalRef(env, value);
    }
    if (error != NULL) {
        jstring value = new_java_string(env, error);
        if (value == NULL) {
            return NULL;
        }
        (*env)->SetObjectArrayElement(env, result, 1, value);
        (*env)->DeleteLocalRef(env, value);
    }
    return result;
}

JNIEXPORT jobjectArray JNICALL
Java_org_zar_jarlib_jni_ZarLibrary_executeNative(
    JNIEnv *env, jclass class, jbyteArray operation_bytes, jbyteArray request_bytes) {
    (void)class;
    char *operation = copy_java_bytes(env, operation_bytes);
    char *request = copy_java_bytes(env, request_bytes);
    if ((*env)->ExceptionCheck(env)) {
        free(operation);
        free(request);
        return NULL;
    }
    if (operation == NULL || request == NULL) {
        free(operation);
        free(request);
        return make_result(env, NULL, "invalid or unallocatable JNI input");
    }

    char *response = NULL;
    char *error = NULL;
    int status = czarlib_execute_json(operation, request, &response, &error);
    free(operation);
    free(request);

    const char *message = error;
    if (status == 2 && message == NULL) {
        message = "czarlib rejected its arguments or could not allocate output";
    } else if (status == 3 && message == NULL) {
        message = "czarlib caught a Rust panic";
    } else if (status != 0 && message == NULL) {
        message = "czarlib operation failed without an error message";
    }

    jobjectArray result = make_result(
        env, status == 0 ? response : NULL, status == 0 ? NULL : message);
    czarlib_string_free(response);
    czarlib_string_free(error);
    return result;
}

typedef struct {
    JNIEnv *env;
    jmethodID read;
    jmethodID write;
    jmethodID seek;
    jthrowable exception;
} stream_call;

typedef struct {
    stream_call *call;
    jobject bridge;
} stream_context;

static void capture_exception(stream_call *call) {
    JNIEnv *env = call->env;
    if ((*env)->ExceptionCheck(env)) {
        jthrowable exception = (*env)->ExceptionOccurred(env);
        (*env)->ExceptionClear(env);
        if (call->exception == NULL) {
            call->exception = exception;
        } else {
            (*env)->DeleteLocalRef(env, exception);
        }
    }
}

static void throw_named(JNIEnv *env, const char *name, const char *message) {
    jclass type = (*env)->FindClass(env, name);
    if (type != NULL) {
        (*env)->ThrowNew(env, type, message);
        (*env)->DeleteLocalRef(env, type);
    }
}

static int stream_read(void *opaque, unsigned char *buffer, size_t length, size_t *count) {
    stream_context *context = opaque;
    stream_call *call = context->call;
    JNIEnv *env = call->env;
    *count = 0;
    if (call->exception != NULL) return 1;
    if (length == 0) return 0;
    jsize size = (jsize)(length > 65536 ? 65536 : length);
    jbyteArray bytes = (*env)->NewByteArray(env, size);
    if (bytes == NULL) {
        capture_exception(call);
        return 1;
    }
    jint n = (*env)->CallIntMethod(env, context->bridge, call->read, bytes);
    if (!(*env)->ExceptionCheck(env) && (n < 0 || n > size)) {
        throw_named(env, "java/io/IOException", "invalid stream read count");
    }
    if (!(*env)->ExceptionCheck(env) && n > 0) {
        (*env)->GetByteArrayRegion(env, bytes, 0, n, (jbyte *)buffer);
    }
    capture_exception(call);
    (*env)->DeleteLocalRef(env, bytes);
    if (call->exception != NULL) return 1;
    *count = (size_t)n;
    return 0;
}

static int stream_write(void *opaque, const unsigned char *buffer, size_t length, size_t *count) {
    stream_context *context = opaque;
    stream_call *call = context->call;
    JNIEnv *env = call->env;
    *count = 0;
    if (call->exception != NULL) return 1;
    if (length == 0) return 0;
    jsize size = (jsize)(length > 65536 ? 65536 : length);
    jbyteArray bytes = (*env)->NewByteArray(env, size);
    if (bytes == NULL) {
        capture_exception(call);
        return 1;
    }
    (*env)->SetByteArrayRegion(env, bytes, 0, size, (const jbyte *)buffer);
    jint n = 0;
    if (!(*env)->ExceptionCheck(env)) {
        n = (*env)->CallIntMethod(env, context->bridge, call->write, bytes);
    }
    if (!(*env)->ExceptionCheck(env) && (n <= 0 || n > size)) {
        throw_named(env, "java/io/IOException", "invalid stream write count");
    }
    capture_exception(call);
    (*env)->DeleteLocalRef(env, bytes);
    if (call->exception != NULL) return 1;
    *count = (size_t)n;
    return 0;
}

static int stream_seek(void *opaque, int32_t origin, int64_t offset, uint64_t *position) {
    stream_context *context = opaque;
    stream_call *call = context->call;
    JNIEnv *env = call->env;
    *position = 0;
    if (call->exception != NULL) return 1;
    jlong result = (*env)->CallLongMethod(env, context->bridge, call->seek,
                                           (jint)origin, (jlong)offset);
    if (!(*env)->ExceptionCheck(env) && result < 0) {
        throw_named(env, "java/io/IOException", "invalid stream seek position");
    }
    capture_exception(call);
    if (call->exception != NULL) return 1;
    *position = (uint64_t)result;
    return 0;
}

static int init_stream_call(stream_call *call, JNIEnv *env, jobject bridge) {
    memset(call, 0, sizeof(*call));
    call->env = env;
    jclass type = (*env)->GetObjectClass(env, bridge);
    if (type == NULL) return 0;
    call->read = (*env)->GetMethodID(env, type, "read", "([B)I");
    if (call->read != NULL) call->write = (*env)->GetMethodID(env, type, "write", "([B)I");
    if (call->write != NULL) call->seek = (*env)->GetMethodID(env, type, "seek", "(IJ)J");
    (*env)->DeleteLocalRef(env, type);
    return call->seek != NULL;
}

static jstring stream_result(JNIEnv *env, stream_call *call, int status, char *error) {
    jstring result = NULL;
    if (call->exception != NULL) {
        (*env)->Throw(env, call->exception);
        (*env)->DeleteLocalRef(env, call->exception);
    } else if (status != 0) {
        const char *message = error != NULL ? error : "czarlib streaming operation failed";
        result = new_java_string(env, message);
        if (result == NULL && !(*env)->ExceptionCheck(env)) {
            throw_named(env, "java/lang/OutOfMemoryError", "could not convert native error");
        }
    }
    czarlib_string_free(error);
    return result;
}

JNIEXPORT jstring JNICALL
Java_org_zar_jarlib_jni_ZarLibrary_createFromReadersNative(
    JNIEnv *env, jclass type, jobject archive, jobjectArray paths,
    jobjectArray inputs, jbyteArray options_bytes) {
    (void)type;
    stream_call call;
    if (!init_stream_call(&call, env, archive)) return NULL;
    jsize count = (*env)->GetArrayLength(env, paths);
    czarlib_stream_input *native_inputs = calloc((size_t)count + 1, sizeof(*native_inputs));
    stream_context *contexts = calloc((size_t)count + 1, sizeof(*contexts));
    char *options = copy_java_bytes(env, options_bytes);
    if (native_inputs == NULL || contexts == NULL || options == NULL) {
        if (!(*env)->ExceptionCheck(env))
            throw_named(env, "java/lang/OutOfMemoryError", "could not allocate stream inputs");
        goto cleanup;
    }
    for (jsize i = 0; i < count; i++) {
        jbyteArray path = (jbyteArray)(*env)->GetObjectArrayElement(env, paths, i);
        if ((*env)->ExceptionCheck(env)) goto cleanup;
        native_inputs[i].path = copy_java_bytes(env, path);
        (*env)->DeleteLocalRef(env, path);
        if (native_inputs[i].path == NULL) {
            if (!(*env)->ExceptionCheck(env))
                throw_named(env, "java/lang/OutOfMemoryError", "could not allocate stream path");
            goto cleanup;
        }
        jobject bridge = (*env)->GetObjectArrayElement(env, inputs, i);
        if ((*env)->ExceptionCheck(env)) goto cleanup;
        contexts[i].bridge = (*env)->NewGlobalRef(env, bridge);
        (*env)->DeleteLocalRef(env, bridge);
        if (contexts[i].bridge == NULL) goto cleanup;
        contexts[i].call = &call;
        native_inputs[i].stream = (czarlib_stream){&contexts[i], stream_read, NULL, NULL};
    }
    stream_context archive_context = {&call, archive};
    czarlib_stream archive_stream = {&archive_context, stream_read, stream_write, stream_seek};
    char *error = NULL;
    int status = czarlib_create_archive_from_readers(
        &archive_stream, native_inputs, (size_t)count, options, &error);
    jstring result = stream_result(env, &call, status, error);
    for (jsize i = 0; i < count; i++) {
        free((void *)native_inputs[i].path);
        if (contexts[i].bridge != NULL) (*env)->DeleteGlobalRef(env, contexts[i].bridge);
    }
    free(native_inputs);
    free(contexts);
    free(options);
    return result;

cleanup:
    if (native_inputs != NULL) {
        for (jsize i = 0; i < count; i++) free((void *)native_inputs[i].path);
    }
    if (contexts != NULL) {
        for (jsize i = 0; i < count; i++) {
            if (contexts[i].bridge != NULL) (*env)->DeleteGlobalRef(env, contexts[i].bridge);
        }
    }
    free(native_inputs);
    free(contexts);
    free(options);
    return NULL;
}

JNIEXPORT jstring JNICALL
Java_org_zar_jarlib_jni_ZarLibrary_extractToWriterNative(
    JNIEnv *env, jclass type, jobject archive, jbyteArray path_bytes,
    jobject output, jbyteArray options_bytes, jlongArray written_array) {
    (void)type;
    stream_call call;
    if (!init_stream_call(&call, env, archive)) return NULL;
    char *path = copy_java_bytes(env, path_bytes);
    char *options = (*env)->ExceptionCheck(env) ? NULL : copy_java_bytes(env, options_bytes);
    if (path == NULL || options == NULL) {
        free(path);
        free(options);
        if (!(*env)->ExceptionCheck(env))
            throw_named(env, "java/lang/OutOfMemoryError", "could not allocate stream arguments");
        return NULL;
    }
    stream_context archive_context = {&call, archive};
    stream_context output_context = {&call, output};
    czarlib_stream archive_stream = {&archive_context, stream_read, NULL, stream_seek};
    czarlib_stream output_stream = {&output_context, NULL, stream_write, NULL};
    uint64_t written = 0;
    char *error = NULL;
    int status = czarlib_extract_single_file_to_writer(
        &archive_stream, path, &output_stream, options, &written, &error);
    free(path);
    free(options);
    if (status == 0 && call.exception == NULL) {
        if (written > INT64_MAX) {
            throw_named(env, "java/io/IOException", "extracted byte count exceeds Java long");
        } else {
            jlong value = (jlong)written;
            (*env)->SetLongArrayRegion(env, written_array, 0, 1, &value);
        }
    }
    return stream_result(env, &call, status, error);
}
