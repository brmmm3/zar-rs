use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let address = args.next().ok_or("usage: streaming_server ADDRESS FILE")?;
    let path = args.next().ok_or("usage: streaming_server ADDRESS FILE")?;
    if args.next().is_some() {
        return Err("usage: streaming_server ADDRESS FILE".into());
    }

    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err("input must be a regular file".into());
    }
    let size = metadata.len();
    let listener = TcpListener::bind(address.to_str().ok_or("address must be valid UTF-8")?)?;
    eprintln!("Listening on {}", listener.local_addr()?);

    let (mut socket, peer) = listener.accept()?;
    eprintln!("Sending {size} bytes to {peer}");
    socket.write_all(&size.to_be_bytes())?;
    let sent = io::copy(&mut file.take(size), &mut socket)?;
    if sent != size {
        return Err("input was truncated during transfer".into());
    }
    socket.flush()?;
    socket.shutdown(Shutdown::Write)?;
    Ok(())
}
