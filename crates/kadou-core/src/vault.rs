//! age-encrypted secret vault: flat needs payload, byte-for-byte Go envelope
//! (`docs/design/05-prd.md` §6.5, §9 slice 4).
//!
//! `vault.json` is `{"version":1,"data":"age1"+base64::STANDARD_NO_PAD(<binary age v1
//! ciphertext>)}` — the literal 4-character `"age1"` tag is a Go-side prefix, stripped
//! before base64 decoding; there is no base64 padding. The decrypted plaintext is a flat
//! `{ "<name>": { "value", "secret" } }` map — needs are one namespace across every folder,
//! not scoped per folder (§4.4, §6.5 "[shape]").

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use age::secrecy::ExposeSecret as _;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::fsutil;

const ENVELOPE_VERSION: u32 = 1;
const AGE_TAG: &str = "age1";

/// One vault entry: the resolved value plus whether it is secret-shaped (`docs/design/05-prd.md`
/// §6.5). A need with a header default never reaches the vault at all (§4.4); everything
/// stored here is either a plain non-secret value (`--plain`) or a secret one.
///
/// `ZeroizeOnDrop` (R13/E1, PRD §3.1 "wipe decrypted vault buffers"): `secret` doesn't need
/// wiping, so it's marked `#[zeroize(skip)]` — only `value`, the actual secret material, is
/// overwritten when an entry (or the `Vault` map holding it) is dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct VaultEntry {
    pub value: String,
    #[zeroize(skip)]
    pub secret: bool,
}

/// The flat needs payload (§6.5): one namespace across every folder, keyed by need name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vault {
    entries: BTreeMap<String, VaultEntry>,
}

impl Vault {
    pub fn get(&self, name: &str) -> Option<&VaultEntry> {
        self.entries.get(name)
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>, secret: bool) {
        self.entries.insert(
            name.into(),
            VaultEntry {
                value: value.into(),
                secret,
            },
        );
    }

    /// `true` if an entry with this name existed and was removed.
    pub fn remove(&mut self, name: &str) -> bool {
        self.entries.remove(name).is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `(name, secret)` pairs, sorted by name — `kadou vault list` never prints values
    /// (§6.5, §7.1).
    pub fn names(&self) -> impl Iterator<Item = (&str, bool)> {
        self.entries
            .iter()
            .map(|(name, entry)| (name.as_str(), entry.secret))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a valid vault envelope: {message}")]
    BadEnvelope { path: PathBuf, message: String },
    #[error("failed to decrypt the vault; check the identity at {path}: {source}")]
    Decrypt {
        path: PathBuf,
        #[source]
        source: age::DecryptError,
    },
    #[error("failed to encrypt the vault: {0}")]
    Encrypt(#[source] age::EncryptError),
    #[error("failed to serialize the vault: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("{path} is not a valid age identity: {message}")]
    BadIdentity { path: PathBuf, message: String },
    #[cfg(feature = "keyring")]
    #[error("keyring error: {0}")]
    Keyring(String),
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u32,
    data: String,
}

/// Parses `text` as a `{"version":1,"data":"age1..."}` envelope and returns the raw age
/// ciphertext bytes (§6.5 "specified byte-for-byte to match the Go product exactly").
fn parse_envelope(text: &str, path: &Path) -> Result<Vec<u8>, VaultError> {
    let envelope: Envelope =
        serde_json::from_str(text).map_err(|source| VaultError::BadEnvelope {
            path: path.to_path_buf(),
            message: source.to_string(),
        })?;
    if envelope.version != ENVELOPE_VERSION {
        return Err(VaultError::BadEnvelope {
            path: path.to_path_buf(),
            message: format!("unsupported envelope version {}", envelope.version),
        });
    }
    let encoded = envelope
        .data
        .strip_prefix(AGE_TAG)
        .ok_or_else(|| VaultError::BadEnvelope {
            path: path.to_path_buf(),
            message: format!("`data` does not start with the `{AGE_TAG}` tag"),
        })?;
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(encoded)
        .map_err(|source| VaultError::BadEnvelope {
            path: path.to_path_buf(),
            message: format!("invalid base64: {source}"),
        })
}

/// Renders `ciphertext` as the Go-compatible envelope JSON text (§6.5).
fn encode_envelope(ciphertext: &[u8]) -> Result<String, VaultError> {
    let data = format!(
        "{AGE_TAG}{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(ciphertext)
    );
    let envelope = Envelope {
        version: ENVELOPE_VERSION,
        data,
    };
    serde_json::to_string(&envelope).map_err(VaultError::Serialize)
}

/// Decrypts `ciphertext` with any of `identities`, reporting decrypt failures against
/// `path` (used for both kadou's own vault and a Go-imported one, §9 slice 4 "wrong key
/// gives a clean error"). The returned buffer wipes itself on drop (R13/E1) — the decrypted
/// plaintext is the vault's own secret material, in memory only as long as it takes to parse.
fn decrypt_with<'a>(
    ciphertext: &[u8],
    identities: impl Iterator<Item = &'a dyn age::Identity>,
    path: &Path,
) -> Result<zeroize::Zeroizing<Vec<u8>>, VaultError> {
    let decryptor =
        age::Decryptor::new_buffered(ciphertext).map_err(|source| VaultError::Decrypt {
            path: path.to_path_buf(),
            source,
        })?;
    let mut reader = decryptor
        .decrypt(identities)
        .map_err(|source| VaultError::Decrypt {
            path: path.to_path_buf(),
            source,
        })?;
    let mut plaintext = zeroize::Zeroizing::new(Vec::new());
    reader
        .read_to_end(&mut plaintext)
        .map_err(|source| VaultError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(plaintext)
}

/// The first non-comment, non-blank line of a standard age identity file — the
/// `AGE-SECRET-KEY-1...` string (`keys.txt` = "a standard age identity file", §6.5).
fn parse_identity_line(text: &str) -> Option<age::x25519::Identity> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| line.parse().ok())
}

/// `vault.json` + `keys/identity.txt` under the product's data dir (§6.5, §4.1).
pub struct VaultStore {
    data_dir: PathBuf,
    #[cfg(feature = "keyring")]
    keyring: bool,
}

impl VaultStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            #[cfg(feature = "keyring")]
            keyring: false,
        }
    }

    /// Switches this store to `[vault] keyring = true` mode (§6.5, §9 slice 4 "optional
    /// cargo feature `keyring`... off by default"): the identity is age scrypt-encrypted
    /// and the passphrase lives in the OS keyring rather than in `identity.txt` itself.
    #[cfg(feature = "keyring")]
    pub fn with_keyring(mut self) -> Self {
        self.keyring = true;
        self
    }

    fn keyring_enabled(&self) -> bool {
        #[cfg(feature = "keyring")]
        {
            self.keyring
        }
        #[cfg(not(feature = "keyring"))]
        {
            false
        }
    }

    pub fn vault_file(&self) -> PathBuf {
        self.data_dir.join("vault.json")
    }

    pub fn keys_dir(&self) -> PathBuf {
        self.data_dir.join("keys")
    }

    pub fn identity_file(&self) -> PathBuf {
        self.keys_dir().join("identity.txt")
    }

    /// The copy-once marker for [`Self::import_go_vault_once`] (§6.5 "records an import
    /// marker").
    fn go_import_marker(&self) -> PathBuf {
        self.data_dir.join("go-import.done")
    }

    pub fn go_import_done(&self) -> bool {
        self.go_import_marker().is_file()
    }

    pub fn exists(&self) -> bool {
        self.vault_file().is_file()
    }

    pub fn has_identity(&self) -> bool {
        self.identity_file().is_file()
    }

    /// Loads the identity at [`Self::identity_file`], generating one (dir `0700`, file
    /// `0600`) if it doesn't exist yet (§6.5 "Default identity: age X25519... `0600`").
    pub fn ensure_identity(&self) -> Result<age::x25519::Identity, VaultError> {
        if self.has_identity() {
            return self.load_identity();
        }
        let identity = age::x25519::Identity::generate();
        self.write_identity(&identity)?;
        Ok(identity)
    }

    fn load_identity(&self) -> Result<age::x25519::Identity, VaultError> {
        let path = self.identity_file();
        let text = std::fs::read_to_string(&path).map_err(|source| VaultError::Read {
            path: path.clone(),
            source,
        })?;

        if self.keyring_enabled() {
            return self.load_identity_via_keyring(&path, &text);
        }

        parse_identity_line(&text).ok_or(VaultError::BadIdentity {
            path,
            message: "no AGE-SECRET-KEY-1 line found".to_string(),
        })
    }

    #[cfg(feature = "keyring")]
    fn load_identity_via_keyring(
        &self,
        path: &Path,
        text: &str,
    ) -> Result<age::x25519::Identity, VaultError> {
        let ciphertext = parse_envelope(text, path)?;
        let line = keyring_backend::unwrap_identity(&ciphertext)?;
        line.parse().map_err(|_| VaultError::BadIdentity {
            path: path.to_path_buf(),
            message: "keyring-wrapped identity did not decode to AGE-SECRET-KEY-1".to_string(),
        })
    }

    #[cfg(not(feature = "keyring"))]
    fn load_identity_via_keyring(
        &self,
        _path: &Path,
        _text: &str,
    ) -> Result<age::x25519::Identity, VaultError> {
        unreachable!("keyring_enabled() is always false without the `keyring` feature")
    }

    fn write_identity(&self, identity: &age::x25519::Identity) -> Result<(), VaultError> {
        fsutil::ensure_dir_0700(&self.keys_dir()).map_err(|source| VaultError::Write {
            path: self.keys_dir(),
            source,
        })?;

        let path = self.identity_file();
        let secret = identity.to_string();
        let line = secret.expose_secret();

        if self.keyring_enabled() {
            return self.write_identity_via_keyring(&path, line);
        }

        let public = identity.to_public();
        let text = format!(
            "# created: {}\n# public key: {public}\n{line}\n",
            humantime::format_rfc3339(std::time::SystemTime::now())
        );
        fsutil::write_atomic_0600(&path, text.as_bytes())
            .map_err(|source| VaultError::Write { path, source })
    }

    #[cfg(feature = "keyring")]
    fn write_identity_via_keyring(&self, path: &Path, line: &str) -> Result<(), VaultError> {
        let ciphertext = keyring_backend::wrap_identity(line)?;
        let text = encode_envelope(&ciphertext)?;
        fsutil::write_atomic_0600(path, text.as_bytes()).map_err(|source| VaultError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    #[cfg(not(feature = "keyring"))]
    fn write_identity_via_keyring(&self, _path: &Path, _line: &str) -> Result<(), VaultError> {
        unreachable!("keyring_enabled() is always false without the `keyring` feature")
    }

    /// Loads the vault, decrypting with the existing identity. An absent `vault.json` is an
    /// empty vault, not an error — every need with no vault entry is then simply `missing`
    /// (§4.4).
    pub fn load(&self) -> Result<Vault, VaultError> {
        if !self.exists() {
            return Ok(Vault::default());
        }
        let identity = self.load_identity()?;
        let path = self.vault_file();
        let text = std::fs::read_to_string(&path).map_err(|source| VaultError::Read {
            path: path.clone(),
            source,
        })?;
        let ciphertext = parse_envelope(&text, &path)?;
        let plaintext = decrypt_with(
            &ciphertext,
            std::iter::once(&identity as &dyn age::Identity),
            &path,
        )?;
        let entries: BTreeMap<String, VaultEntry> =
            serde_json::from_slice(&plaintext).map_err(|source| VaultError::BadEnvelope {
                path,
                message: source.to_string(),
            })?;
        Ok(Vault { entries })
    }

    /// Encrypts and atomically writes `vault` (§6.5 "0700"/"0600" atomic writes),
    /// generating an identity first if this is the first save.
    pub fn save(&self, vault: &Vault) -> Result<(), VaultError> {
        fsutil::ensure_dir_0700(&self.data_dir).map_err(|source| VaultError::Write {
            path: self.data_dir.clone(),
            source,
        })?;
        let identity = self.ensure_identity()?;
        let recipient = identity.to_public();
        // The serialized plaintext holds every secret value in the vault; wipe it on drop
        // (R13/E1) the same way the decrypt path already does.
        let plaintext: zeroize::Zeroizing<Vec<u8>> = zeroize::Zeroizing::new(
            serde_json::to_vec(&vault.entries).map_err(VaultError::Serialize)?,
        );
        let ciphertext = age::encrypt(&recipient, &plaintext).map_err(VaultError::Encrypt)?;
        let text = encode_envelope(&ciphertext)?;
        let path = self.vault_file();
        fsutil::write_atomic_0600(&path, text.as_bytes())
            .map_err(|source| VaultError::Write { path, source })
    }

    /// Copy-once, read-only import against a Go `~/.dops` tree (§6.5, §9 slice 4). A no-op
    /// (`Ok(None)`) when the marker already exists or the source is incomplete. Never opens
    /// `source.vault_json`/`source.keys_txt` for writing.
    pub fn import_go_vault_once(
        &self,
        source: &GoVaultSource,
    ) -> Result<Option<GoImportSummary>, VaultError> {
        if self.go_import_done() || !source.exists() {
            return Ok(None);
        }

        let vault_json =
            std::fs::read(&source.vault_json).map_err(|source_err| VaultError::Read {
                path: source.vault_json.clone(),
                source: source_err,
            })?;
        let keys_txt = std::fs::read(&source.keys_txt).map_err(|source_err| VaultError::Read {
            path: source.keys_txt.clone(),
            source: source_err,
        })?;

        let (imported, summary) = decrypt_go_vault(&vault_json, &keys_txt)?;

        // Copies the identity byte-for-byte into kadou's own data dir, the way §6.5 phrases
        // it ("copies them into its own XDG data dir once") — but only when kadou doesn't
        // already have an identity of its own: reusing an existing identity keeps any
        // vault entries a human already saved with `kadou vault set` readable.
        if !self.has_identity() {
            fsutil::ensure_dir_0700(&self.keys_dir()).map_err(|source_err| VaultError::Write {
                path: self.keys_dir(),
                source: source_err,
            })?;
            fsutil::write_atomic_0600(&self.identity_file(), &keys_txt).map_err(|source_err| {
                VaultError::Write {
                    path: self.identity_file(),
                    source: source_err,
                }
            })?;
        }

        let mut vault = self.load()?;
        for (name, secret) in imported.names() {
            if vault.get(name).is_none()
                && let Some(entry) = imported.get(name)
            {
                vault.set(name, entry.value.clone(), secret);
            }
        }
        self.save(&vault)?;

        fsutil::write_atomic_0600(&self.go_import_marker(), b"imported\n").map_err(
            |source_err| VaultError::Write {
                path: self.go_import_marker(),
                source: source_err,
            },
        )?;

        Ok(Some(summary))
    }
}

/// Locates a legacy dops install's vault files under `home` (§6.5 "read-only against
/// `~/.dops/vault.json` + `~/.dops/keys/keys.txt`"). `home` is passed in explicitly rather
/// than read from `$HOME` here, so this stays a pure, testable path — the `kadou` bin
/// resolves the real `$HOME` once at the CLI layer.
pub struct GoVaultSource {
    pub vault_json: PathBuf,
    pub keys_txt: PathBuf,
}

impl GoVaultSource {
    pub fn under_home(home: &Path) -> Self {
        Self {
            vault_json: home.join(".dops/vault.json"),
            keys_txt: home.join(".dops/keys/keys.txt"),
        }
    }

    /// Both files must exist for an import to proceed (§6.5 "if both exist and convert").
    pub fn exists(&self) -> bool {
        self.vault_json.is_file() && self.keys_txt.is_file()
    }
}

/// What a Go vault import did, for the CLI to report (§4.6 "prints per-file diffs" applied
/// to the vault path; §6.5 "reports... `catalog.*` runbook-scope values").
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GoImportSummary {
    /// `global.*` names copied into the flat payload.
    pub imported: Vec<String>,
    /// Dotted `catalog.*` key paths that were reported and dropped (§6.5 "reports and
    /// drops `catalog.*` scope values" — they are never silently kept as a vault scope,
    /// since scopes no longer exist).
    pub dropped_catalog_keys: Vec<String>,
}

/// The Go decrypted vault plaintext shape (`docs/design/01-audit.md` §4.7): `{"global": {name:
/// value}, "catalog": {...}}`. Values are raw strings with no per-value secret marker.
#[derive(Debug, Deserialize)]
struct GoPayload {
    #[serde(default)]
    global: BTreeMap<String, String>,
    #[serde(default)]
    catalog: serde_json::Map<String, serde_json::Value>,
}

/// Decrypts a Go `vault.json` (bytes of the envelope file) using `keys.txt` (bytes of a
/// standard age identity file) and maps `global.*` to the flat payload (§6.5, §4.6 "Global
/// params").
///
/// Every migrated value is written `secret: true`. The Go decrypted payload carries no
/// per-value secret marker — it is a plain `{name: value}` map (`01-audit.md` §4.7), not
/// `{name: {value, secret}}` — so there is no bit to read off the vault itself; the PRD's
/// "secret bit from the source YAML's `secret: true` flags" (§6.5) would require re-parsing
/// `runbook.yaml` during what is otherwise a vault-only, read-only import against two
/// `~/.dops` files, which is out of scope here. Marking every import `secret: true` is the
/// fail-safe default; a human can downgrade a specific name afterward with `kadou vault set
/// --plain <name>`. Noted as an interpretation call in the handoff.
pub fn decrypt_go_vault(
    vault_json: &[u8],
    keys_txt: &[u8],
) -> Result<(Vault, GoImportSummary), VaultError> {
    let keys_path = PathBuf::from("keys.txt");
    let identities = age::IdentityFile::from_buffer(keys_txt)
        .map_err(|source| VaultError::BadIdentity {
            path: keys_path.clone(),
            message: source.to_string(),
        })?
        .into_identities()
        .map_err(|source| VaultError::Decrypt {
            path: keys_path,
            source,
        })?;

    let vault_path = PathBuf::from("vault.json");
    let text = std::str::from_utf8(vault_json).map_err(|_| VaultError::BadEnvelope {
        path: vault_path.clone(),
        message: "not valid UTF-8".to_string(),
    })?;
    let ciphertext = parse_envelope(text, &vault_path)?;
    let plaintext = decrypt_with(
        &ciphertext,
        identities.iter().map(|b| b.as_ref() as &dyn age::Identity),
        &vault_path,
    )?;

    let payload: GoPayload =
        serde_json::from_slice(&plaintext).map_err(|source| VaultError::BadEnvelope {
            path: vault_path,
            message: source.to_string(),
        })?;

    let mut vault = Vault::default();
    let mut summary = GoImportSummary::default();
    for (name, value) in payload.global {
        vault.set(name.clone(), value, true);
        summary.imported.push(name);
    }
    summary.imported.sort();

    collect_catalog_keys(
        "catalog",
        &serde_json::Value::Object(payload.catalog),
        &mut summary.dropped_catalog_keys,
    );
    summary.dropped_catalog_keys.sort();

    Ok((vault, summary))
}

/// Walks the Go `catalog.*` tree, collecting one dotted key path per leaf value — these are
/// the runbook-scoped saved values that never survive the flat rewrite (§6.5).
fn collect_catalog_keys(prefix: &str, value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                collect_catalog_keys(&format!("{prefix}.{key}"), child, out);
            }
        }
        _ => out.push(prefix.to_string()),
    }
}

/// `[vault] keyring = true` support (§6.5, §9 slice 4): the identity is age
/// scrypt-encrypted; the passphrase — not the identity file — lives in the OS keyring via
/// `keyring-core` + `apple-native-keyring-store`, service `kadou`, account
/// `vault-identity`. Compiled and tested behind the `keyring` cargo feature; off by
/// default.
#[cfg(feature = "keyring")]
pub mod keyring_backend {
    use age::secrecy::SecretString;

    use super::VaultError;

    const SERVICE: &str = "kadou";
    const ACCOUNT: &str = "vault-identity";

    /// Registers `apple-native-keyring-store` as the `keyring-core` default backend. Call
    /// once, before the first [`wrap_identity`]/[`unwrap_identity`], when `[vault] keyring
    /// = true` (macOS only — this crate's `apple-native-keyring-store` dependency is the
    /// macOS Keychain backend named in §3.1).
    pub fn register_apple_backend() -> Result<(), VaultError> {
        let store = apple_native_keyring_store::keychain::Store::new()
            .map_err(|source| VaultError::Keyring(source.to_string()))?;
        keyring_core::set_default_store(store);
        Ok(())
    }

    fn entry() -> Result<keyring_core::Entry, VaultError> {
        keyring_core::Entry::new(SERVICE, ACCOUNT)
            .map_err(|source| VaultError::Keyring(source.to_string()))
    }

    fn store_passphrase(passphrase: &str) -> Result<(), VaultError> {
        entry()?
            .set_password(passphrase)
            .map_err(|source| VaultError::Keyring(source.to_string()))
    }

    fn load_passphrase() -> Result<String, VaultError> {
        entry()?
            .get_password()
            .map_err(|source| VaultError::Keyring(source.to_string()))
    }

    /// 256 bits of randomness from two v4 UUIDs — a passphrase never typed by a human and
    /// held only in the OS keyring, so readability does not matter.
    fn generate_passphrase() -> String {
        format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4())
    }

    /// Encrypts `identity_line` (the `AGE-SECRET-KEY-1...` string) with a fresh scrypt
    /// passphrase, stores the passphrase in the keyring, and returns the ciphertext to
    /// write to `identity.txt` in place of the plaintext line.
    pub fn wrap_identity(identity_line: &str) -> Result<Vec<u8>, VaultError> {
        let passphrase = generate_passphrase();
        let recipient = age::scrypt::Recipient::new(SecretString::from(passphrase.clone()));
        let ciphertext =
            age::encrypt(&recipient, identity_line.as_bytes()).map_err(VaultError::Encrypt)?;
        store_passphrase(&passphrase)?;
        Ok(ciphertext)
    }

    /// Reverses [`wrap_identity`]: fetches the passphrase from the keyring and decrypts
    /// `ciphertext` back to the `AGE-SECRET-KEY-1...` line.
    pub fn unwrap_identity(ciphertext: &[u8]) -> Result<String, VaultError> {
        let passphrase = load_passphrase()?;
        let identity = age::scrypt::Identity::new(SecretString::from(passphrase));
        let plaintext =
            age::decrypt(&identity, ciphertext).map_err(|source| VaultError::Decrypt {
                path: std::path::PathBuf::from("identity.txt"),
                source,
            })?;
        String::from_utf8(plaintext)
            .map_err(|_| VaultError::Keyring("decrypted identity is not valid UTF-8".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R13/E1: the decrypted plaintext (a secret value, in memory) must be wiped on drop.
    /// Hard to observe the actual memory wipe from safe Rust, so this pins the type-level
    /// contract instead — if `VaultEntry` ever stops deriving `ZeroizeOnDrop`, this fails to
    /// compile rather than silently losing the guarantee.
    fn assert_zeroize_on_drop<T: zeroize::ZeroizeOnDrop>() {}

    #[test]
    fn vault_entry_zeroizes_its_value_on_drop() {
        assert_zeroize_on_drop::<VaultEntry>();
    }

    #[test]
    fn round_trips_a_vault_through_the_real_envelope_and_a_fresh_identity() {
        let dir = tempfile::tempdir().unwrap();
        let store = VaultStore::new(dir.path());

        let mut vault = Vault::default();
        vault.set("jenkins_user", "ci-user", false);
        vault.set("jenkins_token", "not-a-real-token", true);
        store.save(&vault).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(loaded.get("jenkins_user").unwrap().value, "ci-user");
        assert!(!loaded.get("jenkins_user").unwrap().secret);
        assert_eq!(
            loaded.get("jenkins_token").unwrap().value,
            "not-a-real-token"
        );
        assert!(loaded.get("jenkins_token").unwrap().secret);
    }

    #[test]
    fn on_disk_envelope_matches_the_go_shape_and_files_are_0600() {
        let dir = tempfile::tempdir().unwrap();
        let store = VaultStore::new(dir.path());
        let mut vault = Vault::default();
        vault.set("k", "v", true);
        store.save(&vault).unwrap();

        let text = std::fs::read_to_string(store.vault_file()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["version"], 1);
        assert!(json["data"].as_str().unwrap().starts_with("age1"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let vault_mode = std::fs::metadata(store.vault_file())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(vault_mode, 0o600);
            let identity_mode = std::fs::metadata(store.identity_file())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(identity_mode, 0o600);
            let keys_dir_mode = std::fs::metadata(store.keys_dir())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(keys_dir_mode, 0o700);
            let data_dir_mode = std::fs::metadata(dir.path()).unwrap().permissions().mode() & 0o777;
            assert_eq!(data_dir_mode, 0o700);
        }
    }

    #[test]
    fn missing_vault_file_loads_as_empty_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = VaultStore::new(dir.path());
        let vault = store.load().unwrap();
        assert!(vault.is_empty());
        assert!(!store.has_identity(), "load must not generate an identity");
    }

    #[test]
    fn wrong_identity_gives_a_clean_decrypt_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = VaultStore::new(dir.path());
        let mut vault = Vault::default();
        vault.set("k", "v", true);
        store.save(&vault).unwrap();

        // Swap in an unrelated identity after the vault was written to it.
        let other = age::x25519::Identity::generate();
        let text = format!(
            "# public key: {}\n{}\n",
            other.to_public(),
            other.to_string().expose_secret()
        );
        std::fs::write(store.identity_file(), text).unwrap();

        let err = store.load().unwrap_err();
        assert!(matches!(err, VaultError::Decrypt { .. }), "{err:?}");
    }

    fn synthetic_go_vault(
        global: &[(&str, &str)],
        catalog: serde_json::Value,
    ) -> (Vec<u8>, Vec<u8>, age::x25519::Identity) {
        let identity = age::x25519::Identity::generate();
        let recipient = identity.to_public();

        let mut global_map = serde_json::Map::new();
        for (k, v) in global {
            global_map.insert(
                (*k).to_string(),
                serde_json::Value::String((*v).to_string()),
            );
        }
        let payload = serde_json::json!({ "global": global_map, "catalog": catalog });
        let plaintext = serde_json::to_vec(&payload).unwrap();
        let ciphertext = age::encrypt(&recipient, &plaintext).unwrap();
        let vault_json = encode_envelope(&ciphertext).unwrap().into_bytes();

        let keys_txt = format!(
            "# created: synthetic-test-fixture\n{}\n",
            identity.to_string().expose_secret()
        )
        .into_bytes();

        (vault_json, keys_txt, identity)
    }

    #[test]
    fn go_envelope_import_decrypts_and_maps_global_to_flat_entries() {
        let (vault_json, keys_txt, _identity) = synthetic_go_vault(
            &[
                ("jenkins_url", "https://ci.example.com"),
                ("jenkins_token", "not-a-real-token"),
            ],
            serde_json::json!({
                "sesami": {
                    "runbooks": {
                        "cc4-aaa": { "branch": "dev" }
                    }
                }
            }),
        );

        let (vault, summary) = decrypt_go_vault(&vault_json, &keys_txt).unwrap();
        assert_eq!(
            vault.get("jenkins_url").unwrap().value,
            "https://ci.example.com"
        );
        assert!(
            vault.get("jenkins_url").unwrap().secret,
            "safe default is secret:true"
        );
        assert_eq!(
            vault.get("jenkins_token").unwrap().value,
            "not-a-real-token"
        );
        assert_eq!(
            summary.imported,
            vec!["jenkins_token".to_string(), "jenkins_url".to_string()]
        );
        assert_eq!(
            summary.dropped_catalog_keys,
            vec!["catalog.sesami.runbooks.cc4-aaa.branch".to_string()]
        );
    }

    #[test]
    fn go_envelope_import_wrong_key_is_a_clean_error() {
        let (vault_json, _keys_txt, _identity) =
            synthetic_go_vault(&[("k", "v")], serde_json::json!({}));
        let (_, other_keys_txt, _other) =
            synthetic_go_vault(&[("unused", "x")], serde_json::json!({}));

        let err = decrypt_go_vault(&vault_json, &other_keys_txt).unwrap_err();
        assert!(matches!(err, VaultError::Decrypt { .. }), "{err:?}");
    }

    #[test]
    fn go_vault_import_is_idempotent_and_leaves_the_source_untouched() {
        let dops_home = tempfile::tempdir().unwrap();
        let (vault_json, keys_txt, _identity) =
            synthetic_go_vault(&[("jenkins_user", "ci-user")], serde_json::json!({}));
        let dops_vault = dops_home.path().join(".dops/vault.json");
        let dops_keys = dops_home.path().join(".dops/keys/keys.txt");
        std::fs::create_dir_all(dops_vault.parent().unwrap()).unwrap();
        std::fs::create_dir_all(dops_keys.parent().unwrap()).unwrap();
        std::fs::write(&dops_vault, &vault_json).unwrap();
        std::fs::write(&dops_keys, &keys_txt).unwrap();

        let kadou_data = tempfile::tempdir().unwrap();
        let store = VaultStore::new(kadou_data.path());
        let source = GoVaultSource::under_home(dops_home.path());

        let summary = store
            .import_go_vault_once(&source)
            .unwrap()
            .expect("first import runs");
        assert_eq!(summary.imported, vec!["jenkins_user".to_string()]);
        assert!(store.go_import_done());

        let vault = store.load().unwrap();
        assert_eq!(vault.get("jenkins_user").unwrap().value, "ci-user");

        // Idempotent: a second call is a no-op.
        let second = store.import_go_vault_once(&source).unwrap();
        assert!(second.is_none());

        // Non-destructive: the source files are byte-identical to what was written.
        assert_eq!(std::fs::read(&dops_vault).unwrap(), vault_json);
        assert_eq!(std::fs::read(&dops_keys).unwrap(), keys_txt);
    }

    #[test]
    fn go_vault_import_does_not_clobber_an_existing_entry() {
        let dops_home = tempfile::tempdir().unwrap();
        let (vault_json, keys_txt, _identity) =
            synthetic_go_vault(&[("jenkins_user", "from-go")], serde_json::json!({}));
        let dops_vault = dops_home.path().join(".dops/vault.json");
        let dops_keys = dops_home.path().join(".dops/keys/keys.txt");
        std::fs::create_dir_all(dops_vault.parent().unwrap()).unwrap();
        std::fs::create_dir_all(dops_keys.parent().unwrap()).unwrap();
        std::fs::write(&dops_vault, &vault_json).unwrap();
        std::fs::write(&dops_keys, &keys_txt).unwrap();

        let kadou_data = tempfile::tempdir().unwrap();
        let store = VaultStore::new(kadou_data.path());
        let mut existing = Vault::default();
        existing.set("jenkins_user", "already-set-by-a-human", false);
        store.save(&existing).unwrap();

        store
            .import_go_vault_once(&GoVaultSource::under_home(dops_home.path()))
            .unwrap();

        let vault = store.load().unwrap();
        assert_eq!(
            vault.get("jenkins_user").unwrap().value,
            "already-set-by-a-human"
        );
    }

    #[test]
    fn missing_go_source_is_a_silent_no_op() {
        let dops_home = tempfile::tempdir().unwrap();
        let kadou_data = tempfile::tempdir().unwrap();
        let store = VaultStore::new(kadou_data.path());
        let result = store
            .import_go_vault_once(&GoVaultSource::under_home(dops_home.path()))
            .unwrap();
        assert!(result.is_none());
        assert!(!store.go_import_done());
    }

    #[cfg(feature = "keyring")]
    mod keyring_tests {
        use super::*;

        /// Exercises the passphrase-in-keyring plumbing against `keyring-core`'s in-memory
        /// mock backend rather than the real macOS keychain — deterministic, and leaves no
        /// residue in Mason's actual Keychain. `apple-native-keyring-store` is still a real
        /// compiled dependency behind this feature (proving it "compiles on the macOS
        /// target", §9 slice 4); wiring it as the default store is a `kadou` bin-level
        /// concern exercised manually, not by this suite. Noted as an interpretation call
        /// in the handoff.
        fn use_mock_store() {
            keyring_core::set_default_store(keyring_core::mock::Store::new().unwrap());
        }

        #[test]
        fn identity_wrap_round_trips_through_the_mock_keyring() {
            use_mock_store();
            let identity = age::x25519::Identity::generate();
            let line = identity.to_string();
            let line = line.expose_secret();

            let ciphertext = keyring_backend::wrap_identity(line).unwrap();
            let recovered = keyring_backend::unwrap_identity(&ciphertext).unwrap();
            assert_eq!(recovered, line);
        }

        #[test]
        fn vault_store_with_keyring_round_trips_end_to_end() {
            use_mock_store();
            let dir = tempfile::tempdir().unwrap();
            let store = VaultStore::new(dir.path()).with_keyring();

            let mut vault = Vault::default();
            vault.set("jenkins_token", "not-a-real-token", true);
            store.save(&vault).unwrap();

            // identity.txt holds a keyring-wrapped envelope, not a plaintext AGE-SECRET-KEY
            // line.
            let text = std::fs::read_to_string(store.identity_file()).unwrap();
            assert!(!text.contains("AGE-SECRET-KEY-1"));
            assert!(text.contains("age1"));

            let loaded = store.load().unwrap();
            assert_eq!(
                loaded.get("jenkins_token").unwrap().value,
                "not-a-real-token"
            );
        }
    }
}
