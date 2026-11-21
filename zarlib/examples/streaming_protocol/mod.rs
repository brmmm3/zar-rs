use std::io::{self, Read, Write};

pub const MAGIC_STREAMING: &[u8; 8] = b"ZARREQ03";
pub const MAX_TEXT: usize = 64 * 1024;
pub const MAX_PATTERNS: usize = 64;
pub const MAX_FILES: usize = 1_000_000;

pub fn write_u32(writer: &mut impl Write, value: u32) -> io::Result<()> {
    writer.write_all(&value.to_be_bytes())
}

pub fn read_u32(reader: &mut impl Read) -> io::Result<u32> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_be_bytes(bytes))
}

pub fn write_text(writer: &mut impl Write, value: &str) -> io::Result<()> {
    if value.len() > MAX_TEXT {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "text too long"));
    }
    write_u32(writer, value.len() as u32)?;
    writer.write_all(value.as_bytes())
}

pub fn read_text(reader: &mut impl Read) -> io::Result<String> {
    let len = read_u32(reader)? as usize;
    if len > MAX_TEXT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "text too long"));
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
