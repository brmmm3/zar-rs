use std::env;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::TcpStream;

use zarlib::operations::{
    EncodingOptions, PasswordSet, StreamInput, create_archive_from_readers,
    extract_single_file_to_writer,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let usage = "usage: streaming_client ADDRESS ARCHIVE ENTRY";
    let address = args.next().ok_or(usage)?;
    let archive_path = args.next().ok_or(usage)?;
    let entry = args.next().ok_or(usage)?;
    if args.next().is_some() {
        return Err(usage.into());
    }
    let address = address.to_str().ok_or("address must be valid UTF-8")?;
    let entry = entry.to_str().ok_or("archive entry must be valid UTF-8")?;

    let mut socket = TcpStream::connect(address)?;
    let mut size = [0_u8; 8];
    socket.read_exact(&mut size)?;
    let size = u64::from_be_bytes(size);
    let mut input = socket.take(size);
    let mut archive = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(archive_path)?;
    create_archive_from_readers(
        &mut archive,
        &mut [StreamInput {
            path: entry,
            reader: &mut input,
        }],
        EncodingOptions::default(),
        None,
    )?;
    if input.limit() != 0 {
        return Err("server closed the connection before sending the complete file".into());
    }
    archive.flush()?;

    let stdout = io::stdout();
    let mut output = stdout.lock();
    extract_single_file_to_writer(
        &mut archive,
        entry,
        &mut output,
        &PasswordSet::default(),
        None,
    )?;
    output.flush()?;
    Ok(())
}
