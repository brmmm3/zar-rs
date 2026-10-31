use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::ChaCha20Poly1305;
use zeroize::{Zeroize, Zeroizing};

use crate::{ContentAead, FramingError, PasswordKeyWrap};

pub const DEK_SIZE: usize = 32;
pub const AEAD_NONCE_SIZE: usize = 12;

/// Generates a cryptographically random 256-bit data-encryption key.
pub fn generate_dek() -> Result<[u8; DEK_SIZE], FramingError> {
    let mut key = [0; DEK_SIZE];
    if let Err(error) = getrandom::fill(&mut key) {
        key.zeroize();
        return Err(FramingError::Io(error.to_string()));
    }
    Ok(key)
}

/// Generates a cryptographically random nonce for content encryption.
pub fn generate_nonce() -> Result<[u8; AEAD_NONCE_SIZE], FramingError> {
    let mut nonce = [0; AEAD_NONCE_SIZE];
    getrandom::fill(&mut nonce).map_err(|error| FramingError::Io(error.to_string()))?;
    Ok(nonce)
}

/// Encrypts one independently-nonced compressed content payload.
pub fn encrypt_content(
    algorithm: ContentAead,
    key: &[u8; DEK_SIZE],
    nonce: &[u8; AEAD_NONCE_SIZE],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, FramingError> {
    let payload = Payload {
        msg: plaintext,
        aad,
    };
    let nonce = Nonce::try_from(nonce.as_slice()).expect("AEAD nonce has the expected length");
    match algorithm {
        ContentAead::Aes256Gcm => Aes256Gcm::new_from_slice(key)
            .expect("AES-256-GCM accepts a 32-byte key")
            .encrypt(&nonce, payload),
        ContentAead::ChaCha20Poly1305 => ChaCha20Poly1305::new_from_slice(key)
            .expect("ChaCha20-Poly1305 accepts a 32-byte key")
            .encrypt(&nonce, payload),
    }
    .map_err(|_| FramingError::AuthenticationFailed)
}

/// Derives a wrapping key using Argon2id v1.3 and the supplied bounded parameters.
pub fn derive_password_key(
    password: &[u8],
    salt: &[u8; 16],
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<[u8; DEK_SIZE], FramingError> {
    if !(65_536..=1_048_576).contains(&memory_kib)
        || !(2..=10).contains(&iterations)
        || !(1..=16).contains(&parallelism)
    {
        return Err(FramingError::InvalidDescriptor);
    }
    let params = Params::new(memory_kib, iterations, parallelism, Some(DEK_SIZE))
        .map_err(|_| FramingError::InvalidDescriptor)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0; DEK_SIZE];
    if argon2.hash_password_into(password, salt, &mut key).is_err() {
        key.zeroize();
        return Err(FramingError::InvalidDescriptor);
    }
    Ok(key)
}

/// Authenticates and decrypts one content payload.
pub fn decrypt_content(
    algorithm: ContentAead,
    key: &[u8; DEK_SIZE],
    nonce: &[u8; AEAD_NONCE_SIZE],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, FramingError> {
    let payload = Payload {
        msg: ciphertext,
        aad,
    };
    let nonce = Nonce::try_from(nonce.as_slice()).expect("AEAD nonce has the expected length");
    match algorithm {
        ContentAead::Aes256Gcm => Aes256Gcm::new_from_slice(key)
            .expect("AES-256-GCM accepts a 32-byte key")
            .decrypt(&nonce, payload),
        ContentAead::ChaCha20Poly1305 => ChaCha20Poly1305::new_from_slice(key)
            .expect("ChaCha20-Poly1305 accepts a 32-byte key")
            .decrypt(&nonce, payload),
    }
    .map_err(|_| FramingError::AuthenticationFailed)
}

/// Wraps a per-commit DEK with a password-derived key and fresh random salt/nonce.
pub fn wrap_dek_with_password(
    password: &[u8],
    dek: &[u8; DEK_SIZE],
    algorithm: ContentAead,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
) -> Result<PasswordKeyWrap, FramingError> {
    let mut wrap = PasswordKeyWrap {
        algorithm,
        salt: random_array()?,
        memory_kib,
        iterations,
        parallelism,
        wrap_nonce: random_array()?,
        wrapped_dek: Vec::new(),
    };
    let mut key = derive_password_key(password, &wrap.salt, memory_kib, iterations, parallelism)?;
    let encrypted = encrypt_content(algorithm, &key, &wrap.wrap_nonce, b"ZAR0 DEK wrap", dek);
    key.zeroize();
    wrap.wrapped_dek = encrypted?;
    wrap.validate()?;
    Ok(wrap)
}

/// Recovers and authenticates a password-wrapped DEK.
///
/// Prefer [`unwrap_dek_with_password_zeroizing`] when the key can remain in a zeroizing buffer.
pub fn unwrap_dek_with_password(
    password: &[u8],
    wrap: &PasswordKeyWrap,
) -> Result<[u8; DEK_SIZE], FramingError> {
    unwrap_dek_with_password_zeroizing(password, wrap).map(|key| *key)
}

/// Recovers a password-wrapped DEK in a buffer that is zeroized on drop.
pub fn unwrap_dek_with_password_zeroizing(
    password: &[u8],
    wrap: &PasswordKeyWrap,
) -> Result<Zeroizing<[u8; DEK_SIZE]>, FramingError> {
    wrap.validate()?;
    let mut key = derive_password_key(
        password,
        &wrap.salt,
        wrap.memory_kib,
        wrap.iterations,
        wrap.parallelism,
    )?;
    let plaintext = decrypt_content(
        wrap.algorithm,
        &key,
        &wrap.wrap_nonce,
        b"ZAR0 DEK wrap",
        &wrap.wrapped_dek,
    );
    key.zeroize();
    let mut plaintext = plaintext?;
    if plaintext.len() != DEK_SIZE {
        plaintext.zeroize();
        return Err(FramingError::AuthenticationFailed);
    }
    let mut dek = Zeroizing::new([0; DEK_SIZE]);
    dek.copy_from_slice(&plaintext);
    plaintext.zeroize();
    Ok(dek)
}

fn random_array<const N: usize>() -> Result<[u8; N], FramingError> {
    let mut value = [0; N];
    getrandom::fill(&mut value).map_err(|error| FramingError::Io(error.to_string()))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_aead_round_trips_and_authenticates_data_and_aad() {
        let key = [7; DEK_SIZE];
        let nonce = [9; AEAD_NONCE_SIZE];
        for algorithm in [ContentAead::Aes256Gcm, ContentAead::ChaCha20Poly1305] {
            let encrypted =
                encrypt_content(algorithm, &key, &nonce, b"prefix AAD", b"secret").unwrap();
            assert_eq!(
                decrypt_content(algorithm, &key, &nonce, b"prefix AAD", &encrypted).unwrap(),
                b"secret"
            );
            assert_eq!(
                decrypt_content(algorithm, &key, &nonce, b"altered AAD", &encrypted),
                Err(FramingError::AuthenticationFailed)
            );
            let mut changed = encrypted;
            changed[0] ^= 1;
            assert_eq!(
                decrypt_content(algorithm, &key, &nonce, b"prefix AAD", &changed),
                Err(FramingError::AuthenticationFailed)
            );
        }
    }

    #[test]
    fn password_wrapping_round_trips_and_rejects_wrong_password() {
        let dek = [3; DEK_SIZE];
        let wrapped = wrap_dek_with_password(
            b"correct horse battery staple",
            &dek,
            ContentAead::ChaCha20Poly1305,
            65_536,
            2,
            1,
        )
        .unwrap();
        assert_eq!(
            unwrap_dek_with_password(b"correct horse battery staple", &wrapped)
                .unwrap()
                .as_ref(),
            &dek
        );
        assert_eq!(
            unwrap_dek_with_password(b"wrong password", &wrapped),
            Err(FramingError::AuthenticationFailed)
        );
    }

    #[test]
    fn password_key_derivation_rejects_out_of_range_parameters() {
        assert_eq!(
            derive_password_key(b"password", &[0; 16], 1024, 3, 1),
            Err(FramingError::InvalidDescriptor)
        );
        assert_eq!(
            derive_password_key(b"password", &[0; 16], 65_536, 1, 1),
            Err(FramingError::InvalidDescriptor)
        );
    }

    #[test]
    fn random_material_has_expected_lengths() {
        assert_eq!(generate_dek().unwrap().len(), DEK_SIZE);
        assert_eq!(generate_nonce().unwrap().len(), AEAD_NONCE_SIZE);
    }
}
