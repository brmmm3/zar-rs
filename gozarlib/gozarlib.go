package gozarlib

/*
#cgo CFLAGS: -I${SRCDIR}/../czarlib/include
#cgo LDFLAGS: -L${SRCDIR}/../target/debug -lczarlib
#cgo linux LDFLAGS: -Wl,-rpath,${SRCDIR}/../target/debug
#cgo darwin LDFLAGS: -Wl,-rpath,${SRCDIR}/../target/debug
#include <stdlib.h>
#include "czarlib.h"
*/
import "C"

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"strings"
	"unicode/utf8"
	"unsafe"
)

// ExecuteJSON sends a JSON object to a named archive operation and returns its
// JSON response. requestJSON must be UTF-8 encoded JSON. The operation must not
// contain a NUL byte.
func ExecuteJSON(operation string, requestJSON []byte) (json.RawMessage, error) {
	if operation == "" {
		return nil, errors.New("operation must not be empty")
	}
	if strings.IndexByte(operation, 0) >= 0 {
		return nil, errors.New("operation must not contain NUL")
	}
	if !utf8.ValidString(operation) || !utf8.Valid(requestJSON) {
		return nil, errors.New("operation and request must be UTF-8")
	}
	if !json.Valid(requestJSON) {
		return nil, errors.New("request must be valid JSON")
	}
	trimmed := bytes.TrimSpace(requestJSON)
	if len(trimmed) == 0 || trimmed[0] != '{' {
		return nil, errors.New("request JSON must be an object")
	}

	cOperation := C.CString(operation)
	defer C.free(unsafe.Pointer(cOperation))
	cRequest := C.CString(string(requestJSON))
	defer C.free(unsafe.Pointer(cRequest))

	var response *C.char
	var message *C.char
	status := C.czarlib_execute_json(
		cOperation,
		cRequest,
		&response,
		&message,
	)
	if response != nil {
		defer C.czarlib_string_free(response)
	}
	if message != nil {
		defer C.czarlib_string_free(message)
	}

	switch status {
	case 0:
		if response == nil {
			return nil, errors.New("czarlib returned success without a response")
		}
		result := C.GoString(response)
		if !json.Valid([]byte(result)) {
			return nil, errors.New("czarlib returned invalid JSON")
		}
		return json.RawMessage(result), nil
	case 1:
		if message == nil {
			return nil, errors.New("czarlib operation failed without an error message")
		}
		return nil, errors.New(C.GoString(message))
	case 2:
		return nil, errors.New("czarlib rejected its arguments or could not allocate output")
	case 3:
		if message != nil {
			return nil, fmt.Errorf("czarlib caught a Rust panic: %s", C.GoString(message))
		}
		return nil, errors.New("czarlib caught a Rust panic")
	default:
		return nil, fmt.Errorf("czarlib returned unknown status %d", int(status))
	}
}

func execute(operation string, request any) (json.RawMessage, error) {
	encoded, err := json.Marshal(request)
	if err != nil {
		return nil, fmt.Errorf("encode %s request: %w", operation, err)
	}
	return ExecuteJSON(operation, encoded)
}

// CreateArchive creates a new archive. Request fields match the czarlib
// create_archive JSON request.
func CreateArchive(request any) (json.RawMessage, error) {
	return execute("create_archive", request)
}

// AddArchive appends a commit. Request fields match the czarlib add_archive
// JSON request.
func AddArchive(request any) (json.RawMessage, error) {
	return execute("add_archive", request)
}

// RemoveArchive appends tombstones for archive-relative paths.
func RemoveArchive(request any) (json.RawMessage, error) {
	return execute("remove_archive", request)
}

// JoinArchive compacts the requested inclusive commit range.
func JoinArchive(request any) (json.RawMessage, error) {
	return execute("join_archive", request)
}

// CheckArchive validates archive framing and readable content.
func CheckArchive(request any) (json.RawMessage, error) {
	return execute("check_archive", request)
}

// InfoArchive returns commit and archive information.
func InfoArchive(request any) (json.RawMessage, error) {
	return execute("info_archive", request)
}

// ListArchive returns the effective archive table of contents.
func ListArchive(request any) (json.RawMessage, error) {
	return execute("list_archive", request)
}

// ExtractArchive extracts selected files to a destination directory.
func ExtractArchive(request any) (json.RawMessage, error) {
	return execute("extract_archive", request)
}

// ExtractArchiveToMemory returns selected file contents as JSON byte arrays.
func ExtractArchiveToMemory(request any) (json.RawMessage, error) {
	return execute("extract_archive_to_memory", request)
}
