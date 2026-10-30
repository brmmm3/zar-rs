//! Framed, optionally encrypted control channel shared by client and server.
//!
//! Every frame is a big-endian `u32` length followed by the payload. With a
//! password, payloads are AES-256-GCM encrypted with a key derived from the
//! password and a per-connection random salt; nonces are direction plus a
//! frame counter, so frames cannot be replayed, reordered or reflected.
//! Bulk archive bytes are sent raw between frames because the archives are
//! already encrypted by `zarlib` when a password is configured.

use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use zarlib::{ContentAead, decrypt_content, derive_password_key, encrypt_content};

pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub const MAGIC: [u8; 4] = *b"ZCP1";
pub const DEFAULT_PORT: u16 = 7878;
const MAX_FRAME: usize = 1 << 30;
const MAX_UNCOMPRESSED: u64 = 8 << 30;
const AAD: &[u8] = b"zcopy-frame-v1";
pub const DIR_CLIENT_TO_SERVER: u8 = 0;
pub const DIR_SERVER_TO_CLIENT: u8 = 1;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Msg {
    /// Proves knowledge of the password (decryptable first frame).
    Auth,
    /// Server settings followed by a compressed manifest frame.
    Manifest {
        entries: u64,
        aead: String,
        level: i32,
    },
    /// Followed by a compressed item-list frame and `bytes` raw archive bytes.
    Batch {
        bytes: u64,
        files: u64,
    },
    Ack {
        ok: bool,
        message: String,
        files: u64,
    },
    Done,
}

pub fn derive_key(password: &[u8], salt: &[u8; 16]) -> Res<[u8; 32]> {
    derive_password_key(password, salt, 65_536, 2, 1).map_err(|error| error.to_string().into())
}

fn nonce(direction: u8, counter: u64) -> [u8; 12] {
    let mut value = [0; 12];
    value[0] = direction;
    value[4..].copy_from_slice(&counter.to_be_bytes());
    value
}

pub struct FrameWriter<W: Write> {
    inner: W,
    key: Option<[u8; 32]>,
    direction: u8,
    counter: u64,
}

impl<W: Write> FrameWriter<W> {
    pub fn new(inner: W, key: Option<[u8; 32]>, direction: u8) -> Self {
        Self {
            inner,
            key,
            direction,
            counter: 0,
        }
    }

    pub fn send(&mut self, payload: &[u8]) -> Res<()> {
        let body = match &self.key {
            Some(key) => encrypt_content(
                ContentAead::Aes256Gcm,
                key,
                &nonce(self.direction, self.counter),
                AAD,
                payload,
            )
            .map_err(|error| error.to_string())?,
            None => payload.to_vec(),
        };
        self.counter += 1;
        if body.len() > MAX_FRAME {
            return Err("frame too large".into());
        }
        self.inner.write_all(&(body.len() as u32).to_be_bytes())?;
        self.inner.write_all(&body)?;
        self.inner.flush()?;
        Ok(())
    }

    pub fn send_msg(&mut self, message: &Msg) -> Res<()> {
        self.send(&serde_json::to_vec(message)?)
    }

    /// Sends zstd-compressed JSON of `value` as one frame.
    pub fn send_compressed<T: Serialize>(&mut self, value: &T, level: i32) -> Res<()> {
        let json = serde_json::to_vec(value)?;
        self.send(&zstd::encode_all(&json[..], level)?)
    }

    pub fn send_raw(&mut self, source: &mut impl Read, length: u64) -> Res<()> {
        let copied = io::copy(&mut source.take(length), &mut self.inner)?;
        if copied != length {
            return Err("file shrank while sending".into());
        }
        self.inner.flush()?;
        Ok(())
    }
}

pub struct FrameReader<R: Read> {
    inner: R,
    key: Option<[u8; 32]>,
    direction: u8,
    counter: u64,
}

impl<R: Read> FrameReader<R> {
    pub fn new(inner: R, key: Option<[u8; 32]>, direction: u8) -> Self {
        Self {
            inner,
            key,
            direction,
            counter: 0,
        }
    }

    pub fn recv(&mut self) -> Res<Vec<u8>> {
        let mut length = [0; 4];
        self.inner.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;
        if length > MAX_FRAME {
            return Err("frame too large".into());
        }
        let mut body = vec![0; length];
        self.inner.read_exact(&mut body)?;
        let counter = self.counter;
        self.counter += 1;
        match &self.key {
            Some(key) => decrypt_content(
                ContentAead::Aes256Gcm,
                key,
                &nonce(self.direction, counter),
                AAD,
                &body,
            )
            .map_err(|_| "authentication failed (wrong password or tampered data)".into()),
            None => Ok(body),
        }
    }

    pub fn recv_msg(&mut self) -> Res<Msg> {
        Ok(serde_json::from_slice(&self.recv()?)?)
    }

    pub fn recv_compressed<T: for<'de> Deserialize<'de>>(&mut self) -> Res<T> {
        let packed = self.recv()?;
        let decoder = zstd::stream::Decoder::new(&packed[..])?;
        let mut json = Vec::new();
        decoder.take(MAX_UNCOMPRESSED).read_to_end(&mut json)?;
        Ok(serde_json::from_slice(&json)?)
    }

    pub fn recv_raw(&mut self, destination: &mut impl Write, length: u64) -> Res<()> {
        let copied = io::copy(&mut (&mut self.inner).take(length), destination)?;
        if copied != length {
            return Err("connection closed during transfer".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_frames_round_trip_and_reject_wrong_direction() {
        let key = Some([5; 32]);
        let mut wire = Vec::new();
        {
            let mut writer = FrameWriter::new(&mut wire, key, DIR_CLIENT_TO_SERVER);
            writer.send_msg(&Msg::Auth).unwrap();
            writer.send(b"hello").unwrap();
        }
        assert!(!wire.windows(5).any(|window| window == b"hello"));
        let mut reader = FrameReader::new(&wire[..], key, DIR_CLIENT_TO_SERVER);
        assert!(matches!(reader.recv_msg().unwrap(), Msg::Auth));
        assert_eq!(reader.recv().unwrap(), b"hello");

        let mut reflected = FrameReader::new(&wire[..], key, DIR_SERVER_TO_CLIENT);
        assert!(reflected.recv().is_err());
        let mut wrong_key = FrameReader::new(&wire[..], Some([6; 32]), DIR_CLIENT_TO_SERVER);
        assert!(wrong_key.recv().is_err());
    }

    #[test]
    fn compressed_frames_round_trip() {
        let mut wire = Vec::new();
        FrameWriter::new(&mut wire, None, 0)
            .send_compressed(&vec![("a".to_owned(), 1u64)], 3)
            .unwrap();
        let mut reader = FrameReader::new(&wire[..], None, 0);
        let items: Vec<(String, u64)> = reader.recv_compressed().unwrap();
        assert_eq!(items, vec![("a".to_owned(), 1)]);
    }
}
