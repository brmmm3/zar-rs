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
