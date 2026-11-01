package main

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/brmmm3/zar-rs/gozarlib"
)

func main() {
	if len(os.Args) != 3 {
		fmt.Fprintf(os.Stderr, "Usage: %s ARCHIVE INPUT_FILE\n", os.Args[0])
		os.Exit(2)
	}
	archive, input := os.Args[1], os.Args[2]
	request := func(fields map[string]any) json.RawMessage {
		encoded, err := json.Marshal(fields)
		if err != nil {
			panic(err)
		}
		return encoded
	}
	printResponse := func(label string, response json.RawMessage, err error) {
		if err != nil {
			fmt.Fprintf(os.Stderr, "%s: %v\n", label, err)
			os.Exit(1)
		}
		fmt.Printf("%s: %s\n", label, response)
	}

	created, err := gozarlib.CreateArchive(map[string]any{
		"archive": archive, "paths": []string{input},
	})
	printResponse("created", created, err)

	listed, err := gozarlib.ListArchive(request(map[string]any{"archive": archive}))
	printResponse("contents", listed, err)

	checked, err := gozarlib.CheckArchive(map[string]any{"archive": archive})
	printResponse("validated", checked, err)

	files, err := gozarlib.ExtractArchiveToMemory(map[string]any{
		"archive": archive, "size_limit": 1024, "offset": 0,
	})
	printResponse("in-memory extraction", files, err)

	info, err := gozarlib.InfoArchive(map[string]any{"archive": archive})
	printResponse("archive info", info, err)
}
