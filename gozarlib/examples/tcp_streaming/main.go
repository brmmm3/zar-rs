// Transfer one archive entry over TCP. For trusted networks only.
package main

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"os"

	"github.com/brmmm3/zar-rs/gozarlib"
)

const maxChunk = 65536

func writeLength(w io.Writer, n uint32) error {
	return binary.Write(w, binary.BigEndian, n)
}

func readLength(r io.Reader) (uint32, error) {
	var n uint32
	err := binary.Read(r, binary.BigEndian, &n)
	return n, err
}

type frameWriter struct{ io.Writer }

func (w frameWriter) Write(data []byte) (int, error) {
	written := 0
	for len(data) > 0 {
		size := min(len(data), maxChunk)
		if err := writeLength(w.Writer, uint32(size)); err != nil {
			return written, err
		}
		n, err := w.Writer.Write(data[:size])
		written += n
		if err != nil {
			return written, err
		}
		if n != size {
			return written, io.ErrShortWrite
		}
		data = data[size:]
	}
	return written, nil
}

func handle(conn net.Conn, archivePath string) error {
	defer conn.Close()
	length, err := readLength(conn)
	if err != nil {
		return err
	}
	if length == 0 || length > maxChunk {
		return errors.New("invalid entry path length")
	}
	name := make([]byte, length)
	if _, err := io.ReadFull(conn, name); err != nil {
		return err
	}
	archive, err := os.Open(archivePath)
	if err != nil {
		return err
	}
	defer archive.Close()
	n, err := gozarlib.ExtractSingleFileToWriter(archive, string(name), frameWriter{conn}, nil)
	if err != nil {
		return err
	}
	if err = writeLength(conn, 0); err != nil {
		return err
	}
	log.Printf("%s: %q (%d bytes)", conn.RemoteAddr(), name, n)
	return nil
}

func serve(address, archive string) error {
	listener, err := net.Listen("tcp", address)
	if err != nil {
		return err
	}
	defer listener.Close()
	log.Printf("Listening on %s", listener.Addr())
	for {
		conn, err := listener.Accept()
		if err != nil {
			return err
		}
		if err = handle(conn, archive); err != nil {
			log.Printf("request failed: %v", err)
		}
	}
}

func receive(address, entry, path string) (result error) {
	if len(entry) == 0 || len(entry) > maxChunk {
		return errors.New("invalid entry path length")
	}
	output, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0666)
	if err != nil {
		return err
	}
	defer func() {
		if err := output.Close(); result == nil && err != nil {
			result = err
		}
		if result != nil {
			if err := os.Remove(path); err != nil {
				result = errors.Join(result, err)
			}
		}
	}()
	conn, err := net.Dial("tcp", address)
	if err != nil {
		return err
	}
	defer conn.Close()
	if err = writeLength(conn, uint32(len(entry))); err != nil {
		return err
	}
	if _, err = io.WriteString(conn, entry); err != nil {
		return err
	}
	for {
		length, err := readLength(conn)
		if err != nil {
			return err
		}
		if length == 0 {
			return nil
		}
		if length > maxChunk {
			return errors.New("invalid data frame length")
		}
		if _, err = io.CopyN(output, conn, int64(length)); err != nil {
			return err
		}
	}
}

func main() {
	if len(os.Args) < 2 {
		log.Fatal("usage: tcp_streaming server ADDRESS ARCHIVE | client ADDRESS ENTRY OUTPUT")
	}
	var err error
	switch {
	case os.Args[1] == "server" && len(os.Args) == 4:
		err = serve(os.Args[2], os.Args[3])
	case os.Args[1] == "client" && len(os.Args) == 5:
		err = receive(os.Args[2], os.Args[3], os.Args[4])
	default:
		err = fmt.Errorf("usage: tcp_streaming server ADDRESS ARCHIVE | client ADDRESS ENTRY OUTPUT")
	}
	if err != nil {
		log.Fatal(err)
	}
}
