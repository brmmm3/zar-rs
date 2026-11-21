use std::env;
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;

use zarlib::operations::{
    CollisionPolicy, ExistingPolicy, ExtractionOptions, extract_archive_with_progress,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let usage = "usage: streaming_tree_client ADDRESS archive|tree OUTPUT";
    let address = args.next().ok_or(usage)?;
    let mode = args.next().ok_or(usage)?;
    let output = args.next().ok_or(usage)?;
    if args.next().is_some() {
        return Err(usage.into());
    }

    let mut socket = TcpStream::connect(address)?;
    let mut size = [0; 8];
    socket.read_exact(&mut size)?;
    let size = u64::from_be_bytes(size);
    let mut input = socket.take(size);
    match mode.as_str() {
        "archive" => {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output)?;
            io::copy(&mut input, &mut file)?;
            file.flush()?;
        }
        "tree" => {
            let mut archive = tempfile::NamedTempFile::new()?;
            io::copy(&mut input, &mut archive)?;
            if input.limit() != 0 {
                return Err(
                    "server closed the connection before sending the complete archive".into(),
                );
            }
            archive.flush()?;
            extract_archive_with_progress(
                archive
                    .path()
                    .to_str()
                    .ok_or("invalid temporary archive path")?,
                Path::new(&output),
                ExtractionOptions {
                    dry_run: false,
                    to: None,
                    collision_policy: CollisionPolicy::Rename,
                    existing_policy: ExistingPolicy::Error,
                    restore_metadata: false,
                    restore_symlinks: true,
                    restore_devices: false,
                    strip_top_level: false,
                },
                None,
                None,
            )?;
        }
        _ => return Err(usage.into()),
    }
    if input.limit() != 0 {
        return Err("server closed the connection before sending the complete archive".into());
    }
    Ok(())
}
