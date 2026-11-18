//! Import of Portfolio Performance files (XML, binary/protobuf, encrypted,
//! ZIP compressed).

pub mod export;
pub mod pdf;
mod proto;
mod upgrade;
mod xml;

use std::io::{Cursor, Read};

use pp_core::model::Client;
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("Die Datei ist verschlüsselt, bitte Passwort angeben")]
    PasswordRequired,
    #[error("Falsches Passwort oder beschädigte Datei")]
    WrongPassword,
    #[error("Dateiformat nicht unterstützt: {0}")]
    Format(String),
    #[error("Dateiversion {0} ist neuer als unterstützt ({1})")]
    UnsupportedVersion(i32, i32),
}

/// Newest PP file version this importer knows about.
pub const MAX_SUPPORTED_VERSION: i32 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
    Xml,
    XmlIdReferences,
    Binary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportInfo {
    pub format: FileFormat,
    pub encrypted: bool,
    pub compressed: bool,
    pub version: i32,
    pub warnings: Vec<String>,
}

pub struct ImportResult {
    pub client: Client,
    pub info: ImportInfo,
}

const SIG_ENCRYPTED: &[u8] = b"PORTFOLIO";
const SIG_ZIP: &[u8] = &[0x50, 0x4b, 0x03, 0x04];
const SIG_BINARY: &[u8] = b"PPPBV1";
const SALT: [u8; 16] = [
    112, 67, 103, 107, 164, 131, 144, 161, 159, 142, 117, 200, 203, 187, 231, 228,
];

pub fn is_encrypted(bytes: &[u8]) -> bool {
    bytes.starts_with(SIG_ENCRYPTED)
}

/// Reads a PP file from bytes. `password` is required for encrypted files.
pub fn read_client(bytes: &[u8], password: Option<&str>) -> Result<ImportResult, ImportError> {
    let mut encrypted = false;
    let mut compressed = false;

    let payload: Vec<u8>;
    let mut data: &[u8] = bytes;

    if bytes.starts_with(SIG_ENCRYPTED) {
        encrypted = true;
        let password = password
            .filter(|p| !p.is_empty())
            .ok_or(ImportError::PasswordRequired)?;
        payload = decrypt(bytes, password)?;
        data = &payload;
    }

    let unzipped: Vec<u8>;
    let mut zip_entry_binary = None;
    if data.starts_with(SIG_ZIP) {
        compressed = !encrypted;
        let mut archive = zip::ZipArchive::new(Cursor::new(data))
            .map_err(|e| ImportError::Format(format!("ZIP ungültig: {e}")))?;
        let mut file = archive
            .by_index(0)
            .map_err(|e| ImportError::Format(format!("ZIP leer: {e}")))?;
        zip_entry_binary = Some(
            file.name()
                .map(|n| n.ends_with(".portfolio"))
                .unwrap_or(false),
        );
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .map_err(|e| ImportError::Format(format!("ZIP lesen: {e}")))?;
        unzipped = buf;
        data = &unzipped;
    }

    let (mut client, mut warnings, format) = if data.starts_with(SIG_BINARY) {
        let (c, w) = proto::read_protobuf(&data[SIG_BINARY.len()..])?;
        (c, w, FileFormat::Binary)
    } else if zip_entry_binary == Some(true) {
        return Err(ImportError::Format("Binärdatei ohne Signatur".into()));
    } else {
        let text = String::from_utf8_lossy(data);
        let text = text.trim_start_matches('\u{feff}');
        let id_refs = text[..text.len().min(200)].contains("<client id=");
        let (c, w) = xml::read_xml(text)?;
        (
            c,
            w,
            if id_refs {
                FileFormat::XmlIdReferences
            } else {
                FileFormat::Xml
            },
        )
    };

    if client.version > MAX_SUPPORTED_VERSION {
        return Err(ImportError::UnsupportedVersion(
            client.version,
            MAX_SUPPORTED_VERSION,
        ));
    }

    let file_version = client.version;
    upgrade::upgrade(&mut client, &mut warnings);
    client.normalize();

    let info = ImportInfo {
        format,
        encrypted,
        compressed,
        version: file_version,
        warnings,
    };
    Ok(ImportResult { client, info })
}

fn decrypt(bytes: &[u8], password: &str) -> Result<Vec<u8>, ImportError> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};

    let rest = &bytes[SIG_ENCRYPTED.len()..];
    if rest.len() < 17 {
        return Err(ImportError::Format("Verschlüsselte Datei zu kurz".into()));
    }
    let method = rest[0];
    let iv = &rest[1..17];
    let cipher_text = &rest[17..];

    let key_len = if method == 1 { 32 } else { 16 };
    let mut key = vec![0u8; key_len];
    pbkdf2::pbkdf2_hmac::<sha1::Sha1>(password.as_bytes(), &SALT, 65536, &mut key);

    let mut buf = cipher_text.to_vec();
    let plain = if key_len == 32 {
        cbc::Decryptor::<aes::Aes256>::new_from_slices(&key, iv)
            .map_err(|_| ImportError::WrongPassword)?
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|_| ImportError::WrongPassword)?
            .to_vec()
    } else {
        cbc::Decryptor::<aes::Aes128>::new_from_slices(&key, iv)
            .map_err(|_| ImportError::WrongPassword)?
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|_| ImportError::WrongPassword)?
            .to_vec()
    };
    if plain.len() < 8 {
        return Err(ImportError::WrongPassword);
    }
    let content_type = i32::from_be_bytes(plain[0..4].try_into().unwrap());
    let version = i32::from_be_bytes(plain[4..8].try_into().unwrap());
    if !(1..=2).contains(&content_type) || !(1..=MAX_SUPPORTED_VERSION + 20).contains(&version) {
        return Err(ImportError::WrongPassword);
    }
    if version > MAX_SUPPORTED_VERSION {
        return Err(ImportError::UnsupportedVersion(
            version,
            MAX_SUPPORTED_VERSION,
        ));
    }
    Ok(plain[8..].to_vec())
}
