# ISO import fixtures

The compressed ISO images were generated with `genisoimage -no-pad` from a
directory containing `nested/hello.txt` (`ISO content`), `nested/café.txt`
(`unicode`), `empty.txt`, and an empty `emptydir` directory. The primary image
uses no extensions, the Joliet image uses `-J`, and the Rock Ridge image uses
`-R -J`. The link and special images use `-R` with, respectively, a symbolic
link to `nested/hello.txt` and a named pipe. Input encoding was set with
`-input-charset utf-8`; each image was compressed with `gzip -n`.
The large fixture was generated with `-R` from a 300 KiB zero-filled
`large.bin` to exercise streaming and cancellation.
