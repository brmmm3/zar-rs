package gozarlib

import (
	"bytes"
	"errors"
	"io"
	"strings"
	"sync"
	"testing"
)

type memoryArchive struct {
	data []byte
	pos  int64
}

func (m *memoryArchive) Write(p []byte) (int, error) {
	end := m.pos + int64(len(p))
	if end > int64(len(m.data)) {
		m.data = append(m.data, make([]byte, int(end)-len(m.data))...)
	}
	copy(m.data[m.pos:end], p)
	m.pos = end
	return len(p), nil
}

func (m *memoryArchive) Seek(offset int64, whence int) (int64, error) {
	pos := offset
	if whence == io.SeekCurrent {
		pos += m.pos
	} else if whence == io.SeekEnd {
		pos += int64(len(m.data))
	}
	if pos < 0 {
		return 0, errors.New("negative position")
	}
	m.pos = pos
	return pos, nil
}

type shortReader struct{ data []byte }

func (r *shortReader) Read(p []byte) (int, error) {
	if len(r.data) == 0 {
		return 0, io.EOF
	}
	n := copy(p, r.data[:1])
	r.data = r.data[n:]
	return n, nil
}

type delayedReader struct {
	called bool
}

func (r *delayedReader) Read(p []byte) (int, error) {
	if !r.called {
		r.called = true
		return 0, nil
	}
	copy(p, "x")
	return 1, io.EOF
}

type stalledReader struct{}

func (stalledReader) Read([]byte) (int, error) { return 0, nil }

func makeStreamArchive(t *testing.T, options any) []byte {
	t.Helper()
	dst := &memoryArchive{}
	err := CreateArchiveFromReaders(dst, []StreamInput{
		{Path: "nested/file.txt", Reader: &shortReader{data: []byte("stream contents")}},
		{Path: "empty", Reader: strings.NewReader("")},
	}, options)
	if err != nil {
		t.Fatal(err)
	}
	return dst.data
}

func TestStreamingRoundTrip(t *testing.T) {
	archive := makeStreamArchive(t, map[string]any{
		"encoding_options": map[string]any{"block_size": 65536, "compression_level": 1},
	})
	for _, tc := range []struct{ path, want string }{
		{"nested/file.txt", "stream contents"}, {"empty", ""},
	} {
		var dst bytes.Buffer
		n, err := ExtractSingleFileToWriter(bytes.NewReader(archive), tc.path, &dst, nil)
		if err != nil || n != uint64(len(tc.want)) || dst.String() != tc.want {
			t.Fatalf("%s: n=%d output=%q error=%v", tc.path, n, dst.String(), err)
		}
	}
	var dst bytes.Buffer
	if _, err := ExtractSingleFileToWriter(bytes.NewReader(archive), "missing", &dst, nil); err == nil {
		t.Fatal("missing member succeeded")
	}
}

func TestStreamingEncryption(t *testing.T) {
	archive := makeStreamArchive(t, map[string]any{"password": "secret", "aead": "aes-256-gcm", "encryption_scope": "header-and-data"})
	var dst bytes.Buffer
	n, err := ExtractSingleFileToWriter(bytes.NewReader(archive), "nested/file.txt", &dst, map[string]any{"password": "secret"})
	if err != nil || n != uint64(len("stream contents")) || dst.String() != "stream contents" {
		t.Fatalf("encrypted extraction: n=%d output=%q error=%v", n, dst.String(), err)
	}
	if _, err := ExtractSingleFileToWriter(bytes.NewReader(archive), "nested/file.txt", io.Discard, map[string]any{"password": "wrong"}); err == nil {
		t.Fatal("incorrect password succeeded")
	}
}

var errStreamTest = errors.New("stream test failure")

type failingReader struct{}

func (failingReader) Read(p []byte) (int, error) { p[0] = 'x'; return 1, errStreamTest }

type failingWriter struct{}

func (failingWriter) Write(p []byte) (int, error) { return len(p) / 2, errStreamTest }

type shortWriter struct{}

func (shortWriter) Write(p []byte) (int, error) { return len(p) / 2, nil }

type panicWriter struct{}

func (panicWriter) Write([]byte) (int, error) { panic("writer failed") }

type failingSeeker struct{ *bytes.Reader }

func (failingSeeker) Seek(int64, int) (int64, error) { return 0, errStreamTest }

func TestStreamingCallbackFailures(t *testing.T) {
	err := CreateArchiveFromReaders(&memoryArchive{}, []StreamInput{{Path: "file", Reader: failingReader{}}}, nil)
	if !errors.Is(err, errStreamTest) {
		t.Fatalf("reader error not preserved: %v", err)
	}
	err = CreateArchiveFromReaders(&memoryArchive{}, []StreamInput{{Path: "file", Reader: stalledReader{}}}, nil)
	if !errors.Is(err, io.ErrNoProgress) {
		t.Fatalf("stalled reader: %v", err)
	}
	delayed := &memoryArchive{}
	err = CreateArchiveFromReaders(delayed, []StreamInput{{Path: "file", Reader: &delayedReader{}}}, nil)
	if err != nil {
		t.Fatalf("reader with delayed EOF: %v", err)
	}
	var data bytes.Buffer
	if _, err := ExtractSingleFileToWriter(bytes.NewReader(delayed.data), "file", &data, nil); err != nil || data.String() != "x" {
		t.Fatalf("reader with delayed EOF: output=%q error=%v", data.String(), err)
	}
	archive := makeStreamArchive(t, nil)
	n, err := ExtractSingleFileToWriter(bytes.NewReader(archive), "nested/file.txt", failingWriter{}, nil)
	if !errors.Is(err, errStreamTest) || n != uint64(len("stream contents")/2) {
		t.Fatalf("writer failure: n=%d err=%v", n, err)
	}
	n, err = ExtractSingleFileToWriter(bytes.NewReader(archive), "nested/file.txt", shortWriter{}, nil)
	if !errors.Is(err, io.ErrShortWrite) || n != uint64(len("stream contents")/2) {
		t.Fatalf("short write: n=%d err=%v", n, err)
	}
	if _, err := ExtractSingleFileToWriter(bytes.NewReader(archive), "nested/file.txt", panicWriter{}, nil); err == nil || !strings.Contains(err.Error(), "writer failed") {
		t.Fatalf("panic not converted: %v", err)
	}
	if _, err := ExtractSingleFileToWriter(failingSeeker{bytes.NewReader(archive)}, "nested/file.txt", io.Discard, nil); !errors.Is(err, errStreamTest) {
		t.Fatalf("seek error not preserved: %v", err)
	}
}

func TestStreamingValidationAndConcurrency(t *testing.T) {
	if err := CreateArchiveFromReaders(&memoryArchive{}, []StreamInput{{Path: "bad\x00path", Reader: strings.NewReader("")}}, nil); err == nil {
		t.Fatal("NUL path accepted")
	}
	if err := CreateArchiveFromReaders(&memoryArchive{}, nil, []int{1}); err == nil {
		t.Fatal("non-object options accepted")
	}
	archive := makeStreamArchive(t, nil)
	var wg sync.WaitGroup
	for i := 0; i < 8; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			created := &memoryArchive{}
			if err := CreateArchiveFromReaders(created, []StreamInput{{Path: "thread", Reader: strings.NewReader("ok")}}, nil); err != nil {
				t.Errorf("concurrent creation: %v", err)
				return
			}
			var dst bytes.Buffer
			n, err := ExtractSingleFileToWriter(bytes.NewReader(archive), "nested/file.txt", &dst, nil)
			if err != nil || n != 15 || dst.String() != "stream contents" {
				t.Errorf("concurrent extraction: n=%d output=%q error=%v", n, dst.String(), err)
			}
		}()
	}
	wg.Wait()
}
