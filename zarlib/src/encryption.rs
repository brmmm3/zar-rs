use std::collections::HashSet;
use std::io::{Cursor, Read};

use crate::FramingError;

const TAG_UNENCRYPTED: u8 = 0;
const TAG_SYMMETRIC: u8 = 1;
const TAG_PASSWORD: u8 = 2;
const TAG_RECIPIENTS: u8 = 3;
const TAG_PASSWORD_AND_RECIPIENTS: u8 = 4;
const TAG_ARGON2ID: u8 = 1;
const TAG_AES_256_GCM: u8 = 1;
const TAG_CHACHA20_POLY1305: u8 = 2;
const TAG_HPKE_X25519_HKDF_SHA256_AES128GCM: u8 = 1;
const TAG_EXTENSIONS: u8 = 0x80;
const MAX_RECIPIENTS: usize = 1024;
const MIN_RECIPIENT_ID_SIZE: usize = 1;
const MAX_RECIPIENT_ID_SIZE: usize = 256;
const MAX_HPKE_ENC_SIZE: usize = 1024;
const MAX_WRAPPED_DEK_SIZE: usize = 4096;
pub const PASSWORD_SALT_SIZE: usize = 16;
pub const AEAD_NONCE_SIZE: usize = 12;
pub const DEK_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EncryptionScope {
    Header = 1,
    Data = 2,
    HeaderAndData = 3,
}

impl EncryptionScope {
    fn from_tag(tag: u8) -> Result<Self, FramingError> {
        match tag {
            1 => Ok(Self::Header),
            2 => Ok(Self::Data),
            3 => Ok(Self::HeaderAndData),
            _ => Err(FramingError::InvalidDescriptor),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentAead {
    Aes256Gcm = TAG_AES_256_GCM,
    ChaCha20Poly1305 = TAG_CHACHA20_POLY1305,
}

impl ContentAead {
    fn from_tag(tag: u8) -> Result<Self, FramingError> {
        match tag {
            TAG_AES_256_GCM => Ok(Self::Aes256Gcm),
            TAG_CHACHA20_POLY1305 => Ok(Self::ChaCha20Poly1305),
            _ => Err(FramingError::InvalidDescriptor),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HpkeSuite {
    X25519HkdfSha256Aes128Gcm = TAG_HPKE_X25519_HKDF_SHA256_AES128GCM,
}

impl HpkeSuite {
    fn from_tag(tag: u8) -> Result<Self, FramingError> {
        match tag {
            TAG_HPKE_X25519_HKDF_SHA256_AES128GCM => Ok(Self::X25519HkdfSha256Aes128Gcm),
            _ => Err(FramingError::InvalidDescriptor),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasswordKeyWrap {
    pub algorithm: ContentAead,
    pub salt: [u8; PASSWORD_SALT_SIZE],
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub wrap_nonce: [u8; AEAD_NONCE_SIZE],
    pub wrapped_dek: Vec<u8>,
}

impl PasswordKeyWrap {
    pub fn validate(&self) -> Result<(), FramingError> {
        if !(65_536..=1_048_576).contains(&self.memory_kib)
            || !(2..=10).contains(&self.iterations)
            || !(1..=16).contains(&self.parallelism)
            || !(DEK_SIZE + 16..=MAX_WRAPPED_DEK_SIZE).contains(&self.wrapped_dek.len())
        {
            return Err(FramingError::InvalidDescriptor);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipientKeyWrap {
    pub recipient_id: Vec<u8>,
    pub suite: HpkeSuite,
    pub enc: Vec<u8>,
    pub wrapped_dek: Vec<u8>,
}

impl RecipientKeyWrap {
    pub fn validate(&self) -> Result<(), FramingError> {
        if !(MIN_RECIPIENT_ID_SIZE..=MAX_RECIPIENT_ID_SIZE).contains(&self.recipient_id.len())
            || self.enc.is_empty()
            || self.enc.len() > MAX_HPKE_ENC_SIZE
            || !(DEK_SIZE + 16..=MAX_WRAPPED_DEK_SIZE).contains(&self.wrapped_dek.len())
        {
            return Err(FramingError::InvalidDescriptor);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorExtension {
    pub tag: u8,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncryptionDescriptor {
    Unencrypted,
    Symmetric {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; AEAD_NONCE_SIZE],
    },
    Password {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; AEAD_NONCE_SIZE],
        key_wrap: PasswordKeyWrap,
    },
    Recipients {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; AEAD_NONCE_SIZE],
        recipients: Vec<RecipientKeyWrap>,
    },
    PasswordAndRecipients {
        scope: EncryptionScope,
        algorithm: ContentAead,
        nonce: [u8; AEAD_NONCE_SIZE],
        password: PasswordKeyWrap,
        recipients: Vec<RecipientKeyWrap>,
    },
}

impl EncryptionDescriptor {
    pub fn is_header_encrypted(&self) -> bool {
        matches!(
            self,
            Self::Symmetric {
                scope: EncryptionScope::Header | EncryptionScope::HeaderAndData,
                ..
            } | Self::Password {
                scope: EncryptionScope::Header | EncryptionScope::HeaderAndData,
                ..
            } | Self::Recipients {
                scope: EncryptionScope::Header | EncryptionScope::HeaderAndData,
                ..
            } | Self::PasswordAndRecipients {
                scope: EncryptionScope::Header | EncryptionScope::HeaderAndData,
                ..
            }
        )
    }

    pub fn is_data_encrypted(&self) -> bool {
        matches!(
            self,
            Self::Symmetric {
                scope: EncryptionScope::Data | EncryptionScope::HeaderAndData,
                ..
            } | Self::Password {
                scope: EncryptionScope::Data | EncryptionScope::HeaderAndData,
                ..
            } | Self::Recipients {
                scope: EncryptionScope::Data | EncryptionScope::HeaderAndData,
                ..
            } | Self::PasswordAndRecipients {
                scope: EncryptionScope::Data | EncryptionScope::HeaderAndData,
                ..
            }
        )
    }

    pub fn encode(&self) -> Result<Vec<u8>, FramingError> {
        self.encode_with_extensions(&[])
    }

    pub fn encode_with_extensions(
        &self,
        extensions: &[DescriptorExtension],
    ) -> Result<Vec<u8>, FramingError> {
        let mut output = Vec::new();
        match self {
            Self::Unencrypted => output.push(TAG_UNENCRYPTED),
            Self::Symmetric {
                scope,
                algorithm,
                nonce,
            } => {
                output.extend_from_slice(&[TAG_SYMMETRIC, *scope as u8, *algorithm as u8]);
                output.extend_from_slice(nonce);
            }
            Self::Password {
                scope,
                algorithm,
                nonce,
                key_wrap,
            } => {
                output.extend_from_slice(&[TAG_PASSWORD, *scope as u8, *algorithm as u8]);
                output.extend_from_slice(nonce);
                encode_password_wrap(&mut output, key_wrap)?;
            }
            Self::Recipients {
                scope,
                algorithm,
                nonce,
                recipients,
            } => {
                output.extend_from_slice(&[TAG_RECIPIENTS, *scope as u8, *algorithm as u8]);
                output.extend_from_slice(nonce);
                encode_recipients(&mut output, recipients)?;
            }
            Self::PasswordAndRecipients {
                scope,
                algorithm,
                nonce,
                password,
                recipients,
            } => {
                output.extend_from_slice(&[
                    TAG_PASSWORD_AND_RECIPIENTS,
                    *scope as u8,
                    *algorithm as u8,
                ]);
                output.extend_from_slice(nonce);
                encode_password_wrap(&mut output, password)?;
                encode_recipients(&mut output, recipients)?;
            }
        }
        for extension in extensions {
            if extension.tag < TAG_EXTENSIONS {
                return Err(FramingError::InvalidDescriptor);
            }
            output.push(extension.tag);
            write_bytes(&mut output, &extension.value)?;
        }
        if output.len() > crate::MAX_ENCRYPTION_DESCRIPTOR_SIZE {
            return Err(FramingError::InvalidDescriptor);
        }
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<(Self, Vec<DescriptorExtension>), FramingError> {
        if bytes.is_empty() || bytes.len() > crate::MAX_ENCRYPTION_DESCRIPTOR_SIZE {
            return Err(FramingError::InvalidDescriptor);
        }
        let mut cursor = Cursor::new(bytes);
        let mode = read_u8(&mut cursor)?;
        let descriptor = match mode {
            TAG_UNENCRYPTED => Self::Unencrypted,
            TAG_SYMMETRIC | TAG_PASSWORD | TAG_RECIPIENTS | TAG_PASSWORD_AND_RECIPIENTS => {
                let scope = EncryptionScope::from_tag(read_u8(&mut cursor)?)?;
                let algorithm = ContentAead::from_tag(read_u8(&mut cursor)?)?;
                let nonce = read_array(&mut cursor)?;
                match mode {
                    TAG_SYMMETRIC => Self::Symmetric {
                        scope,
                        algorithm,
                        nonce,
                    },
                    TAG_PASSWORD => Self::Password {
                        scope,
                        algorithm,
                        nonce,
                        key_wrap: decode_password_wrap(&mut cursor)?,
                    },
                    TAG_RECIPIENTS => Self::Recipients {
                        scope,
                        algorithm,
                        nonce,
                        recipients: decode_recipients(&mut cursor)?,
                    },
                    _ => Self::PasswordAndRecipients {
                        scope,
                        algorithm,
                        nonce,
                        password: decode_password_wrap(&mut cursor)?,
                        recipients: decode_recipients(&mut cursor)?,
                    },
                }
            }
            _ => return Err(FramingError::InvalidDescriptor),
        };

        let mut extensions = Vec::new();
        while (cursor.position() as usize) < bytes.len() {
            let tag = read_u8(&mut cursor)?;
            if tag < TAG_EXTENSIONS {
                return Err(FramingError::InvalidDescriptor);
            }
            let value = read_bytes(&mut cursor, crate::MAX_ENCRYPTION_DESCRIPTOR_SIZE)?;
            extensions.push(DescriptorExtension { tag, value });
        }
        Ok((descriptor, extensions))
    }
}

fn encode_password_wrap(output: &mut Vec<u8>, wrap: &PasswordKeyWrap) -> Result<(), FramingError> {
    wrap.validate()?;
    output.extend_from_slice(&[TAG_ARGON2ID, wrap.algorithm as u8]);
    output.extend_from_slice(&wrap.salt);
    output.extend_from_slice(&wrap.memory_kib.to_le_bytes());
    output.extend_from_slice(&wrap.iterations.to_le_bytes());
    output.extend_from_slice(&wrap.parallelism.to_le_bytes());
    output.extend_from_slice(&wrap.wrap_nonce);
    write_bytes(output, &wrap.wrapped_dek)
}

fn decode_password_wrap(reader: &mut Cursor<&[u8]>) -> Result<PasswordKeyWrap, FramingError> {
    if read_u8(reader)? != TAG_ARGON2ID {
        return Err(FramingError::InvalidDescriptor);
    }
    let wrap = PasswordKeyWrap {
        algorithm: ContentAead::from_tag(read_u8(reader)?)?,
        salt: read_array(reader)?,
        memory_kib: read_u32(reader)?,
        iterations: read_u32(reader)?,
        parallelism: read_u32(reader)?,
        wrap_nonce: read_array(reader)?,
        wrapped_dek: read_bytes(reader, MAX_WRAPPED_DEK_SIZE)?,
    };
    wrap.validate()?;
    Ok(wrap)
}

fn encode_recipients(
    output: &mut Vec<u8>,
    recipients: &[RecipientKeyWrap],
) -> Result<(), FramingError> {
    if recipients.is_empty() || recipients.len() > MAX_RECIPIENTS {
        return Err(FramingError::InvalidDescriptor);
    }
    write_u32(output, recipients.len())?;
    let mut ids = HashSet::with_capacity(recipients.len());
    for recipient in recipients {
        recipient.validate()?;
        if !ids.insert(recipient.recipient_id.as_slice()) {
            return Err(FramingError::InvalidDescriptor);
        }
        write_bytes(output, &recipient.recipient_id)?;
        output.push(recipient.suite as u8);
        write_bytes(output, &recipient.enc)?;
        write_bytes(output, &recipient.wrapped_dek)?;
    }
    Ok(())
}

fn decode_recipients(reader: &mut Cursor<&[u8]>) -> Result<Vec<RecipientKeyWrap>, FramingError> {
    let count = read_u32(reader)? as usize;
    if count == 0 || count > MAX_RECIPIENTS {
        return Err(FramingError::InvalidDescriptor);
    }
    let mut recipients = Vec::with_capacity(count);
    let mut ids = HashSet::with_capacity(count);
    for _ in 0..count {
        let recipient_id = read_bytes(reader, MAX_RECIPIENT_ID_SIZE)?;
        let suite = HpkeSuite::from_tag(read_u8(reader)?)?;
        let enc = read_bytes(reader, MAX_HPKE_ENC_SIZE)?;
        let wrapped_dek = read_bytes(reader, MAX_WRAPPED_DEK_SIZE)?;
        let recipient = RecipientKeyWrap {
            recipient_id,
            suite,
            enc,
            wrapped_dek,
        };
        recipient.validate()?;
        if !ids.insert(recipient.recipient_id.clone()) {
            return Err(FramingError::InvalidDescriptor);
        }
        recipients.push(recipient);
    }
    Ok(recipients)
}

fn write_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), FramingError> {
    let length = u32::try_from(bytes.len()).map_err(|_| FramingError::InvalidDescriptor)?;
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(bytes);
    Ok(())
}

fn write_u32(output: &mut Vec<u8>, value: usize) -> Result<(), FramingError> {
    let value = u32::try_from(value).map_err(|_| FramingError::InvalidDescriptor)?;
    output.extend_from_slice(&value.to_le_bytes());
    Ok(())
}

fn read_u8(reader: &mut Cursor<&[u8]>) -> Result<u8, FramingError> {
    let mut value = [0];
    reader.read_exact(&mut value)?;
    Ok(value[0])
}

fn read_u32(reader: &mut Cursor<&[u8]>) -> Result<u32, FramingError> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_array<const N: usize>(reader: &mut Cursor<&[u8]>) -> Result<[u8; N], FramingError> {
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_bytes(reader: &mut Cursor<&[u8]>, max_len: usize) -> Result<Vec<u8>, FramingError> {
    let length = read_u32(reader)? as usize;
    if length > max_len {
        return Err(FramingError::InvalidDescriptor);
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password_wrap() -> PasswordKeyWrap {
        PasswordKeyWrap {
            algorithm: ContentAead::Aes256Gcm,
            salt: [1; PASSWORD_SALT_SIZE],
            memory_kib: 65_536,
            iterations: 3,
            parallelism: 1,
            wrap_nonce: [2; AEAD_NONCE_SIZE],
            wrapped_dek: vec![3; DEK_SIZE + 16],
        }
    }

    fn recipient(id: &[u8]) -> RecipientKeyWrap {
        RecipientKeyWrap {
            recipient_id: id.to_vec(),
            suite: HpkeSuite::X25519HkdfSha256Aes128Gcm,
            enc: vec![4; 32],
            wrapped_dek: vec![5; DEK_SIZE + 16],
        }
    }

    #[test]
    fn unencrypted_descriptor_has_stable_wire_tag() {
        let bytes = EncryptionDescriptor::Unencrypted.encode().unwrap();
        assert_eq!(bytes, [0]);
        assert_eq!(
            EncryptionDescriptor::decode(&bytes).unwrap(),
            (EncryptionDescriptor::Unencrypted, vec![])
        );
    }

    #[test]
    fn password_and_recipients_round_trip() {
        let descriptor = EncryptionDescriptor::PasswordAndRecipients {
            scope: EncryptionScope::HeaderAndData,
            algorithm: ContentAead::ChaCha20Poly1305,
            nonce: [6; AEAD_NONCE_SIZE],
            password: password_wrap(),
            recipients: vec![recipient(b"key-a"), recipient(b"key-b")],
        };
        let encoded = descriptor.encode().unwrap();
        let (decoded, extensions) = EncryptionDescriptor::decode(&encoded).unwrap();
        assert_eq!(decoded, descriptor);
        assert!(extensions.is_empty());
    }

    #[test]
    fn descriptor_rejects_bad_parameters_duplicates_and_trailing_mandatory_data() {
        let mut wrap = password_wrap();
        wrap.memory_kib = 1024;
        assert_eq!(wrap.validate(), Err(FramingError::InvalidDescriptor));

        let descriptor = EncryptionDescriptor::Recipients {
            scope: EncryptionScope::Data,
            algorithm: ContentAead::Aes256Gcm,
            nonce: [0; AEAD_NONCE_SIZE],
            recipients: vec![recipient(b"same"), recipient(b"same")],
        };
        assert_eq!(descriptor.encode(), Err(FramingError::InvalidDescriptor));

        let mut encoded = EncryptionDescriptor::Unencrypted.encode().unwrap();
        encoded.push(1);
        assert_eq!(
            EncryptionDescriptor::decode(&encoded),
            Err(FramingError::InvalidDescriptor)
        );
    }

    #[test]
    fn descriptor_skips_optional_tlvs_and_rejects_unknown_mandatory_tags() {
        let descriptor = EncryptionDescriptor::Symmetric {
            scope: EncryptionScope::Header,
            algorithm: ContentAead::Aes256Gcm,
            nonce: [7; AEAD_NONCE_SIZE],
        };
        let encoded = descriptor
            .encode_with_extensions(&[DescriptorExtension {
                tag: 0x80,
                value: vec![8, 9],
            }])
            .unwrap();
        let (decoded, extensions) = EncryptionDescriptor::decode(&encoded).unwrap();
        assert_eq!(decoded, descriptor);
        assert_eq!(
            extensions,
            vec![DescriptorExtension {
                tag: 0x80,
                value: vec![8, 9]
            }]
        );
        assert_eq!(
            EncryptionDescriptor::decode(&[0x7f]),
            Err(FramingError::InvalidDescriptor)
        );
    }
}
