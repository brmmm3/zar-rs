package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"strings"

	"github.com/brmmm3/zar-rs/gozarlib"
)

func usage(program string) {
	fmt.Fprintf(os.Stderr, "Usage: %s COMMAND ...\n", program)
	fmt.Fprintln(os.Stderr, "  create|c ARCHIVE PATH...")
	fmt.Fprintln(os.Stderr, "  add|a ARCHIVE PATH...")
	fmt.Fprintln(os.Stderr, "  remove|r ARCHIVE ARCHIVE_PATH...")
	fmt.Fprintln(os.Stderr, "  join|j ARCHIVE --from ID --through ID")
	fmt.Fprintln(os.Stderr, "  check|k ARCHIVE")
	fmt.Fprintln(os.Stderr, "  info|i ARCHIVE")
	fmt.Fprintln(os.Stderr, "  list|l ARCHIVE")
	fmt.Fprintln(os.Stderr, "  extract|e ARCHIVE [DESTINATION] [--dry-run]")
	fmt.Fprintln(os.Stderr, "  memory ARCHIVE")
}

func main() {
	if len(os.Args) < 2 || os.Args[1] == "--help" || os.Args[1] == "-h" {
		usage(os.Args[0])
		if len(os.Args) == 1 {
			os.Exit(2)
		}
		return
	}

	command := os.Args[1]
	operation := map[string]string{
		"create": "create_archive", "c": "create_archive",
		"add": "add_archive", "a": "add_archive",
		"remove": "remove_archive", "r": "remove_archive",
		"join": "join_archive", "j": "join_archive",
		"check": "check_archive", "k": "check_archive",
		"info": "info_archive", "i": "info_archive",
		"list": "list_archive", "l": "list_archive",
		"extract": "extract_archive", "e": "extract_archive",
		"memory": "extract_archive_to_memory",
	}[command]
	if operation == "" || len(os.Args) < 3 {
		usage(os.Args[0])
		os.Exit(2)
	}

	archive := os.Args[2]
	request := map[string]any{"archive": archive}
	args := os.Args[3:]
	switch operation {
	case "create_archive", "add_archive", "remove_archive":
		if len(args) == 0 {
			usage(os.Args[0])
			os.Exit(2)
		}
		request["paths"] = args
	case "join_archive":
		if len(args) != 4 || args[0] != "--from" || args[2] != "--through" {
			usage(os.Args[0])
			os.Exit(2)
		}
		request["from_id"] = args[1]
		request["through_id"] = args[3]
	case "extract_archive":
		destination := "restored"
		for _, arg := range args {
			if arg == "--dry-run" {
				request["dry_run"] = true
			} else {
				destination = arg
			}
		}
		request["destination"] = destination
	}

	response, err := gozarlib.ExecuteJSON(operation, marshal(request))
	if err != nil {
		fmt.Fprintf(os.Stderr, "zarc-go: %v\n", err)
		os.Exit(1)
	}
	var pretty bytes.Buffer
	if json.Indent(&pretty, response, "", "  ") == nil {
		fmt.Println(strings.TrimSpace(pretty.String()))
	} else {
		fmt.Println(string(response))
	}
}

func marshal(value any) []byte {
	encoded, err := json.Marshal(value)
	if err != nil {
		fmt.Fprintf(os.Stderr, "zarc-go: encode request: %v\n", err)
		os.Exit(1)
	}
	return encoded
}
