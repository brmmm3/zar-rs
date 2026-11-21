package gozarlib

/*
#include <stdlib.h>
#include "stream_bridge.h"
*/
import "C"

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"runtime/cgo"
	"strings"
	"sync"
	"unicode/utf8"
	"unsafe"
)

// StreamInput names a regular file and supplies its contents.
type StreamInput struct {
	Path   string
	Reader io.Reader
}

type streamState struct {
	mu      sync.Mutex
	reader  io.Reader
	writer  io.Writer
	seeker  io.Seeker
	pending error
	err     error
	written uint64
}

func (s *streamState) fail(err error) {
	if s.err == nil {
		s.err = err
	}
}

func (s *streamState) recoverCallback(status *C.int) {
	if p := recover(); p != nil {
		s.fail(fmt.Errorf("stream callback panic: %v", p))
		*status = 1
	}
}

func streamBuffer(ptr *C.uchar, size C.size_t) ([]byte, error) {
	if uint64(size) > uint64(^uint(0)>>1) || (ptr == nil && size != 0) {
		return nil, errors.New("invalid stream callback buffer")
	}
	return unsafe.Slice((*byte)(unsafe.Pointer(ptr)), int(size)), nil
}

//export goStreamRead
func goStreamRead(handle C.uintptr_t, ptr *C.uchar, size C.size_t, count *C.size_t) (status C.int) {
	s := cgo.Handle(handle).Value().(*streamState)
	s.mu.Lock()
	defer s.mu.Unlock()
	defer s.recoverCallback(&status)
	*count = 0
	if s.pending != nil {
		err := s.pending
		if !errors.Is(err, io.EOF) {
			s.fail(err)
			return 1
		}
		return 0
	}
	buf, err := streamBuffer(ptr, size)
	if err != nil {
		s.fail(err)
		return 1
	}
	if len(buf) == 0 {
		return 0
	}
	var n int
	for attempts := 0; attempts < 100; attempts++ {
		n, err = s.reader.Read(buf)
		if n != 0 || err != nil {
			break
		}
	}
	if n < 0 || n > len(buf) {
		s.fail(fmt.Errorf("invalid read count %d", n))
		return 1
	}
	*count = C.size_t(n)
	if n > 0 {
		s.pending = err
		if err != nil && !errors.Is(err, io.EOF) {
			s.fail(err)
		}
		return 0
	}
	if err == nil {
		err = io.ErrNoProgress
	}
	if !errors.Is(err, io.EOF) {
		s.fail(err)
		return 1
	}
	s.pending = io.EOF
	return 0
}

//export goStreamWrite
func goStreamWrite(handle C.uintptr_t, ptr *C.uchar, size C.size_t, count *C.size_t) (status C.int) {
	s := cgo.Handle(handle).Value().(*streamState)
	s.mu.Lock()
	defer s.mu.Unlock()
	defer s.recoverCallback(&status)
	*count = 0
	if s.err != nil {
		return 1
	}
	buf, err := streamBuffer(ptr, size)
	if err != nil {
		s.fail(err)
		return 1
	}
	if len(buf) == 0 {
		return 0
	}
	n, err := s.writer.Write(buf)
	if n < 0 || n > len(buf) {
		s.fail(fmt.Errorf("invalid write count %d", n))
		return 1
	}
	*count = C.size_t(n)
	s.written += uint64(n)
	if err == nil && n != len(buf) {
		err = io.ErrShortWrite
	}
	if err != nil {
		s.fail(err)
		if n == 0 {
			return 1
		}
	}
	return 0
}

//export goStreamSeek
func goStreamSeek(handle C.uintptr_t, origin C.int32_t, offset C.int64_t, position *C.uint64_t) (status C.int) {
	s := cgo.Handle(handle).Value().(*streamState)
	s.mu.Lock()
	defer s.mu.Unlock()
	defer s.recoverCallback(&status)
	*position = 0
	if s.err != nil {
		return 1
	}
	if origin < 0 || origin > 2 {
		s.fail(fmt.Errorf("invalid seek origin %d", int(origin)))
		return 1
	}
	pos, err := s.seeker.Seek(int64(offset), int(origin))
	if err == nil && pos < 0 {
		err = errors.New("negative seek position")
	}
	if err != nil {
		s.fail(err)
		return 1
	}
	s.pending = nil
	*position = C.uint64_t(pos)
	return 0
}

type nativeStream struct {
	value  C.czarlib_stream
	handle cgo.Handle
	state  *streamState
}

func newNativeStream(s *streamState) (*nativeStream, error) {
	h := cgo.NewHandle(s)
	flag := func(b bool) C.int {
		if b {
			return 1
		}
		return 0
	}
	v := C.go_stream_new(C.uintptr_t(h), flag(s.reader != nil), flag(s.writer != nil), flag(s.seeker != nil))
	if v.context == nil {
		h.Delete()
		return nil, errors.New("allocate stream context")
	}
	return &nativeStream{value: v, handle: h, state: s}, nil
}

func (s *nativeStream) close() {
	C.go_stream_free(&s.value)
	s.handle.Delete()
}

func streamOptions(options any) (*C.char, error) {
	if options == nil {
		return C.CString("{}"), nil
	}
	data, err := json.Marshal(options)
	if err != nil {
		return nil, fmt.Errorf("encode stream options: %w", err)
	}
	if trimmed := bytes.TrimSpace(data); len(trimmed) == 0 || trimmed[0] != '{' {
		return nil, errors.New("stream options must be a JSON object")
	}
	return C.CString(string(data)), nil
}

func streamPath(path string) error {
	if path == "" || !utf8.ValidString(path) || strings.IndexByte(path, 0) >= 0 {
		return errors.New("stream path must be nonempty UTF-8 without NUL")
	}
	return nil
}

func streamResult(status C.int, message *C.char, streams ...*nativeStream) error {
	if message != nil {
		defer C.czarlib_string_free(message)
	}
	var errs []error
	for _, stream := range streams {
		if stream.state.err != nil {
			errs = append(errs, stream.state.err)
		}
	}
	if status != 0 {
		if message != nil {
			errs = append(errs, fmt.Errorf("czarlib stream error (status %d): %s", int(status), C.GoString(message)))
		} else {
			errs = append(errs, fmt.Errorf("czarlib stream error (status %d)", int(status)))
		}
	}
	return errors.Join(errs...)
}

// CreateArchiveFromReaders writes a new archive to an empty, seekable destination.
// options is nil or a JSON-encodable object. On failure dst may contain a partial archive.
func CreateArchiveFromReaders(dst io.WriteSeeker, inputs []StreamInput, options any) error {
	if dst == nil {
		return errors.New("archive destination is nil")
	}
	cOptions, err := streamOptions(options)
	if err != nil {
		return err
	}
	defer C.free(unsafe.Pointer(cOptions))
	output, err := newNativeStream(&streamState{writer: dst, seeker: dst})
	if err != nil {
		return err
	}
	defer output.close()
	streams := []*nativeStream{output}
	defer func() {
		for _, s := range streams[1:] {
			s.close()
		}
	}()
	var nativeInputs *C.czarlib_stream_input
	if len(inputs) > 0 {
		nativeInputs = (*C.czarlib_stream_input)(C.calloc(C.size_t(len(inputs)), C.size_t(C.sizeof_czarlib_stream_input)))
		if nativeInputs == nil {
			return errors.New("allocate stream inputs")
		}
		defer C.free(unsafe.Pointer(nativeInputs))
	}
	items := unsafe.Slice(nativeInputs, len(inputs))
	for i, input := range inputs {
		if err := streamPath(input.Path); err != nil {
			return fmt.Errorf("input %d: %w", i, err)
		}
		if input.Reader == nil {
			return fmt.Errorf("input %d: reader is nil", i)
		}
		stream, err := newNativeStream(&streamState{reader: input.Reader})
		if err != nil {
			return err
		}
		streams = append(streams, stream)
		items[i].path = C.CString(input.Path)
		defer C.free(unsafe.Pointer(items[i].path))
		items[i].stream = stream.value
	}
	var message *C.char
	status := C.czarlib_create_archive_from_readers(&output.value, nativeInputs, C.size_t(len(inputs)), cOptions, &message)
	return streamResult(status, message, streams...)
}

// ExtractSingleFileToWriter writes an archive member to dst and returns the
// number of bytes accepted by dst, including partial output on failure.
func ExtractSingleFileToWriter(archive io.ReadSeeker, path string, dst io.Writer, options any) (uint64, error) {
	if archive == nil || dst == nil {
		return 0, errors.New("archive and destination must not be nil")
	}
	if err := streamPath(path); err != nil {
		return 0, err
	}
	cOptions, err := streamOptions(options)
	if err != nil {
		return 0, err
	}
	defer C.free(unsafe.Pointer(cOptions))
	source, err := newNativeStream(&streamState{reader: archive, seeker: archive})
	if err != nil {
		return 0, err
	}
	defer source.close()
	output, err := newNativeStream(&streamState{writer: dst})
	if err != nil {
		return 0, err
	}
	defer output.close()
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))
	var written C.uint64_t
	var message *C.char
	status := C.czarlib_extract_single_file_to_writer(&source.value, cPath, &output.value, cOptions, &written, &message)
	return output.state.written, streamResult(status, message, source, output)
}
