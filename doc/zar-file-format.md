# ZAR file architecture

A ZAR archive is a container header followed by one or more physically
contiguous commits. The container points to the first commit's header frame.
Each commit stores its data blocks before its compressed logical header, and
the offset fields link the commits in order.

## Physical layout

The diagram shows the byte order for a two-commit archive. A one-commit
archive ends after its first `this_header_offset`. Repeated commits use the
same structure.

```plantuml
@startuml
title Physical layout of a ZAR archive
top to bottom direction
skinparam shadowing false
skinparam defaultTextAlignment center
skinparam rectangle {
  RoundCorner 6
}

rectangle "Container header (12 bytes)" as container #DCEEFF {
  rectangle "Magic: ZAR0\n4 bytes" as magic
  rectangle "first_header_offset\nu64, little-endian" as first_offset
}

rectangle "Commit 1" as commit1 #EAF7E8 {
  rectangle "next_header_offset\nu64; 0 if no next commit" as next1
  rectangle "Data section\n[data_section_size bytes]" as data1 #FFF2CC {
    rectangle "Block frame 0" as block1
    rectangle "Block frame 1" as block2
    rectangle "..." as block_ellipsis
  }
  rectangle "Header frame\n[header_frame_size bytes]" as header1 #FCE4D6
  rectangle "this_header_offset\nu64; points to this header frame" as this1
}

rectangle "Commit 2 and later commits (same layout)" as commit2 #EAF7E8 {
  rectangle "next_header_offset\nu64; 0 for final commit" as next2
  rectangle "Data section\n[block frames]" as data2 #FFF2CC
  rectangle "Header frame\n[header_frame_size bytes]" as header2 #FCE4D6
  rectangle "this_header_offset\nu64; final value is at end of file" as this2
}

container -down-> header1 : first_header_offset
header1 -down-> header2 : next_header_offset from Commit 1
header2 -down-> this2 : final seek-to-last trailer

note right of next1
This link field is physically before the commit data section.
Its value is the absolute byte offset of the next commit's
header frame, or zero when this is the last commit.
end note

note right of this1
The trailing offset repeats the absolute position of this
commit's header frame. For the last commit it is at EOF - 8,
so readers can locate the last header without scanning the chain.
end note
@enduml
```

## Header frame

Each header frame begins at the offset named by the container or a commit
link. Its fixed fields are followed by the encryption descriptor and the
compressed (and, when configured, encrypted) logical-header payload.

```plantuml
@startuml
title Header frame layout
left to right direction
skinparam shadowing false
skinparam defaultTextAlignment center

rectangle "Commit header\n22 bytes" as commit_header #DCEEFF
rectangle "compressed_header_len\nu32" as compressed_len
rectangle "header_checksum\nxxHash64, u64" as header_checksum
rectangle "encryption_descriptor_len\nu32" as descriptor_len
rectangle "Encryption descriptor\nvariable length" as descriptor #EAF7E8
rectangle "Header payload\ncompressed; optionally encrypted" as payload #FCE4D6

commit_header --> compressed_len
compressed_len --> header_checksum
header_checksum --> descriptor_len
descriptor_len --> descriptor
descriptor --> payload

note bottom of commit_header
ZCMT magic (4), framing version (1), flags (1),
commit ID (4), data_section_size (u64),
header_frame_size (u32). Integer fields are little-endian.
end note

note bottom of payload
Decompressed payload is the project logical header:
ZHDR magic, versioned TOC changes, local block references,
optional block-size and archive metadata, and optional entry metadata.
end note
@enduml
```

## Data block frame

The data section is a sequence of block frames. A file entry in the logical
header refers to one or more block indices local to that commit. Blocks may
be compressed or stored uncompressed; encrypted blocks carry a nonce.

```plantuml
@startuml
title Data block frame layout
left to right direction
skinparam shadowing false
skinparam defaultTextAlignment center

rectangle "payload_size\nu32" as payload_size
rectangle "flags\nu16" as flags
rectangle "reserved\nu16; must be zero" as reserved
rectangle "block_index\nu32" as block_index
rectangle "nonce\n12 bytes, only when flag is set" as nonce #EAF7E8
rectangle "payload_checksum\nxxHash64, u64" as checksum
rectangle "payload\nvariable length" as payload #FCE4D6

payload_size --> flags
flags --> reserved
reserved --> block_index
block_index --> nonce
nonce --> checksum
checksum --> payload

note bottom of flags
Flags identify encrypted, nonce-present, and stored-uncompressed
payloads. Integer fields are little-endian.
end note

note bottom of payload
File bytes are split into blocks. A block is compressed when that
reduces its size (unless the file type skips compression), then
encrypted when the commit descriptor enables data encryption.
end note
@enduml
```

## Reading the structure

- `first_header_offset` is an absolute offset to the first header frame. A
  reader locates that frame, derives the commit data start by subtracting
  `data_section_size`, then reads the `next_header_offset` immediately before
  that data section to continue the chain.
- `this_header_offset` at the end of each commit points back to its header
  frame. The final value makes it possible to find the last commit from EOF.
- `data_section_size` covers only that commit's block frames. The 22-byte
  commit header and `header_frame_size` are part of the header frame.
- The header payload describes TOC operations, not a fully materialized final
  TOC. Applying commit changes in chain order reconstructs the archive's
  effective contents.
- Block checksums cover stored payload bytes; the header checksum covers the
  header payload bytes. Encrypted payloads also require successful AEAD
  authentication to read.

The frame definitions are implemented in [`zarlib/src/lib.rs`](../zarlib/src/lib.rs)
and [`zarlib/src/frames.rs`](../zarlib/src/frames.rs); the logical-header
encoding is in [`zarlib/src/header.rs`](../zarlib/src/header.rs).
