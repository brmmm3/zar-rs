package main

import (
	"bytes"
	"fmt"
	"log"
	"os"
	"strings"

	"github.com/brmmm3/zar-rs/gozarlib"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() error {
	archive, err := os.CreateTemp(".", "stream-example-*.zar")
	if err != nil {
		return err
	}
	defer os.Remove(archive.Name())
	defer archive.Close()

	err = gozarlib.CreateArchiveFromReaders(archive, []gozarlib.StreamInput{
		{Path: "greeting.txt", Reader: strings.NewReader("Hello, streaming ZAR!\n")},
	}, nil)
	if err != nil {
		return err
	}

	var contents bytes.Buffer
	n, err := gozarlib.ExtractSingleFileToWriter(archive, "greeting.txt", &contents, nil)
	if err != nil {
		return err
	}
	fmt.Printf("Extracted %d bytes: %s", n, contents.String())
	return nil
}
