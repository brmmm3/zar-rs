package gozarlib

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
)

func call(t *testing.T, operation string, request any) string {
	t.Helper()
	response, err := execute(operation, request)
	if err != nil {
		t.Fatalf("%s failed: %v", operation, err)
	}
	return string(response)
}

func requireContains(t *testing.T, value, fragment string) {
	t.Helper()
	if !strings.Contains(value, fragment) {
		t.Fatalf("expected %q to contain %q", value, fragment)
	}
}

func requireNotContains(t *testing.T, value, fragment string) {
	t.Helper()
	if strings.Contains(value, fragment) {
		t.Fatalf("expected %q not to contain %q", value, fragment)
	}
}

func expectError(t *testing.T, operation string, request any, fragment string) {
	t.Helper()
	_, err := execute(operation, request)
	if err == nil {
		t.Fatalf("%s unexpectedly succeeded", operation)
	}
	requireContains(t, err.Error(), fragment)
}

func writeFixture(t *testing.T, name string, contents []byte) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(name), 0o700); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(name, contents, 0o600); err != nil {
		t.Fatal(err)
	}
}

func createFixture(t *testing.T) {
	t.Helper()
	writeFixture(t, "input/alpha.txt", []byte("alpha contents"))
	binary := make([]byte, 128)
	for index := range binary {
		binary[index] = byte(index)
	}
	writeFixture(t, "input/nested/beta.bin", binary)
	writeFixture(t, "input/empty.txt", nil)
}

func assertMemoryFile(t *testing.T, response, path string, contents []byte) {
	t.Helper()
	var files map[string][]byte
	if err := json.Unmarshal([]byte(response), &files); err != nil {
		t.Fatalf("decode in-memory result: %v", err)
	}
	got, ok := files[path]
	if !ok {
		t.Fatalf("memory extraction omitted %q: %s", path, response)
	}
	if string(got) != string(contents) {
		t.Fatalf("memory extraction for %q = %v, want %v", path, got, contents)
	}
}

func TestFullArchiveOperations(t *testing.T) {
	temp := t.TempDir()
	oldWorkingDirectory, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err = os.Chdir(temp); err != nil {
		t.Fatal(err)
	}
	defer func() {
		if chdirErr := os.Chdir(oldWorkingDirectory); chdirErr != nil {
			t.Errorf("restore working directory: %v", chdirErr)
		}
	}()

	createFixture(t)
	if got := call(t, "create_archive", map[string]any{
		"archive": "sample.zar", "paths": []string{"input"}, "block_size": 65536,
	}); !strings.Contains(got, `"block_size":65536`) {
		t.Fatalf("create response omitted block size: %s", got)
	}

	listing := call(t, "list_archive", map[string]any{"archive": "sample.zar"})
	for _, path := range []string{
		`"path":"input/alpha.txt"`, `"path":"input/nested"`,
		`"path":"input/empty.txt"`,
	} {
		requireContains(t, listing, path)
	}
	requireContains(t, listing, `"size":14`)

	report := call(t, "check_archive", map[string]any{"archive": "sample.zar"})
	requireContains(t, report, `"commits":1`)
	requireContains(t, report, `"logical_state_readable":true`)
	requireContains(t, report, `"encrypted_blocks":0`)
	requireContains(t, call(t, "info_archive", map[string]any{"archive": "sample.zar"}),
		`"id":"00000001"`)

	plan := call(t, "extract_archive", map[string]any{
		"archive": "sample.zar", "destination": "dry", "dry_run": true,
	})
	requireContains(t, plan, `"dry_run":true`)
	requireContains(t, plan, "dry/input/alpha.txt")
	if _, err := os.Stat("dry"); !os.IsNotExist(err) {
		t.Fatal("dry-run created its destination")
	}
	call(t, "extract_archive", map[string]any{
		"archive": "sample.zar", "destination": "restored",
	})
	if contents, err := os.ReadFile("restored/input/alpha.txt"); err != nil ||
		string(contents) != "alpha contents" {
		t.Fatalf("extracted text file = %q, err=%v", contents, err)
	}

	writeFixture(t, "keep.txt", []byte("keep"))
	writeFixture(t, "drop.bin", []byte("drop"))
	expectError(t, "create_archive", map[string]any{
		"archive": "bad-size.zar", "paths": []string{"keep.txt"}, "block_size": 1,
	}, "block_size")
	expectError(t, "create_archive", map[string]any{
		"archive": "bad-level.zar", "paths": []string{"keep.txt"},
		"compression_level": 23,
	}, "compression_level")
	call(t, "create_archive", map[string]any{
		"archive": "filtered.zar", "paths": []string{"keep.txt", "drop.bin"},
		"block_size": 65536, "include_files": []string{"*.txt"},
	})
	listing = call(t, "list_archive", map[string]any{"archive": "filtered.zar"})
	requireContains(t, listing, `"path":"keep.txt"`)
	requireNotContains(t, listing, `"path":"drop.bin"`)
	expectError(t, "create_archive", map[string]any{
		"archive": "filtered.zar", "paths": []string{"keep.txt"}, "block_size": 65536,
	}, "already exists")
	call(t, "create_archive", map[string]any{
		"archive": "filtered.zar", "paths": []string{"keep.txt", "drop.bin"},
		"block_size": 65536, "overwrite": true,
	})

	writeFixture(t, "input/alpha.txt", []byte("replaced"))
	writeFixture(t, "input/nested/new.txt", []byte("new"))
	call(t, "add_archive", map[string]any{
		"archive": "sample.zar",
		"paths":   []string{"input/alpha.txt", "input/nested/new.txt"},
	})
	memory := call(t, "extract_archive_to_memory", map[string]any{
		"archive": "sample.zar",
	})
	assertMemoryFile(t, memory, "input/alpha.txt", []byte("replaced"))
	assertMemoryFile(t, memory, "input/nested/new.txt", []byte("new"))
	through := call(t, "list_archive", map[string]any{
		"archive": "sample.zar", "through": "00000001",
	})
	requireNotContains(t, through, "input/nested/new.txt")
	expectError(t, "list_archive", map[string]any{
		"archive": "sample.zar", "through": "ffffffff",
	}, "not found")
	if _, err := os.Stat("sample.zar"); err != nil {
		t.Fatal(err)
	}
	if err := copyPath("sample.zar", "sample-before-failed-remove.zar"); err != nil {
		t.Fatal(err)
	}
	expectError(t, "remove_archive", map[string]any{
		"archive": "sample.zar", "paths": []string{"missing"},
	}, "not found")
	requireSamePath(t, "sample.zar", "sample-before-failed-remove.zar")
	call(t, "remove_archive", map[string]any{
		"archive": "sample.zar", "paths": []string{"input/nested"},
	})
	listing = call(t, "list_archive", map[string]any{"archive": "sample.zar"})
	requireNotContains(t, listing, "input/nested")
	requireContains(t, listing, "input/alpha.txt")
	requireContains(t, call(t, "check_archive", map[string]any{"archive": "sample.zar"}),
		`"commits":3`)

	writeFixture(t, "input/one.txt", []byte("one"))
	call(t, "add_archive", map[string]any{
		"archive": "sample.zar", "paths": []string{"input/one.txt"},
	})
	writeFixture(t, "input/two.txt", []byte("two"))
	call(t, "add_archive", map[string]any{
		"archive": "sample.zar", "paths": []string{"input/two.txt"},
	})
	requireContains(t, call(t, "info_archive", map[string]any{"archive": "sample.zar"}),
		`"id":"00000005"`)
	call(t, "join_archive", map[string]any{
		"archive": "sample.zar", "from_id": "00000003", "through_id": "00000004",
	})
	listing = call(t, "list_archive", map[string]any{"archive": "sample.zar"})
	requireContains(t, listing, "input/one.txt")
	requireContains(t, listing, "input/two.txt")
	requireContains(t, call(t, "info_archive", map[string]any{"archive": "sample.zar"}),
		`"id":"00000005"`)

	if err := os.MkdirAll("existing/input", 0o700); err != nil {
		t.Fatal(err)
	}
	writeFixture(t, "existing/input/alpha.txt", []byte("keep existing"))
	expectError(t, "extract_archive", map[string]any{
		"archive": "sample.zar", "destination": "existing",
	}, "already exists")
	call(t, "extract_archive", map[string]any{
		"archive": "sample.zar", "destination": "existing", "existing": "skip",
	})
	contents, err := os.ReadFile("existing/input/alpha.txt")
	if err != nil || string(contents) != "keep existing" {
		t.Fatalf("skip policy changed existing contents to %q: %v", contents, err)
	}
	call(t, "extract_archive", map[string]any{
		"archive": "sample.zar", "destination": "existing", "existing": "overwrite",
	})
	contents, err = os.ReadFile("existing/input/alpha.txt")
	if err != nil || string(contents) != "replaced" {
		t.Fatalf("overwrite policy result %q: %v", contents, err)
	}

	plan = call(t, "extract_archive", map[string]any{
		"archive": "sample.zar", "destination": "filtered-dry", "dry_run": true,
		"include_files": []string{"*.txt"},
	})
	requireContains(t, plan, "input/alpha.txt")
	requireNotContains(t, plan, "beta.bin")
	memory = call(t, "extract_archive_to_memory", map[string]any{
		"archive": "sample.zar", "offset": 1, "size_limit": 3,
		"include_files": []string{"*.txt"},
		"exclude_files": []string{"input/empty.txt"},
	})
	assertMemoryFile(t, memory, "input/alpha.txt", []byte("epl"))
	memory = call(t, "extract_archive_to_memory", map[string]any{
		"archive": "sample.zar", "offset": 1000,
	})
	assertMemoryFile(t, memory, "input/alpha.txt", nil)
	assertMemoryFile(t, memory, "input/empty.txt", nil)

	encrypted, err := CreateArchive(map[string]any{
		"archive": "encrypted.zar", "paths": []string{"input/alpha.txt"},
		"block_size": 65536, "password": "secret",
	})
	if err != nil {
		t.Fatal(err)
	}
	requireContains(t, string(encrypted), `"aead":"aes-256-gcm"`)
	requireContains(t, call(t, "check_archive", map[string]any{
		"archive": "encrypted.zar", "password": "secret",
	}), `"commits":1`)
	requireContains(t, call(t, "list_archive", map[string]any{
		"archive": "encrypted.zar", "password": "secret",
	}), "input/alpha.txt")
	memory = call(t, "extract_archive_to_memory", map[string]any{
		"archive": "encrypted.zar", "password": "secret",
	})
	assertMemoryFile(t, memory, "input/alpha.txt", []byte("replaced"))
	expectError(t, "check_archive", map[string]any{
		"archive": "encrypted.zar", "password": "wrong",
	}, "authentication")
	expectError(t, "extract_archive", map[string]any{
		"archive": "encrypted.zar", "destination": "locked",
	}, "password")
	if _, err := os.Stat("locked"); !os.IsNotExist(err) {
		t.Fatal("failed encrypted extraction created output")
	}
	if err := copyPath("encrypted.zar", "encrypted-before-failed-add.zar"); err != nil {
		t.Fatal(err)
	}
	expectError(t, "add_archive", map[string]any{
		"archive": "encrypted.zar", "paths": []string{"keep.txt"},
	}, "encrypted")
	requireSamePath(t, "encrypted.zar", "encrypted-before-failed-add.zar")

	encrypted, err = CreateArchive(map[string]any{
		"archive": "chacha.zar", "paths": []string{"input/alpha.txt"},
		"block_size": 65536, "password": "secret", "aead": "chacha20-poly1305",
	})
	if err != nil {
		t.Fatal(err)
	}
	requireContains(t, string(encrypted), `"aead":"chacha20-poly1305"`)
	call(t, "check_archive", map[string]any{
		"archive": "chacha.zar", "password": "secret",
	})
	call(t, "extract_archive", map[string]any{
		"archive": "chacha.zar", "destination": "chacha-restored",
		"password": "secret",
	})
	call(t, "add_archive", map[string]any{
		"archive": "chacha.zar", "paths": []string{"input/alpha.txt"},
		"block_size": 65536, "password": "second-secret",
		"passwords": map[string]string{"00000001": "secret"},
		"aead":      "aes-256-gcm", "encryption_scope": "data",
	})
	memory = call(t, "extract_archive_to_memory", map[string]any{
		"archive": "chacha.zar",
		"passwords": map[string]string{
			"00000001": "secret", "00000002": "second-secret",
		},
	})
	assertMemoryFile(t, memory, "input/alpha.txt", []byte("replaced"))
	requireContains(t, call(t, "info_archive", map[string]any{
		"archive": "chacha.zar",
	}), `"data_encrypted":true`)

	testConcurrentCalls(t)
}

func testConcurrentCalls(t *testing.T) {
	t.Helper()
	const count = 4
	errors := make(chan error, count)
	var workers sync.WaitGroup
	for index := 0; index < count; index++ {
		workers.Add(1)
		go func(index int) {
			defer workers.Done()
			source := fmt.Sprintf("thread-%d.bin", index)
			archive := fmt.Sprintf("thread-%d.zar", index)
			contents := fmt.Sprintf("payload-%d", index)
			if err := os.WriteFile(source, []byte(contents), 0o600); err != nil {
				errors <- err
				return
			}
			_, err := CreateArchive(map[string]any{
				"archive": archive, "paths": []string{source}, "block_size": 65536,
			})
			if err != nil {
				errors <- err
				return
			}
			report, err := CheckArchive(map[string]any{"archive": archive})
			if err != nil {
				errors <- err
				return
			}
			if !strings.Contains(string(report), `"commits":1`) {
				errors <- fmt.Errorf("concurrent check returned: %s", report)
				return
			}
			files, err := ExtractArchiveToMemory(map[string]any{"archive": archive})
			if err != nil {
				errors <- err
				return
			}
			var extracted map[string][]byte
			if err := json.Unmarshal(files, &extracted); err != nil {
				errors <- err
				return
			}
			if string(extracted[source]) != contents {
				errors <- fmt.Errorf("concurrent memory extraction mismatch for %s", source)
			}
		}(index)
	}
	workers.Wait()
	close(errors)
	for err := range errors {
		if err != nil {
			t.Error(err)
		}
	}
}

func TestExecuteJSONValidation(t *testing.T) {
	for _, test := range []struct {
		name      string
		operation string
		request   []byte
		fragment  string
	}{
		{"empty operation", "", []byte(`{}`), "operation must not be empty"},
		{"bad json", "info_archive", []byte("{"), "valid JSON"},
		{"non-object", "info_archive", []byte(`[]`), "must be an object"},
		{"unknown operation", "unknown", []byte(`{}`), "unknown operation"},
		{"nul operation", "info_archive\x00", []byte(`{}`), "NUL"},
		{"invalid utf8", "info_archive", []byte{0xff}, "UTF-8"},
	} {
		t.Run(test.name, func(t *testing.T) {
			_, err := ExecuteJSON(test.operation, test.request)
			if err == nil {
				t.Fatal("expected an error")
			}
			requireContains(t, err.Error(), test.fragment)
		})
	}
}

func copyPath(source, destination string) error {
	contents, err := os.ReadFile(source)
	if err != nil {
		return err
	}
	return os.WriteFile(destination, contents, 0o600)
}

func requireSamePath(t *testing.T, left, right string) {
	t.Helper()
	leftContents, err := os.ReadFile(left)
	if err != nil {
		t.Fatal(err)
	}
	rightContents, err := os.ReadFile(right)
	if err != nil {
		t.Fatal(err)
	}
	if string(leftContents) != string(rightContents) {
		t.Fatalf("%s differs from %s", left, right)
	}
}
