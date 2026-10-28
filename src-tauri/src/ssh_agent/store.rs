// In-memory store of decrypted SSH keys that the agent advertises and signs
// with. Each `StoredIdentity` holds a decoded `ssh_key::PrivateKey` (the secret
// material). ssh-key zeroizes that material when the value is dropped, so
// removing a db's slice or clearing the store wipes the keys from memory.
//
// This module is transport-neutral: the unix-socket `Session` adapter and, in
// the Windows named-pipe / Pageant adapters all call the same
// `identities()` / `sign()` entry points here.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ssh_agent_lib::error::AgentError;
use ssh_agent_lib::proto::{Identity, PublicCredential};

// ssh-key 0.6.7, the same version ssh-agent-lib re-exports (see Cargo.toml).
use ssh_key::public::KeyData;
use ssh_key::{Algorithm, HashAlg, PrivateKey, PublicKey, Signature};

use onekeepass_core::db_service::ssh_agent::SshAgentKeySource;

// SIGN_REQUEST flag bits from the agent protocol (draft-miller-ssh-agent).
const SSH_AGENT_RSA_SHA2_256: u32 = 0x02;
const SSH_AGENT_RSA_SHA2_512: u32 = 0x04;

// One agent identity: a decoded private key plus the metadata the agent needs.
// Deliberately not `Debug`/`Serialize` — it carries secret key material.
struct StoredIdentity {
    // Owning database; used to remove this identity when its db is locked/closed.
    db_key: String,
    // The agent "comment" advertised to clients (the key entry title).
    comment: String,
    // Decoded + decrypted private key. Zeroized on drop by ssh-key.
    key: PrivateKey,
    // Modern RustCrypto-based ECDSA key, kept only for ECDSA identities.
    modern_ecdsa_key: Option<ssh_key_ppk::PrivateKey>,
    // Public half, derived from `key`. The advertised identity and the value a
    // sign request is matched against. Never the entry's stored "Public Key".
    public_key_data: KeyData,
    // When true, each sign request for this key must be user-confirmed.
    require_confirmation: bool,
    // "Agent Lifetime": when set, the identity stops being advertised/signable
    // once this instant passes (and is pruned + zeroized by the sweep task).
    // `None` means it lives until the database is locked/closed. The clock starts
    // when the key is (re)loaded — on agent start, db unlock/open, or entry edit.
    expires_at: Option<Instant>,
}

// Metadata the Session needs to decide whether (and how) to prompt before
// signing. Cloned out from under the store lock so the async confirmation wait
// holds no lock.
pub(crate) struct ConfirmInfo {
    pub require_confirmation: bool,
    pub comment: String,
    pub fingerprint: String,
    pub generation: u64,
}

// Shared decoded representation used by embedded Agent Mode and external Client
// Mode. It is intentionally short-lived in Client Mode: after sending the
// private key to the external agent, the value is dropped and zeroized by
// ssh-key.
pub(crate) struct DecodedIdentity {
    pub(crate) db_key: String,
    pub(crate) comment: String,
    pub(crate) key: PrivateKey,
    pub(crate) public_key_data: KeyData,
    pub(crate) require_confirmation: bool,
    pub(crate) lifetime: Option<Duration>,
}

impl DecodedIdentity {
    pub(crate) fn fingerprint(&self) -> String {
        self.public_key_data.fingerprint(HashAlg::Sha256).to_string()
    }
}

impl StoredIdentity {
    // Decodes one key source into a signable identity. Returns a human-readable
    // error (logged by the caller) when the key can't be parsed/decrypted or
    // when the stored public key contradicts the private key.
    fn from_source(src: &SshAgentKeySource) -> Result<Self, String> {
        let decoded = decode_identity(src)?;
        // Keep the stable SSH Agent wire types while using the modern
        // cryptographic implementation for ECDSA P-256/P-384/P-521.
        let modern_ecdsa_key = if matches!(decoded.key.algorithm(), Algorithm::Ecdsa { .. }) {
            // The legacy parser has already normalized PPK/OpenSSH input,
            // validated the public key, and decrypted any passphrase.
            // Re-encode that same key in memory rather than using decrypt
            // APIs not provided by the modern SSH signing implementation.
            let plain_pem = decoded.key
                .to_openssh(ssh_key::LineEnding::LF)
                .map_err(|e| format!("ECDSA private key serialization failed: {e}"))?;
            let modern = ssh_key_ppk::PrivateKey::from_openssh(plain_pem.as_str())
                .map_err(|e| format!("Modern ECDSA private key parse failed: {e}"))?;
            Some(modern)
        } else {
            None
        };
        let expires_at = decoded.lifetime
            .map(|d| Instant::now().checked_add(d)
                .ok_or_else(|| "Agent Lifetime exceeds the supported range".to_string()))
            .transpose()?;

        Ok(Self {
            db_key: decoded.db_key,
            comment: decoded.comment,
            key: decoded.key,
            modern_ecdsa_key,
            public_key_data: decoded.public_key_data,
            require_confirmation: decoded.require_confirmation,
            expires_at,
        })
    }

    fn fingerprint(&self) -> String {
        self.public_key_data.fingerprint(HashAlg::Sha256).to_string()
    }

    // True once the "Agent Lifetime" deadline has passed. Keys with no lifetime
    // never expire (they are cleared on db lock/close instead).
    fn is_expired(&self) -> bool {
        self.expires_at.map_or(false, |t| Instant::now() >= t)
    }

    fn sign(&self, data: &[u8], flags: u32) -> Result<Signature, String> {
        match self.key.algorithm() {
            // ssh-key 0.6.7's high-level RSA signer is broken (it builds primes
            // as [p, p]); drive the rsa crate directly with the correct primes.
            Algorithm::Rsa { .. } => sign_rsa_with_flags(&self.key, data, flags),
            Algorithm::Ecdsa { .. } => {
                let key = self.modern_ecdsa_key.as_ref()
                    .ok_or_else(|| "missing modern ECDSA signing key".to_string())?;
                let sig: ssh_key_ppk::Signature = signature3::Signer::try_sign(key, data)
                    .map_err(|e| format!("modern ECDSA signing failed: {e}"))?;
                Signature::new(self.key.algorithm(), sig.as_bytes().to_vec())
                    .map_err(|e| format!("ECDSA agent signature encoding failed: {e}"))
            }
            // Preserve the existing Ed25519 and RSA signing behavior.
            _ => signature::Signer::try_sign(&self.key, data)
                .map_err(|e| format!("sign failed: {e}")),
        }
    }
}

pub(crate) fn decode_identity(src: &SshAgentKeySource) -> Result<DecodedIdentity, String> {
    let pem = normalize_private_key_pem(
        &src.private_key_pem,
        src.passphrase.as_deref().filter(|p| !p.is_empty()),
    )?;
    let parsed =
        PrivateKey::from_openssh(pem.trim()).map_err(|e| format!("private key parse failed: {e}"))?;

    let key = if parsed.is_encrypted() {
        let pass = src
            .passphrase
            .as_deref()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| "key is encrypted but no passphrase is set".to_string())?;
        parsed
            .decrypt(pass)
            .map_err(|e| format!("private key decrypt failed: {e}"))?
    } else {
        parsed
    };

    let public_key_data = key.public_key().key_data().clone();

    // If the entry also stores a "Public Key", it must match the key we
    // derived. We never advertise one public key while signing with a different
    // private key.
    if let Some(stored_pub) = src.public_key.as_deref().map(str::trim) {
        if !stored_pub.is_empty() {
            if let Ok(parsed_pub) = PublicKey::from_openssh(stored_pub) {
                if parsed_pub.key_data() != &public_key_data {
                    return Err("stored Public Key does not match the Private Key; skipping".into());
                }
            }
        }
    }

    // Prefer the entry title as the comment; fall back to any comment baked
    // into the OpenSSH key.
    let comment = if src.title.trim().is_empty() {
        key.comment().to_string()
    } else {
        src.title.clone()
    };

    // Invalid lifetimes must not silently become unlimited authorization.
    // An empty or zero value explicitly requests the no-expiry behavior.
    let lifetime = src.agent_lifetime.as_deref()
        .map(parse_lifetime)
        .transpose()?
        .flatten();

    Ok(DecodedIdentity {
        db_key: src.db_key.clone(),
        comment,
        key,
        public_key_data,
        require_confirmation: src.require_confirmation,
        lifetime,
    })
}

// Parses seconds chosen in the desktop entry form. Empty/zero explicitly means
// no lifetime; malformed or unreasonably large values reject the key instead of
// silently allowing indefinite signing.
fn parse_lifetime(input: &str) -> Result<Option<Duration>, String> {
    let s = input.trim();
    if s.is_empty() || s == "0" {
        return Ok(None);
    }
    let secs = s.parse::<u64>()
        .map_err(|_| "Invalid Agent Lifetime; expected a number of seconds".to_string())?;
    if secs == 0 {
        return Ok(None);
    }
    if secs > u32::MAX as u64 {
        return Err("Agent Lifetime exceeds maximum supported seconds".into());
    }
    Ok(Some(Duration::from_secs(secs)))
}

// Accepts the key formats stored by SSH Key entries and normalizes them to the
// OpenSSH text format used by the rest of the agent code.
fn normalize_private_key_pem(input: &str, passphrase: Option<&str>) -> Result<String, String> {
    let trimmed = input.trim_start();
    if trimmed.starts_with("PuTTY-User-Key-File-") {
        return ppk_to_openssh(input, passphrase);
    }

    Ok(normalize_openssh_pem(input))
}

// Converts a PuTTY PPK key into OpenSSH text without changing the saved entry.
fn ppk_to_openssh(input: &str, passphrase: Option<&str>) -> Result<String, String> {
    let ppk_key = ssh_key_ppk::PrivateKey::from_ppk(input, passphrase.map(str::to_string))
        .map_err(|e| format!("PPK private key parse failed: {e}"))?;
    ppk_key
        .to_openssh(ssh_key_ppk::LineEnding::LF)
        .map(|pem| pem.to_string())
        .map_err(|e| format!("PPK to OpenSSH conversion failed: {e}"))
}

// Repairs an OpenSSH private key whose PEM line structure was flattened — e.g.
// pasted into a text field that collapsed the newlines, leaving the
// `-----BEGIN/END-----` markers and the base64 body run together with spaces.
// ssh-key's parser requires the markers on their own lines and the body wrapped,
// so we strip all whitespace from the body and re-wrap it canonically. A key
// that is already well-formed round-trips through this unchanged. Inputs without
// the OpenSSH markers (e.g. PKCS#8) are returned untouched and left to fail in
// the parser as before.
fn normalize_openssh_pem(input: &str) -> String {
    const BEGIN: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
    const END: &str = "-----END OPENSSH PRIVATE KEY-----";

    let s = input.trim();
    let (Some(b), Some(e)) = (s.find(BEGIN), s.find(END)) else {
        return input.to_string();
    };
    if e <= b {
        return input.to_string();
    }

    let body = &s[b + BEGIN.len()..e];
    let b64: String = body.chars().filter(|c| !c.is_whitespace()).collect();

    let mut wrapped = String::with_capacity(b64.len() + b64.len() / 70 + 2);
    let mut i = 0;
    while i < b64.len() {
        let end = (i + 70).min(b64.len());
        wrapped.push_str(&b64[i..end]);
        wrapped.push('\n');
        i = end;
    }

    format!("{BEGIN}\n{wrapped}{END}\n")
}

// Produces an `ssh_key::Signature` honoring the agent's requested RSA hash flag.
// Defaults to SHA-256 when neither flag is set; never emits legacy SHA-1.
fn sign_rsa_with_flags(key: &PrivateKey, msg: &[u8], flags: u32) -> Result<Signature, String> {
    use rsa::pkcs1v15::SigningKey;
    use rsa::signature::{SignatureEncoding, Signer as _};
    use rsa::RsaPrivateKey;

    let keypair = match key.key_data() {
        ssh_key::private::KeypairData::Rsa(kp) => kp,
        _ => return Err("not an RSA key".into()),
    };

    let n = rsa::BigUint::try_from(&keypair.public.n).map_err(|e| format!("n: {e}"))?;
    let e = rsa::BigUint::try_from(&keypair.public.e).map_err(|e| format!("e: {e}"))?;
    let d = rsa::BigUint::try_from(&keypair.private.d).map_err(|e| format!("d: {e}"))?;
    let p = rsa::BigUint::try_from(&keypair.private.p).map_err(|e| format!("p: {e}"))?;
    let q = rsa::BigUint::try_from(&keypair.private.q).map_err(|e| format!("q: {e}"))?;
    let priv_key =
        RsaPrivateKey::from_components(n, e, d, vec![p, q]).map_err(|e| format!("to rsa: {e}"))?;

    let use_sha512 = flags & SSH_AGENT_RSA_SHA2_512 != 0;
    if use_sha512 {
        let sk = SigningKey::<sha2::Sha512>::new(priv_key);
        let sig = sk.sign(msg);
        Signature::new(
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha512),
            },
            sig.to_vec(),
        )
        .map_err(|e| format!("wrap sha512: {e}"))
    } else {
        // SHA-256 flag set, or neither flag set (default).
        let _ = SSH_AGENT_RSA_SHA2_256;
        let sk = SigningKey::<sha2::Sha256>::new(priv_key);
        let sig = sk.sign(msg);
        Signature::new(
            Algorithm::Rsa {
                hash: Some(HashAlg::Sha256),
            },
            sig.to_vec(),
        )
        .map_err(|e| format!("wrap sha256: {e}"))
    }
}

// The set of identities currently served by the agent, ordered for stable
// listing and de-duplicated by public-key fingerprint when advertised.
pub(crate) struct SshAgentStore {
    identities: Vec<StoredIdentity>,
    // Bumped whenever the key set changes; invalidates in-flight confirmations.
    generation: u64,
}

impl SshAgentStore {
    pub(crate) fn new() -> Self {
        Self {
            identities: Vec::new(),
            generation: 0,
        }
    }

    // Count of currently *servable* identities — expired ones (Agent Lifetime
    // elapsed) are excluded even before the sweep prunes them, so the settings
    // panel's "keys served" reflects what clients can actually use.
    pub(crate) fn len(&self) -> usize {
        self.identities.iter().filter(|id| !id.is_expired()).count()
    }

    pub(crate) fn clear(&mut self) {
        // Dropping each StoredIdentity zeroizes its private key.
        self.generation = self.generation.wrapping_add(1);
        self.identities.clear();
    }

    // Drops (and zeroizes, via Drop) every identity whose Agent Lifetime has
    // elapsed. Returns how many were removed. Called periodically by the sweep
    // task so an expired key's secret does not linger in memory until db lock.
    pub(crate) fn prune_expired(&mut self) -> usize {
        let before = self.identities.len();
        self.identities.retain(|id| !id.is_expired());
        let removed = before - self.identities.len();
        if removed > 0 {
            self.generation = self.generation.wrapping_add(1);
        }
        removed
    }

    // Replaces every identity drawn from `db_key` with a freshly decoded slice.
    // Sources that fail to decode are logged and skipped.
    pub(crate) fn replace_db(&mut self, db_key: &str, sources: Vec<SshAgentKeySource>) {
        self.generation = self.generation.wrapping_add(1);
        self.identities.retain(|id| id.db_key != db_key);
        self.decode_into(sources);
    }

    pub(crate) fn remove_db(&mut self, db_key: &str) {
        self.generation = self.generation.wrapping_add(1);
        self.identities.retain(|id| id.db_key != db_key);
    }

    // Clears the store and rebuilds it from sources across all open databases.
    pub(crate) fn set_all(&mut self, sources: Vec<SshAgentKeySource>) {
        self.generation = self.generation.wrapping_add(1);
        self.identities.clear();
        self.decode_into(sources);
    }

    fn decode_into(&mut self, sources: Vec<SshAgentKeySource>) {
        for src in sources {
            match StoredIdentity::from_source(&src) {
                Ok(id) => {
                    log::debug!(
                        "SSH agent: loaded identity ({})",
                        id.fingerprint()
                    );
                    self.identities.push(id);
                }
                Err(e) => {
                    log::warn!(
                        "SSH agent: skipping invalid SSH Key entry: {}",
                        e
                    );
                }
            }
        }
    }

    // The identities to advertise to clients, de-duplicated by fingerprint so a
    // key present in two open databases is listed once.
    pub(crate) fn identities(&self) -> Vec<Identity> {
        let mut seen: HashMap<String, ()> = HashMap::new();
        let mut out = Vec::new();
        for id in &self.identities {
            // Expired keys are never advertised, even before the sweep prunes them.
            if id.is_expired() {
                continue;
            }
            if seen.insert(id.fingerprint(), ()).is_none() {
                out.push(Identity {
                    credential: PublicCredential::Key(id.public_key_data.clone()),
                    comment: id.comment.clone(),
                });
            }
        }
        out
    }

    // Returns the confirmation metadata for the identity matching `requested`,
    // or None if no such key is currently served (including an expired one).
    pub(crate) fn confirmation_info(&self, requested: &KeyData) -> Option<ConfirmInfo> {
        self.identities
            .iter()
            .find(|id| !id.is_expired() && &id.public_key_data == requested)
            .map(|id| ConfirmInfo {
                require_confirmation: id.require_confirmation,
                comment: id.comment.clone(),
                fingerprint: id.fingerprint(),
                generation: self.generation,
            })
    }

    // Finds the identity matching the requested public key and signs `data`.
    // The first matching identity wins when the same key is in multiple dbs.
    pub(crate) fn sign(
        &self,
        requested: &KeyData,
        data: &[u8],
        flags: u32,
        expected_generation: u64,
        confirmation_granted: bool,
    ) -> Result<Signature, AgentError> {
        if self.generation != expected_generation {
            // An in-flight confirmation must not authorize a replacement key,
            // even when the public key bytes remain identical.
            return Err(AgentError::Failure);
        }
        let id = self
            .identities
            .iter()
            .find(|id| !id.is_expired() && &id.public_key_data == requested)
            .ok_or(AgentError::Failure)?;
        if id.require_confirmation && !confirmation_granted {
            return Err(AgentError::Failure);
        }

        id.sign(data, flags).map_err(|e| {
            log::error!("SSH agent: signing failed: {}", e);
            AgentError::Failure
        })
    }
}

#[cfg(test)]
mod tests {
    use super::parse_lifetime;
    use std::time::Duration;

    // Real OpenSSH keys exercise format compatibility and the agent's
    // algorithm-specific signing. No keys are checked into the repository:
    // ssh-keygen creates disposable test fixtures under the runner temp dir.
    #[cfg(windows)]
    #[test]
    fn openssh_key_signing_roundtrips_all_supported_algorithms() {
        use super::{StoredIdentity, SSH_AGENT_RSA_SHA2_512};
        use onekeepass_core::db_service::ssh_agent::SshAgentKeySource;
        use std::fs;
        use std::process::Command;

        let message = b"OneKeePass isolated SSH agent cryptography test";
        let cases = [
            ("ed25519", None),
            ("ecdsa", Some("256")),
            ("ecdsa", Some("384")),
            ("ecdsa", Some("521")),
            ("rsa", Some("2048")),
        ];

        for (key_type, bits) in cases {
            let stem = std::env::temp_dir().join(format!(
                "onekeepass-ssh-test-{}",
                uuid::Uuid::new_v4()
            ));
            let filename = stem.to_string_lossy().into_owned();
            let mut cmd = Command::new("ssh-keygen");
            cmd.args(["-q", "-t", key_type, "-N", "", "-C", "temporary-test", "-f", &filename]);
            if let Some(b) = bits {
                cmd.args(["-b", b]);
            }
            let result = cmd.output().expect("ssh-keygen must be available on Windows CI");
            assert!(
                result.status.success(),
                "ssh-keygen {key_type} {bits:?} failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );

            let private_key_pem = fs::read_to_string(&stem).unwrap();
            let public_key = fs::read_to_string(format!("{filename}.pub")).unwrap();
            let _ = fs::remove_file(&stem);
            let _ = fs::remove_file(format!("{filename}.pub"));

            let source = SshAgentKeySource {
                db_key: "unit-test-db".into(),
                entry_uuid: uuid::Uuid::new_v4(),
                title: format!("test {key_type} {bits:?}"),
                public_key: Some(public_key),
                private_key_pem,
                passphrase: None,
                require_confirmation: false,
                agent_lifetime: None,
            };

            // The stable agent protocol parser must load the exact key
            // produced by ssh-keygen, including its public-key identity.
            let id = StoredIdentity::from_source(&source)
                .unwrap_or_else(|e| panic!("{key_type} {bits:?} parse failed: {e}"));
            assert!(!id.public_key_data.fingerprint(ssh_key::HashAlg::Sha256).to_string().is_empty());

            // Verify signatures using the independent modern SSH key verifier;
            // this checks ECDSA r/s framing as well as actual cryptographic validity.
            let modern = ssh_key_ppk::PrivateKey::from_openssh(source.private_key_pem.trim())
                .expect("modern SSH parser must accept generated keys");
            let verify_signature = |sig: &ssh_key::Signature| {
                let hash = match sig.algorithm() {
                    ssh_key::Algorithm::Rsa { hash: Some(ssh_key::HashAlg::Sha512) } => {
                        ssh_key_ppk::Algorithm::Rsa { hash: Some(ssh_key_ppk::HashAlg::Sha512) }
                    }
                    ssh_key::Algorithm::Rsa { .. } => {
                        ssh_key_ppk::Algorithm::Rsa { hash: Some(ssh_key_ppk::HashAlg::Sha256) }
                    }
                    _ => modern.algorithm(),
                };
                let modern_sig = ssh_key_ppk::Signature::new(hash, sig.as_bytes().to_vec())
                    .expect("signature must have valid SSH wire encoding");
                signature3::Verifier::verify(modern.public_key(), message, &modern_sig)
                    .unwrap_or_else(|e| panic!("{key_type} {bits:?} signature rejected: {e}"));
            };
            verify_signature(&id.sign(message, 0).expect("agent signing must succeed"));
            if key_type == "rsa" {
                verify_signature(&id.sign(message, SSH_AGENT_RSA_SHA2_512)
                    .expect("RSA SHA2-512 agent signing must succeed"));
            }
        }
    }

    #[test]
    fn lifetime_seconds() {
        // The duration picker stores whole seconds.
        assert_eq!(parse_lifetime("3600"), Ok(Some(Duration::from_secs(3600))));
        assert_eq!(parse_lifetime("900"), Ok(Some(Duration::from_secs(900))));
        assert_eq!(parse_lifetime("604800"), Ok(Some(Duration::from_secs(604_800))));
        assert_eq!(parse_lifetime("  90 "), Ok(Some(Duration::from_secs(90))));
    }

    #[test]
    fn key_store_generation_changes_on_clear_and_reload() {
        use super::SshAgentStore;
        let mut store = SshAgentStore::new();
        let initial = store.generation;
        store.clear();
        assert_ne!(store.generation, initial);
        let next = store.generation;
        store.set_all(vec![]);
        assert_ne!(store.generation, next);
        let next = store.generation;
        store.remove_db("test-db");
        assert_ne!(store.generation, next);
    }

    #[test]
    fn lifetime_none_or_invalid() {
        // Empty / zero -> no expiry.
        assert_eq!(parse_lifetime(""), Ok(None));
        assert_eq!(parse_lifetime("   "), Ok(None));
        assert_eq!(parse_lifetime("0"), Ok(None));
        // Non-numeric -> no expiry (logged).
        assert!(parse_lifetime("abc").is_err());
        assert!(parse_lifetime("30m").is_err());
        assert!(parse_lifetime("-5").is_err());
    }
}
