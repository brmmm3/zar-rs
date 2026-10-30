//! Settings from `zcopy.json` and command-line arguments (CLI wins).

use std::path::Path;

use serde::Deserialize;

use crate::net::{DEFAULT_PORT, Res};

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub password: Option<String>,
    /// `aes-256-gcm` or `chacha20-poly1305`.
    pub aead: Option<String>,
    pub level: Option<i32>,
    /// Machine names or IP addresses that may connect to the server.
    pub allow: Option<Vec<String>>,
    pub port: Option<u16>,
    pub bind: Option<String>,
    /// Approximate size of one transfer batch in MiB.
    pub batch_mib: Option<u64>,
    /// Client: also send permissions and ownership. Server: apply them.
    pub metadata: Option<bool>,
}

impl Settings {
    pub fn load(path: &Path, required: bool) -> Res<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|error| format!("{}: {error}", path.display()).into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => {
                Ok(Self::default())
            }
            Err(error) => Err(format!("{}: {error}", path.display()).into()),
        }
    }

    /// Fills unset fields from `other`.
    pub fn or(self, other: Settings) -> Settings {
        Settings {
            password: self.password.or(other.password),
            aead: self.aead.or(other.aead),
            level: self.level.or(other.level),
            allow: self.allow.or(other.allow),
            port: self.port.or(other.port),
            bind: self.bind.or(other.bind),
            batch_mib: self.batch_mib.or(other.batch_mib),
            metadata: self.metadata.or(other.metadata),
        }
    }

    pub fn port(&self) -> u16 {
        self.port.unwrap_or(DEFAULT_PORT)
    }
}

pub fn parse_aead(name: &str) -> Res<zarlib::ContentAead> {
    match name {
        "aes-256-gcm" => Ok(zarlib::ContentAead::Aes256Gcm),
        "chacha20-poly1305" => Ok(zarlib::ContentAead::ChaCha20Poly1305),
        _ => Err(format!("unknown aead '{name}' (aes-256-gcm or chacha20-poly1305)").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_overrides_file_and_unknown_keys_fail() {
        let file: Settings =
            serde_json::from_str(r#"{"level": 9, "password": "x", "allow": ["a"]}"#).unwrap();
        let cli = Settings {
            level: Some(1),
            ..Settings::default()
        };
        let merged = cli.or(file);
        assert_eq!(merged.level, Some(1));
        assert_eq!(merged.password.as_deref(), Some("x"));
        assert!(serde_json::from_str::<Settings>(r#"{"bogus": 1}"#).is_err());
    }
}
