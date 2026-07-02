# Large-Blob Legibility (Tier A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every entry in a FIDO2 large-blob array legible — classified (keyroost note / SSH certificate / opaque RP data), decoded where recognized, capacity-metered, and exportable to a file — across the ctap layer, `keyroostctl`, and the GUI Storage view.

**Architecture:** All recognition/decoding logic lives in `keyroost-ctap` (a new `ssh_cert` module plus additions to `large_blobs`), so the CLI and GUI stay thin renderers over one classifier. Everything is read-only inspection + export; no writes change shape, no new crypto.

**Tech Stack:** Pure Rust. One new in-tree path dependency: `keyroost-ctap` → `keyroost-proto` (for the existing vendored base64 codec). No external dependencies added anywhere.

**Spec:** Tier A of `docs/superpowers/specs/2026-06-30-largeblob-storage-roadmap-design.md`.

## Global Constraints

- Work on a branch `feat/largeblob-legibility` cut from `main` (after the roadmap-doc branch has landed). Sync `origin/main` first (project rule).
- Commit with `git -c commit.gpgsign=false commit …` — the user re-signs before push (project rule; the hardware signing key is not available to agents).
- No new external dependencies. `keyroost-proto` as a path dep of `keyroost-ctap` is in-tree and allowed; nothing else changes in any Cargo.toml dependency set.
- Workspace MSRV is **1.85** (`keyroost-ctap`, `keyroostctl`); the GUI crate `keyroost` is pinned **1.92**. Don't use APIs newer than these floors in the respective crates.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` must stay clean on stable **1.96** (note: `manual_is_multiple_of` etc. are enforced).
- Existing known-answer/unit suites must stay green: run `cargo test --workspace --offline` after every task.
- RP-owned (opaque) entries are never rewritten or reinterpreted as text — display raw only. Recognition must be conservative: when parsing fails at any point, fall back to Opaque.
- The large-blob store is world-readable; keep the existing UI/CLI honesty copy intact.

## File Structure

- `crates/keyroost-ctap/src/cmd.rs` — add `maxSerializedLargeBlobArray` (getInfo key `0x0B`) to `AuthenticatorInfo`.
- `crates/keyroost-ctap/src/ssh_cert.rs` — **new**: OpenSSH certificate wire + text parsing, validity formatting, `-cert.pub` re-encoding. No I/O, no crypto verification (display-grade decode only).
- `crates/keyroost-ctap/src/large_blobs.rs` — `BlobCapacity` + `LargeBlobArray::capacity()`, `EntryKind` + `LargeBlobEntry::classify()`.
- `crates/keyroost-ctap/src/lib.rs` — one `pub mod ssh_cert;` line.
- `crates/keyroost-ctap/Cargo.toml` — add `keyroost-proto` path dep.
- `crates/keyroostctl/src/main.rs` — richer `list`/`get`, new `export` subcommand, JSON additions.
- `crates/keyroost/src/main.rs` — capacity meter, per-entry kind + parsed-cert view, per-entry Export via the existing `FileTarget` dialog plumbing.
- `crates/keyroost/src/ui/help.rs` — extend the `large_blobs` help copy.

---

### Task 1: `maxSerializedLargeBlobArray` in `AuthenticatorInfo`

**Files:**
- Modify: `crates/keyroost-ctap/src/cmd.rs` (struct at ~line 171, parse match in `parse_authenticator_info`)
- Test: same file's existing `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `AuthenticatorInfo.max_serialized_large_blob_array: Option<u64>` — read by Task 2's `capacity()`.

- [ ] **Step 1: Write the failing tests** (in `cmd.rs`'s existing tests module)

```rust
#[test]
fn parses_max_serialized_large_blob_array() {
    // CTAP2.1 authenticatorGetInfo key 0x0B.
    let v = Value::Map(vec![(Value::UInt(0x0b), Value::UInt(2048))]);
    let info = parse_authenticator_info(&v).unwrap();
    assert_eq!(info.max_serialized_large_blob_array, Some(2048));
}

#[test]
fn max_serialized_large_blob_array_defaults_to_none() {
    let v = Value::Map(vec![]);
    let info = parse_authenticator_info(&v).unwrap();
    assert_eq!(info.max_serialized_large_blob_array, None);
}
```

If the tests module doesn't already import `Value`/`parse_authenticator_info`, add `use super::*;`-style imports matching the module's existing pattern.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p keyroost-ctap --offline parses_max_serialized`
Expected: FAIL — `no field max_serialized_large_blob_array` (compile error counts as the failing state).

- [ ] **Step 3: Implement**

In `pub struct AuthenticatorInfo`, after `pub firmware_version: Option<u64>,` add:

```rust
    /// `maxSerializedLargeBlobArray` (getInfo key 0x0B): the maximum size, in
    /// bytes, of the serialized large-blob array this authenticator stores —
    /// including the 16-byte checksum trailer. Spec minimum is 1024 when the
    /// `largeBlobs` option is supported; absent on keys without large blobs.
    pub max_serialized_large_blob_array: Option<u64>,
```

In `parse_authenticator_info`'s `match key` block, add an arm (numeric order with its neighbors):

```rust
            0x0b => info.max_serialized_large_blob_array = val.as_uint(),
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p keyroost-ctap --offline`
Expected: PASS (all crate tests, including the two new ones).

- [ ] **Step 5: Commit**

```bash
git add crates/keyroost-ctap/src/cmd.rs
git -c commit.gpgsign=false commit -m "feat(ctap): parse maxSerializedLargeBlobArray from getInfo (key 0x0B)"
```

---

### Task 2: Capacity accounting on `LargeBlobArray`

**Files:**
- Modify: `crates/keyroost-ctap/src/large_blobs.rs`
- Test: same file's existing `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `AuthenticatorInfo.max_serialized_large_blob_array` (Task 1).
- Produces: `pub struct BlobCapacity { pub max_bytes: u64, pub used_bytes: u64, pub free_bytes: u64, pub entry_count: usize }` and `LargeBlobArray::capacity(&self, info: &AuthenticatorInfo) -> BlobCapacity` — used by Tasks 6 and 7.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn capacity_of_empty_array() {
    let arr = LargeBlobArray { entries: Vec::new(), raw_array: Vec::new() };
    let info = AuthenticatorInfo::default(); // no 0x0B advertised -> spec minimum
    let cap = arr.capacity(&info);
    // Empty CBOR array (0x80, 1 byte) + 16-byte checksum trailer.
    assert_eq!(cap.used_bytes, 17);
    assert_eq!(cap.max_bytes, 1024);
    assert_eq!(cap.free_bytes, 1024 - 17);
    assert_eq!(cap.entry_count, 0);
}

#[test]
fn capacity_uses_advertised_max_and_saturates() {
    let arr = LargeBlobArray {
        entries: vec![LargeBlobEntry::from_text("hello")],
        raw_array: Vec::new(),
    };
    let mut info = AuthenticatorInfo::default();
    info.max_serialized_large_blob_array = Some(4096);
    let cap = arr.capacity(&info);
    assert_eq!(cap.max_bytes, 4096);
    assert_eq!(cap.used_bytes, arr.serialize_with_checksum().len() as u64);
    assert_eq!(cap.free_bytes, cap.max_bytes - cap.used_bytes);
    assert_eq!(cap.entry_count, 1);

    // A max smaller than what's stored must not underflow.
    info.max_serialized_large_blob_array = Some(10);
    assert_eq!(arr.capacity(&info).free_bytes, 0);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p keyroost-ctap --offline capacity_`
Expected: FAIL — `no method named capacity` (compile error).

- [ ] **Step 3: Implement** (in `large_blobs.rs`, near the `LargeBlobArray` impl)

```rust
/// Spec floor for `maxSerializedLargeBlobArray` when the authenticator
/// supports large blobs but does not advertise the key (CTAP 2.1 §6.10).
const SPEC_MIN_SERIALIZED_ARRAY: u64 = 1024;

/// Space accounting for a large-blob array against an authenticator's
/// advertised (or spec-minimum) storage. Sizes count the full serialized
/// form — CBOR array plus the 16-byte checksum trailer — because that is
/// what `maxSerializedLargeBlobArray` bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobCapacity {
    pub max_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub entry_count: usize,
}

impl LargeBlobArray {
    /// Compute capacity from the entries as currently held (re-serialized,
    /// so it stays correct after local adds/edits that haven't been written).
    pub fn capacity(&self, info: &AuthenticatorInfo) -> BlobCapacity {
        let used = self.serialize_with_checksum().len() as u64;
        let max = info
            .max_serialized_large_blob_array
            .unwrap_or(SPEC_MIN_SERIALIZED_ARRAY);
        BlobCapacity {
            max_bytes: max,
            used_bytes: used,
            free_bytes: max.saturating_sub(used),
            entry_count: self.entries.len(),
        }
    }
}
```

(`AuthenticatorInfo` is already imported in this file — it's used by `max_fragment_length`.)

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p keyroost-ctap --offline`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/keyroost-ctap/src/large_blobs.rs
git -c commit.gpgsign=false commit -m "feat(ctap): large-blob capacity accounting (used/free vs maxSerializedLargeBlobArray)"
```

---

### Task 3: OpenSSH certificate wire parser (`ssh_cert.rs`)

**Files:**
- Create: `crates/keyroost-ctap/src/ssh_cert.rs`
- Modify: `crates/keyroost-ctap/src/lib.rs` (add `pub mod ssh_cert;` alongside the other modules)
- Modify: `crates/keyroost-ctap/Cargo.toml` (add the in-tree base64 source, used by tests here and by Task 4's text form):

```toml
keyroost-proto = { path = "../keyroost-proto", version = "0.7.3" }
```

- Test: `#[cfg(test)] mod tests` at the bottom of `ssh_cert.rs`

**Interfaces:**
- Produces (used by Tasks 4–7):
  - `pub struct SshCertInfo { pub key_type: String, pub serial: u64, pub cert_type: u32, pub key_id: String, pub principals: Vec<String>, pub valid_after: u64, pub valid_before: u64, pub critical_options: Vec<(String, String)>, pub extensions: Vec<String> }`
  - `pub fn parse_wire(blob: &[u8]) -> Option<SshCertInfo>`
  - `pub fn format_timestamp(secs: u64) -> String` and `pub fn format_validity(valid_after: u64, valid_before: u64) -> String`
  - `pub const CERT_TYPE_USER: u32 = 1;` / `pub const CERT_TYPE_HOST: u32 = 2;`

**Test fixture** (generated with `ssh-keygen -s <ca> -I test-key-id -n alice,bob -z 42 -V <UTC 2026-01-01..2027-01-01> -O clear -O permit-pty -O source-address=192.0.2.0/24`; throwaway keys, safe to embed):

```text
ssh-ed25519-cert-v01@openssh.com AAAAIHNzaC1lZDI1NTE5LWNlcnQtdjAxQG9wZW5zc2guY29tAAAAIFnHByOSs9oyjoM3FSMYa4CyEkl9qj7cPldTCWBGw3soAAAAIB8WYDicxYHAvQ5QE8w24ZO0pod+x5Y7Zcjdk8D3kOpZAAAAAAAAACoAAAABAAAAC3Rlc3Qta2V5LWlkAAAAEAAAAAVhbGljZQAAAANib2IAAAAAaVW5AAAAAABrNuyAAAAAJgAAAA5zb3VyY2UtYWRkcmVzcwAAABAAAAAMMTkyLjAuMi4wLzI0AAAAEgAAAApwZXJtaXQtcHR5AAAAAAAAAAAAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIOkAWA6QeBu6LNDfyV4zAgonPK7XpSmq9aFdozDaQr76AAAAUwAAAAtzc2gtZWQyNTUxOQAAAEAEeDetmfpeDeQHbXOGfLlLg9XHjJQpaXg1foE9TuNXWP3Bx3oCk4Foa8S7VkuXtK0geecTqa4WZGF9dM6VSWgI user
```

Expected decode: type `ssh-ed25519-cert-v01@openssh.com`, serial `42`, cert_type `1` (user), key_id `test-key-id`, principals `["alice", "bob"]`, valid_after `1767225600` (2026-01-01 00:00:00 UTC), valid_before `1798761600` (2027-01-01 00:00:00 UTC), critical_options `[("source-address", "192.0.2.0/24")]`, extensions `["permit-pty"]`.

- [ ] **Step 1: Write the failing tests** (bottom of the new `ssh_cert.rs`; the file must exist with at least the module docstring for the test to compile against — write the full test module first)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use keyroost_proto::codec::base64_decode;

    /// Real ed25519 user certificate produced by ssh-keygen from throwaway
    /// keys (see the Tier A plan for the exact invocation). The base64 body
    /// is the wire-format certificate blob.
    const FIXTURE_CERT_PUB: &str = "ssh-ed25519-cert-v01@openssh.com AAAAIHNzaC1lZDI1NTE5LWNlcnQtdjAxQG9wZW5zc2guY29tAAAAIFnHByOSs9oyjoM3FSMYa4CyEkl9qj7cPldTCWBGw3soAAAAIB8WYDicxYHAvQ5QE8w24ZO0pod+x5Y7Zcjdk8D3kOpZAAAAAAAAACoAAAABAAAAC3Rlc3Qta2V5LWlkAAAAEAAAAAVhbGljZQAAAANib2IAAAAAaVW5AAAAAABrNuyAAAAAJgAAAA5zb3VyY2UtYWRkcmVzcwAAABAAAAAMMTkyLjAuMi4wLzI0AAAAEgAAAApwZXJtaXQtcHR5AAAAAAAAAAAAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIOkAWA6QeBu6LNDfyV4zAgonPK7XpSmq9aFdozDaQr76AAAAUwAAAAtzc2gtZWQyNTUxOQAAAEAEeDetmfpeDeQHbXOGfLlLg9XHjJQpaXg1foE9TuNXWP3Bx3oCk4Foa8S7VkuXtK0geecTqa4WZGF9dM6VSWgI user";

    fn fixture_wire() -> Vec<u8> {
        let b64 = FIXTURE_CERT_PUB.split_ascii_whitespace().nth(1).unwrap();
        base64_decode(b64).unwrap()
    }

    #[test]
    fn parses_real_ed25519_user_cert() {
        let info = parse_wire(&fixture_wire()).unwrap();
        assert_eq!(info.key_type, "ssh-ed25519-cert-v01@openssh.com");
        assert_eq!(info.serial, 42);
        assert_eq!(info.cert_type, CERT_TYPE_USER);
        assert_eq!(info.key_id, "test-key-id");
        assert_eq!(info.principals, vec!["alice".to_string(), "bob".to_string()]);
        assert_eq!(info.valid_after, 1767225600);
        assert_eq!(info.valid_before, 1798761600);
        assert_eq!(
            info.critical_options,
            vec![("source-address".to_string(), "192.0.2.0/24".to_string())]
        );
        assert_eq!(info.extensions, vec!["permit-pty".to_string()]);
    }

    #[test]
    fn rejects_non_certificates() {
        // Not even a length-prefixed string.
        assert!(parse_wire(&[0xde, 0xad, 0xbe, 0xef]).is_none());
        // Empty input.
        assert!(parse_wire(&[]).is_none());
        // A valid-looking type string but nothing after it.
        let mut b = Vec::new();
        b.extend_from_slice(&32u32.to_be_bytes());
        b.extend_from_slice(b"ssh-ed25519-cert-v01@openssh.com");
        assert!(parse_wire(&b).is_none());
        // Truncated real cert: cut the last 10 bytes off.
        let wire = fixture_wire();
        assert!(parse_wire(&wire[..wire.len() - 10]).is_none());
        // Trailing junk after a real cert must also be rejected (strictness
        // keeps classification from matching almost-certs).
        let mut padded = fixture_wire();
        padded.push(0x00);
        assert!(parse_wire(&padded).is_none());
    }

    #[test]
    fn formats_timestamps_and_validity() {
        assert_eq!(format_timestamp(1767225600), "2026-01-01 00:00:00 UTC");
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00 UTC");
        // OpenSSH convention: the all-1s valid_before means "forever".
        assert_eq!(format_validity(0, u64::MAX), "always valid");
        assert_eq!(
            format_validity(1767225600, 1798761600),
            "2026-01-01 00:00:00 UTC to 2027-01-01 00:00:00 UTC"
        );
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p keyroost-ctap --offline ssh_cert`
Expected: FAIL — module doesn't exist yet (compile error after adding `pub mod ssh_cert;`).

- [ ] **Step 3: Implement the parser** (top of `ssh_cert.rs`, above the tests)

```rust
//! Read-only decoding of OpenSSH certificates (the `*-cert-v01@openssh.com`
//! wire format described in OpenSSH's PROTOCOL.certkeys). This exists so the
//! large-blob Storage views can *recognize and display* a certificate an SSH
//! CA workflow parked on the key — type, key id, principals, validity,
//! critical options. It deliberately does NOT verify the CA signature; that
//! is the relying server's job, not a display surface's.

/// Certificate type field values (PROTOCOL.certkeys).
pub const CERT_TYPE_USER: u32 = 1;
pub const CERT_TYPE_HOST: u32 = 2;

/// The human-relevant fields of an OpenSSH certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshCertInfo {
    /// e.g. `ssh-ed25519-cert-v01@openssh.com`.
    pub key_type: String,
    pub serial: u64,
    /// [`CERT_TYPE_USER`] or [`CERT_TYPE_HOST`].
    pub cert_type: u32,
    pub key_id: String,
    pub principals: Vec<String>,
    /// Unix seconds; 0 = no lower bound.
    pub valid_after: u64,
    /// Unix seconds; `u64::MAX` = no upper bound ("forever").
    pub valid_before: u64,
    /// `(name, value)` pairs; value is empty for flag-style options.
    pub critical_options: Vec<(String, String)>,
    /// Extension names (values are conventionally empty).
    pub extensions: Vec<String>,
}

/// Cursor over SSH wire data: big-endian integers and u32-length-prefixed
/// byte strings.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let out = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(out)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_be_bytes(b.try_into().unwrap()))
    }

    fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|b| u64::from_be_bytes(b.try_into().unwrap()))
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }

    fn string(&mut self) -> Option<String> {
        std::str::from_utf8(self.bytes()?).ok().map(str::to_owned)
    }

    fn done(&self) -> bool {
        self.pos == self.buf.len()
    }
}

/// How many wire fields the embedded public key occupies for each supported
/// certificate type (they sit between the nonce and the serial). `mpint`s and
/// `string`s share the same length-prefixed wire shape, so a count suffices.
fn pubkey_field_count(key_type: &str) -> Option<usize> {
    Some(match key_type {
        "ssh-ed25519-cert-v01@openssh.com" => 1, // pk
        "ssh-rsa-cert-v01@openssh.com" => 2,     // e, n
        "ecdsa-sha2-nistp256-cert-v01@openssh.com"
        | "ecdsa-sha2-nistp384-cert-v01@openssh.com"
        | "ecdsa-sha2-nistp521-cert-v01@openssh.com" => 2, // curve, point
        "sk-ssh-ed25519-cert-v01@openssh.com" => 2, // pk, application
        "sk-ecdsa-sha2-nistp256-cert-v01@openssh.com" => 3, // curve, point, application
        _ => return None,
    })
}

/// Parse a wire-format OpenSSH certificate. Returns `None` on anything that
/// is not a complete, well-formed certificate of a supported type — including
/// trailing bytes — so this can double as a classifier over arbitrary
/// large-blob entry bytes without false positives.
pub fn parse_wire(blob: &[u8]) -> Option<SshCertInfo> {
    let mut r = Reader::new(blob);
    let key_type = r.string()?;
    let pubkey_fields = pubkey_field_count(&key_type)?;
    let _nonce = r.bytes()?;
    for _ in 0..pubkey_fields {
        r.bytes()?;
    }
    let serial = r.u64()?;
    let cert_type = r.u32()?;
    if cert_type != CERT_TYPE_USER && cert_type != CERT_TYPE_HOST {
        return None;
    }
    let key_id = r.string()?;
    let principals = packed_strings(r.bytes()?)?;
    let valid_after = r.u64()?;
    let valid_before = r.u64()?;
    let critical_options = packed_pairs(r.bytes()?)?;
    let extensions = packed_pairs(r.bytes()?)?
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    let _reserved = r.bytes()?;
    let _signature_key = r.bytes()?;
    let _signature = r.bytes()?;
    if !r.done() {
        return None;
    }
    Some(SshCertInfo {
        key_type,
        serial,
        cert_type,
        key_id,
        principals,
        valid_after,
        valid_before,
        critical_options,
        extensions,
    })
}

/// A buffer holding zero or more concatenated wire strings (the principals
/// section).
fn packed_strings(buf: &[u8]) -> Option<Vec<String>> {
    let mut r = Reader::new(buf);
    let mut out = Vec::new();
    while !r.done() {
        out.push(r.string()?);
    }
    Some(out)
}

/// A buffer holding zero or more `(string name, string data)` tuples, where
/// `data` is either empty or itself a single wire string (the critical
/// options and extensions sections).
fn packed_pairs(buf: &[u8]) -> Option<Vec<(String, String)>> {
    let mut r = Reader::new(buf);
    let mut out = Vec::new();
    while !r.done() {
        let name = r.string()?;
        let data = r.bytes()?;
        let value = if data.is_empty() {
            String::new()
        } else {
            let mut inner = Reader::new(data);
            let v = inner.string()?;
            if !inner.done() {
                return None;
            }
            v
        };
        out.push((name, value));
    }
    Some(out)
}

/// Render a Unix timestamp as `YYYY-MM-DD HH:MM:SS UTC` without pulling in a
/// date crate. Civil-from-days per Howard Hinnant's algorithm.
pub fn format_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        y,
        m,
        d,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Human validity window. OpenSSH treats `(0, u64::MAX)` as unbounded.
pub fn format_validity(valid_after: u64, valid_before: u64) -> String {
    if valid_after == 0 && valid_before == u64::MAX {
        return "always valid".to_string();
    }
    let from = if valid_after == 0 {
        "beginning of time".to_string()
    } else {
        format_timestamp(valid_after)
    };
    let to = if valid_before == u64::MAX {
        "forever".to_string()
    } else {
        format_timestamp(valid_before)
    };
    format!("{from} to {to}")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (y + i64::from(m <= 2), m, d)
}
```

Add to `crates/keyroost-ctap/src/lib.rs` (alphabetical with the other `pub mod` lines):

```rust
pub mod ssh_cert;
```

And the Cargo.toml dep shown in **Files** above.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p keyroost-ctap --offline ssh_cert`
Expected: PASS (3 tests). Then `cargo test --workspace --offline` — everything green.

- [ ] **Step 5: Commit**

```bash
git add crates/keyroost-ctap/src/ssh_cert.rs crates/keyroost-ctap/src/lib.rs crates/keyroost-ctap/Cargo.toml Cargo.lock
git -c commit.gpgsign=false commit -m "feat(ctap): decode OpenSSH certificates for large-blob legibility"
```

---

### Task 4: Text-form certificates and `-cert.pub` re-encoding

**Files:**
- Modify: `crates/keyroost-ctap/src/ssh_cert.rs`
- Test: same file's tests module

**Interfaces:**
- Consumes: `parse_wire`, `SshCertInfo` (Task 3); `keyroost_proto::codec::{base64_decode, base64_encode}`.
- Produces (used by Tasks 5–7):
  - `pub fn parse_text(s: &str) -> Option<(SshCertInfo, Vec<u8>)>` — parses a `-cert.pub`-style line, returning the decoded info plus the raw wire blob.
  - `pub fn to_cert_pub(wire: &[u8]) -> Option<String>` — re-encodes a wire cert as a one-line `-cert.pub` file body (trailing newline included).

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn parses_text_form_and_roundtrips_to_cert_pub() {
        let (info, wire) = parse_text(FIXTURE_CERT_PUB).unwrap();
        assert_eq!(info.key_id, "test-key-id");
        assert_eq!(wire, fixture_wire());

        // Re-encoding the wire form yields a line OpenSSH accepts: same type,
        // same base64 body (comment is not preserved — it isn't part of the
        // certificate).
        let line = to_cert_pub(&wire).unwrap();
        let mut parts = line.split_ascii_whitespace();
        assert_eq!(parts.next(), Some("ssh-ed25519-cert-v01@openssh.com"));
        assert_eq!(
            parts.next(),
            FIXTURE_CERT_PUB.split_ascii_whitespace().nth(1)
        );
        assert!(line.ends_with('\n'));
        // And the re-encoded line parses back.
        assert!(parse_text(&line).is_some());
    }

    #[test]
    fn text_form_rejects_mismatch_and_garbage() {
        // Declared type must match the type inside the blob.
        let b64 = FIXTURE_CERT_PUB.split_ascii_whitespace().nth(1).unwrap();
        let lied = format!("ssh-rsa-cert-v01@openssh.com {b64}");
        assert!(parse_text(&lied).is_none());
        // Ordinary public keys (not certs) are not recognized.
        assert!(parse_text("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIB8WYDicxYHAvQ5QE8w24ZO0pod+x5Y7Zcjdk8D3kOpZ user").is_none());
        // Not base64 / not a cert at all.
        assert!(parse_text("hello world").is_none());
        assert!(parse_text("").is_none());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p keyroost-ctap --offline ssh_cert`
Expected: FAIL — `parse_text` not found (compile error).

- [ ] **Step 3: Implement** (in `ssh_cert.rs`, after `parse_wire`)

```rust
use keyroost_proto::codec::{base64_decode, base64_encode};

/// Parse the text form of a certificate — the single `-cert.pub` line
/// (`<type> <base64> [comment]`). Returns the decoded fields plus the raw
/// wire blob so callers can export or re-store the canonical bytes. The
/// declared type must match the type embedded in the blob.
pub fn parse_text(s: &str) -> Option<(SshCertInfo, Vec<u8>)> {
    let mut parts = s.split_ascii_whitespace();
    let declared = parts.next()?;
    if !declared.ends_with("-cert-v01@openssh.com") {
        return None;
    }
    let wire = base64_decode(parts.next()?).ok()?;
    let info = parse_wire(&wire)?;
    if info.key_type != declared {
        return None;
    }
    Some((info, wire))
}

/// Re-encode a wire-format certificate as the body of a `-cert.pub` file:
/// `<type> <base64>\n`. Returns `None` if the bytes are not a certificate.
pub fn to_cert_pub(wire: &[u8]) -> Option<String> {
    let info = parse_wire(wire)?;
    Some(format!("{} {}\n", info.key_type, base64_encode(wire)))
}
```

Move the tests' `use keyroost_proto::codec::base64_decode;` up into this module-level `use` if clippy flags the duplication.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p keyroost-ctap --offline ssh_cert` — PASS (5 tests), then `cargo clippy -p keyroost-ctap --all-targets --offline -- -D warnings` — clean.

- [ ] **Step 5: Commit**

```bash
git add crates/keyroost-ctap/src/ssh_cert.rs
git -c commit.gpgsign=false commit -m "feat(ctap): parse cert.pub text form and re-encode wire certs for export"
```

---

### Task 5: Entry classification (`EntryKind` + `classify()`)

**Files:**
- Modify: `crates/keyroost-ctap/src/large_blobs.rs`
- Test: same file's tests module

**Interfaces:**
- Consumes: `ssh_cert::{parse_wire, parse_text, SshCertInfo}` (Tasks 3–4); `KR_NOTE_MAGIC` / `as_text()` (existing).
- Produces (used by Tasks 6–7):

```rust
pub enum EntryKind {
    /// keyroost plaintext note (magic-prefixed); payload is the note text.
    Note(String),
    /// A recognized OpenSSH certificate; `wire` is the canonical binary blob
    /// (decoded from base64 when the entry stored the text form).
    SshCert { info: ssh_cert::SshCertInfo, wire: Vec<u8> },
    /// Anything else — treated as RP-owned AEAD data, shown raw only.
    Opaque,
}
impl LargeBlobEntry { pub fn classify(&self) -> EntryKind }
```

- [ ] **Step 1: Write the failing tests**

The certificate fixture currently lives inside `ssh_cert`'s private tests. To share it, in `ssh_cert.rs` move the constant out of `mod tests` into a test-support module:

```rust
/// Test fixture shared with large_blobs' classification tests.
#[cfg(test)]
pub(crate) mod tests_fixture {
    pub const FIXTURE_CERT_PUB: &str = "<the same fixture line as Task 3>";
}
```

…and have `ssh_cert`'s own tests `use super::tests_fixture::FIXTURE_CERT_PUB;` instead of a local const. Then in `large_blobs.rs`:

```rust
    #[test]
    fn classify_note_ssh_cert_and_opaque() {
        use crate::ssh_cert::tests_fixture::FIXTURE_CERT_PUB;
        use keyroost_proto::codec::base64_decode;

        // Note wins (even if a note happens to contain a cert line).
        let note = LargeBlobEntry::from_text("hello");
        assert!(matches!(note.classify(), EntryKind::Note(t) if t == "hello"));

        // Wire-format certificate.
        let wire = base64_decode(FIXTURE_CERT_PUB.split_ascii_whitespace().nth(1).unwrap()).unwrap();
        let wire_entry = LargeBlobEntry { ciphertext: wire.clone(), nonce: vec![0; 12], orig_size: wire.len() as u64 };
        match wire_entry.classify() {
            EntryKind::SshCert { info, wire: w } => {
                assert_eq!(info.key_id, "test-key-id");
                assert_eq!(w, wire);
            }
            other => panic!("expected SshCert, got {other:?}"),
        }

        // Text-form certificate (a -cert.pub file dropped in verbatim).
        let text_entry = LargeBlobEntry {
            ciphertext: FIXTURE_CERT_PUB.as_bytes().to_vec(),
            nonce: vec![0; 12],
            orig_size: FIXTURE_CERT_PUB.len() as u64,
        };
        match text_entry.classify() {
            EntryKind::SshCert { info, wire: w } => {
                assert_eq!(info.serial, 42);
                assert_eq!(w, wire); // canonical wire bytes, not the text
            }
            other => panic!("expected SshCert, got {other:?}"),
        }

        // Random RP bytes stay opaque.
        let rp = LargeBlobEntry { ciphertext: vec![0xde, 0xad, 0xbe, 0xef], nonce: vec![1; 12], orig_size: 4 };
        assert!(matches!(rp.classify(), EntryKind::Opaque));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p keyroost-ctap --offline classify_`
Expected: FAIL — `EntryKind` not found (compile error).

- [ ] **Step 3: Implement** (in `large_blobs.rs`, after the `LargeBlobEntry` impl; add `use crate::ssh_cert;` at the top with the existing imports)

```rust
/// What a large-blob entry *is*, as far as keyroost can honestly tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// keyroost plaintext note (magic-prefixed).
    Note(String),
    /// A recognized OpenSSH certificate. `wire` holds the canonical binary
    /// certificate (decoded from base64 when the entry stored the text form),
    /// ready for export as a `-cert.pub`.
    SshCert {
        info: ssh_cert::SshCertInfo,
        wire: Vec<u8>,
    },
    /// Anything unrecognized — treated as RP-owned AEAD data: shown raw,
    /// never reinterpreted, never rewritten.
    Opaque,
}

impl LargeBlobEntry {
    /// Classify this entry. Conservative by construction: the keyroost note
    /// magic wins outright, certificate recognition requires a complete,
    /// well-formed parse, and everything else is opaque.
    pub fn classify(&self) -> EntryKind {
        if let Some(text) = self.as_text() {
            return EntryKind::Note(text);
        }
        if let Some(info) = ssh_cert::parse_wire(&self.ciphertext) {
            return EntryKind::SshCert {
                info,
                wire: self.ciphertext.clone(),
            };
        }
        if let Ok(s) = std::str::from_utf8(&self.ciphertext) {
            if let Some((info, wire)) = ssh_cert::parse_text(s.trim()) {
                return EntryKind::SshCert { info, wire };
            }
        }
        EntryKind::Opaque
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p keyroost-ctap --offline` — PASS, and `cargo clippy -p keyroost-ctap --all-targets --offline -- -D warnings` — clean.

- [ ] **Step 5: Commit**

```bash
git add crates/keyroost-ctap/src/large_blobs.rs crates/keyroost-ctap/src/ssh_cert.rs
git -c commit.gpgsign=false commit -m "feat(ctap): classify large-blob entries (note / ssh-cert / opaque)"
```

---

### Task 6: CLI — kind-aware `list`/`get`, capacity line, `export` subcommand

**Files:**
- Modify: `crates/keyroostctl/src/main.rs`:
  - `json_out` module (~line 246): extend list/get JSON, add capacity + cert structs
  - `enum LargeBlobCmd` (~line 1614): add `Export`
  - dispatcher `run_fido_large_blob` (~line 4963)
  - `large_blob_list_json` / `run_fido_large_blob_list` / `run_fido_large_blob_get` (~lines 5031–5115)
- Test: pure helpers in `main.rs`'s existing `#[cfg(test)]` module if present; otherwise verification is build + clippy + the JSON snapshot below (the functions are hardware-bound).

**Interfaces:**
- Consumes: `EntryKind`/`classify()` (Task 5), `capacity()` (Task 2), `ssh_cert::{format_validity, to_cert_pub, CERT_TYPE_USER}` (Tasks 3–4).
- Produces: `keyroostctl fido large-blob export <INDEX> <FILE> [--as-cert]`; JSON additions are strictly additive (existing `is_note`/`text`/`size` fields keep their meaning).

- [ ] **Step 1: JSON shapes** — in the `json_out` module, extend:

```rust
    /// `keyroostctl fido large-blob --json list` — one entry per stored blob.
    #[derive(Serialize)]
    pub struct FidoLargeBlobListJson {
        pub entries: Vec<FidoLargeBlobEntryJson>,
        pub capacity: FidoLargeBlobCapacityJson,
    }

    /// Space accounting for the whole array (serialized form incl. checksum).
    #[derive(Serialize)]
    pub struct FidoLargeBlobCapacityJson {
        pub max_bytes: u64,
        pub used_bytes: u64,
        pub free_bytes: u64,
    }

    /// Decoded fields of a recognized OpenSSH certificate entry.
    #[derive(Serialize)]
    pub struct FidoLargeBlobSshCertJson {
        pub key_type: String,
        pub serial: u64,
        /// "user" or "host".
        pub cert_type: &'static str,
        pub key_id: String,
        pub principals: Vec<String>,
        pub valid_after: u64,
        pub valid_before: u64,
        /// Human validity window, e.g. "2026-01-01 00:00:00 UTC to …".
        pub validity: String,
        /// "name=value" (or bare "name") per critical option.
        pub critical_options: Vec<String>,
        pub extensions: Vec<String>,
    }
```

and add to **both** `FidoLargeBlobEntryJson` and `FidoLargeBlobGetJson`:

```rust
        /// Entry classification: "note", "ssh-cert", or "opaque".
        pub kind: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub ssh_cert: Option<FidoLargeBlobSshCertJson>,
```

- [ ] **Step 2: Shared shaping helpers** — near `large_blob_list_json`:

```rust
/// Classification results shaped for both the human and JSON views.
fn large_blob_kind(
    entry: &keyroost_ctap::large_blobs::LargeBlobEntry,
) -> (
    &'static str,
    Option<json_out::FidoLargeBlobSshCertJson>,
    keyroost_ctap::large_blobs::EntryKind,
) {
    use keyroost_ctap::large_blobs::EntryKind;
    let kind = entry.classify();
    match &kind {
        EntryKind::Note(_) => ("note", None, kind),
        EntryKind::Opaque => ("opaque", None, kind),
        EntryKind::SshCert { info, .. } => {
            let cert = json_out::FidoLargeBlobSshCertJson {
                key_type: info.key_type.clone(),
                serial: info.serial,
                cert_type: if info.cert_type == keyroost_ctap::ssh_cert::CERT_TYPE_USER {
                    "user"
                } else {
                    "host"
                },
                key_id: info.key_id.clone(),
                principals: info.principals.clone(),
                valid_after: info.valid_after,
                valid_before: info.valid_before,
                validity: keyroost_ctap::ssh_cert::format_validity(
                    info.valid_after,
                    info.valid_before,
                ),
                critical_options: info
                    .critical_options
                    .iter()
                    .map(|(n, v)| {
                        if v.is_empty() {
                            n.clone()
                        } else {
                            format!("{n}={v}")
                        }
                    })
                    .collect(),
                extensions: info.extensions.clone(),
            };
            ("ssh-cert", Some(cert), kind)
        }
    }
}
```

Replace `large_blob_list_json` with (and adjust its one call site; `AuthenticatorInfo`'s path is whatever `open_and_read_large_blobs`'s signature already names it):

```rust
/// Shape a parsed large-blob array into the JSON `list` view.
fn large_blob_list_json(
    array: &keyroost_ctap::large_blobs::LargeBlobArray,
    info: &keyroost_ctap::cmd::AuthenticatorInfo,
) -> json_out::FidoLargeBlobListJson {
    let entries = array
        .entries
        .iter()
        .enumerate()
        .map(|(index, e)| {
            let (kind, ssh_cert, _) = large_blob_kind(e);
            json_out::FidoLargeBlobEntryJson {
                index,
                size: e.orig_size,
                is_note: e.is_kr_note(),
                text: e.as_text(),
                kind,
                ssh_cert,
            }
        })
        .collect();
    let cap = array.capacity(info);
    json_out::FidoLargeBlobListJson {
        entries,
        capacity: json_out::FidoLargeBlobCapacityJson {
            max_bytes: cap.max_bytes,
            used_bytes: cap.used_bytes,
            free_bytes: cap.free_bytes,
        },
    }
}
```

In `run_fido_large_blob_get`, fill the two new `FidoLargeBlobGetJson` fields the same way (`let (kind, ssh_cert, _) = large_blob_kind(entry);`). Update `run_fido_large_blob_list`/`run_fido_large_blob_get` to bind `info` instead of `_info` from `open_and_read_large_blobs`.

- [ ] **Step 3: Human views** — in `run_fido_large_blob_list`, replace the per-entry `if let Some(text) = e.as_text()` branching with a match on `large_blob_kind(e)`:

```rust
    for (i, e) in array.entries.iter().enumerate() {
        use keyroost_ctap::large_blobs::EntryKind;
        match e.classify() {
            EntryKind::Note(text) => {
                println!("[{}] {} bytes  note      {}", i, e.orig_size, preview_note(&text))
            }
            EntryKind::SshCert { info, .. } => println!(
                "[{}] {} bytes  ssh-cert  {} ({})",
                i,
                e.orig_size,
                info.key_id,
                info.principals.join(",")
            ),
            EntryKind::Opaque => println!(
                "[{}] {} bytes  opaque    {}",
                i,
                e.orig_size,
                preview_opaque(&e.ciphertext)
            ),
        }
    }
    let cap = array.capacity(&info);
    println!();
    println!(
        "Capacity: {} of {} bytes used, {} free",
        cap.used_bytes, cap.max_bytes, cap.free_bytes
    );
```

(keep the existing "(large-blob array is empty)" early-out, but print the capacity line in that case too — free space is exactly what an empty-store user wants to know).

In `run_fido_large_blob_get`, add an `EntryKind::SshCert` arm before the opaque fallthrough:

```rust
        EntryKind::SshCert { info, .. } => {
            println!("Entry {}: OpenSSH certificate, {} bytes", index, entry.orig_size);
            println!("  Type:        {} ({})", info.key_type,
                if info.cert_type == keyroost_ctap::ssh_cert::CERT_TYPE_USER { "user" } else { "host" });
            println!("  Key ID:      {}", info.key_id);
            println!("  Serial:      {}", info.serial);
            println!("  Principals:  {}", if info.principals.is_empty() { "(any)".to_string() } else { info.principals.join(", ") });
            println!("  Valid:       {}", keyroost_ctap::ssh_cert::format_validity(info.valid_after, info.valid_before));
            for (n, v) in &info.critical_options {
                if v.is_empty() { println!("  Critical:    {n}"); } else { println!("  Critical:    {n}={v}"); }
            }
            for ext in &info.extensions {
                println!("  Extension:   {ext}");
            }
            println!("\nExport with: keyroostctl fido large-blob export {index} <FILE> --as-cert");
        }
```

- [ ] **Step 4: `export` subcommand** — add to `enum LargeBlobCmd` (after `Get`):

```rust
    /// Save one entry's bytes to a file (read-only; no PIN needed).
    ///
    /// By default writes the entry's raw stored bytes. With --as-cert, a
    /// recognized OpenSSH certificate entry is written as a `-cert.pub` text
    /// line instead (the format `ssh` and `ssh-keygen` consume).
    Export {
        /// Zero-based entry index as printed by `large-blob list`.
        index: usize,
        /// Destination file (overwritten if it exists).
        output: std::path::PathBuf,
        /// Write a recognized SSH certificate in `-cert.pub` text form.
        #[arg(long)]
        as_cert: bool,
        #[arg(long, value_name = "PATH")]
        path: Option<std::path::PathBuf>,
    },
```

dispatcher arm in `run_fido_large_blob`:

```rust
        LargeBlobCmd::Export { index, output, as_cert, path } => {
            run_fido_large_blob_export(path.as_deref(), *index, output, *as_cert)
        }
```

handler (next to the other `run_fido_large_blob_*` functions):

```rust
fn run_fido_large_blob_export(
    path: Option<&std::path::Path>,
    index: usize,
    output: &std::path::Path,
    as_cert: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use keyroost_ctap::large_blobs::EntryKind;
    let (_dev, _info, array) = open_and_read_large_blobs(path)?;
    let entry = array
        .entries
        .get(index)
        .ok_or_else(|| large_blob_bad_index(index, array.entries.len()))?;
    let bytes: Vec<u8> = if as_cert {
        match entry.classify() {
            EntryKind::SshCert { wire, .. } => keyroost_ctap::ssh_cert::to_cert_pub(&wire)
                .expect("classified cert must re-encode")
                .into_bytes(),
            _ => return Err(format!(
                "entry {index} is not a recognized SSH certificate; drop --as-cert to export raw bytes"
            )
            .into()),
        }
    } else {
        entry.ciphertext.clone()
    };
    std::fs::write(output, &bytes)?;
    println!("Wrote {} bytes to {}", bytes.len(), output.display());
    Ok(())
}
```

- [ ] **Step 5: Verify**

Run: `cargo clippy -p keyroostctl --all-targets --offline -- -D warnings` — clean; `cargo test --workspace --offline` — green; `cargo run -p keyroostctl -- fido large-blob --help` — shows `export`.
Hardware smoke (user has test keys): `keyroostctl fido large-blob list` on a key with a note shows the capacity line; `add` a note, `export 0 /tmp/note.bin`, confirm bytes are `KR1\0hello…`.

- [ ] **Step 6: Commit**

```bash
git add crates/keyroostctl/src/main.rs
git -c commit.gpgsign=false commit -m "feat(cli): kind-aware large-blob list/get, capacity line, and export subcommand"
```

---

### Task 7: GUI — capacity meter, classification, cert view, export

**Files:**
- Modify: `crates/keyroost/src/main.rs`:
  - `SecurityKeys`-ish state near `large_blobs: Option<…>` (~line 154): add `lb_capacity` and `lb_export_idx`
  - `FileTarget` enum (~line 1297) + `drain_file_dialogs` (~line 1367)
  - `render_large_blobs` (~line 8176) and every load/re-read closure that assigns `security_keys.large_blobs` (`load_large_blobs`, `add_large_blob_note` ~7861, `edit_large_blob_note` ~7929, the delete and clear flows)
- Modify: `crates/keyroost/src/ui/help.rs` — the `"large_blobs"` help entry

**Interfaces:**
- Consumes: `BlobCapacity`/`capacity()` (Task 2), `EntryKind`/`classify()` (Task 5), `ssh_cert::{format_validity, to_cert_pub, CERT_TYPE_USER}` (Tasks 3–4).
- Produces: no programmatic interface — GUI surfaces only.

- [ ] **Step 1: State + capacity plumbing**

Next to `large_blobs: Option<keyroost_ctap::large_blobs::LargeBlobArray>` add:

```rust
    /// Capacity snapshot computed at load/reload time (needs the device's
    /// getInfo, which only the worker thread holds).
    lb_capacity: Option<keyroost_ctap::large_blobs::BlobCapacity>,
    /// Entry index awaiting an export-dialog result.
    lb_export_idx: Option<usize>,
```

In every worker closure that ends with `large_blobs::read(&mut dev, &info)` and assigns the result into `app.security_keys.large_blobs`, also compute `let cap = array.capacity(&info);` inside the closure (where `info` is in scope) and assign `app.security_keys.lb_capacity = Some(cap);` alongside the array. There are five such sites: initial load, add, edit, delete, clear. Where the array is cleared/reset (`security_keys.large_blobs = None` at ~line 4159), also set `lb_capacity = None`.

- [ ] **Step 2: Capacity meter** — in `render_large_blobs`, directly under the existing world-readable caption label:

```rust
        if let Some(cap) = self.security_keys.lb_capacity {
            ui.add_space(8.0);
            let frac = if cap.max_bytes == 0 {
                0.0
            } else {
                (cap.used_bytes as f32 / cap.max_bytes as f32).clamp(0.0, 1.0)
            };
            ui.horizontal(|ui| {
                ui.add(
                    egui::ProgressBar::new(frac)
                        .desired_width(160.0)
                        .desired_height(8.0),
                );
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{} of {} bytes used \u{00b7} {} free \u{00b7} {} {}",
                        cap.used_bytes,
                        cap.max_bytes,
                        cap.free_bytes,
                        cap.entry_count,
                        if cap.entry_count == 1 { "entry" } else { "entries" },
                    ))
                    .font(theme::f_reg(11.5))
                    .color(p.txt2),
                );
            });
        }
```

- [ ] **Step 3: Classification in the entry rows**

In the entry list inside `render_large_blobs`, the rows currently branch on `as_text()` (note vs opaque hex). Rework that branch to `match entry.classify()`:
- `Note(text)` — existing note rendering, unchanged.
- `Opaque` — existing hex/ASCII rendering, unchanged.
- `SshCert { info, .. }` — a labeled parsed view in the entry's expanded body (keep whatever badge/label style the row header uses, with text `ssh-cert`):

```rust
                egui::Grid::new(format!("lb_cert_{i}"))
                    .num_columns(2)
                    .spacing([12.0, 2.0])
                    .show(ui, |ui| {
                        let row = |ui: &mut egui::Ui, k: &str, v: String| {
                            ui.label(egui::RichText::new(k).font(theme::f_reg(11.5)).color(p.txt3));
                            ui.label(egui::RichText::new(v).font(theme::f_reg(11.5)).color(p.txt));
                            ui.end_row();
                        };
                        let kind = if info.cert_type == keyroost_ctap::ssh_cert::CERT_TYPE_USER {
                            "user"
                        } else {
                            "host"
                        };
                        row(ui, "Type", format!("{} ({kind})", info.key_type));
                        row(ui, "Key ID", info.key_id.clone());
                        row(ui, "Serial", info.serial.to_string());
                        row(
                            ui,
                            "Principals",
                            if info.principals.is_empty() {
                                "(any)".to_string()
                            } else {
                                info.principals.join(", ")
                            },
                        );
                        row(
                            ui,
                            "Valid",
                            keyroost_ctap::ssh_cert::format_validity(info.valid_after, info.valid_before),
                        );
                        for (n, v) in &info.critical_options {
                            row(ui, "Critical", if v.is_empty() { n.clone() } else { format!("{n}={v}") });
                        }
                        for ext in &info.extensions {
                            row(ui, "Extension", ext.clone());
                        }
                    });
```

(Adapt the closure-vs-helper shape to the file's existing grid helpers if one fits better — `mds_cell` exists for a different view; don't force it.)

- [ ] **Step 4: Export button + dialog plumbing**

Add a `FileTarget` variant and drain arm. In the enum:

```rust
    /// Storage tab "Export…" destination for a large-blob entry
    /// (`lb_export_idx` holds which entry).
    LbExport,
```

In each entry row's action area (next to the existing per-entry buttons), always available (export is read-only):

```rust
                if theme::button(ui, p, BtnKind::Ghost, "Export\u{2026}").clicked() {
                    self.security_keys.lb_export_idx = Some(i);
                    let is_cert = matches!(
                        entry.classify(),
                        keyroost_ctap::large_blobs::EntryKind::SshCert { .. }
                    );
                    let default_name = if is_cert {
                        format!("entry-{i}-cert.pub")
                    } else {
                        format!("large-blob-entry-{i}.bin")
                    };
                    self.spawn_file_dialog(
                        FileTarget::LbExport,
                        true,
                        &[("All files", &["*"])],
                        Some(&default_name),
                    );
                }
```

In `drain_file_dialogs`, the existing arms assign the path string to a text field; `LbExport` instead performs the write immediately (the array is already in memory; no device I/O on the UI thread):

```rust
                            FileTarget::LbExport => {
                                if let (Some(idx), Some(arr)) = (
                                    self.security_keys.lb_export_idx.take(),
                                    self.security_keys.large_blobs.as_ref(),
                                ) {
                                    if let Some(entry) = arr.entries.get(idx) {
                                        use keyroost_ctap::large_blobs::EntryKind;
                                        let bytes = match entry.classify() {
                                            EntryKind::SshCert { wire, .. } => {
                                                keyroost_ctap::ssh_cert::to_cert_pub(&wire)
                                                    .expect("classified cert must re-encode")
                                                    .into_bytes()
                                            }
                                            _ => entry.ciphertext.clone(),
                                        };
                                        self.security_keys.lb_status =
                                            Some(match std::fs::write(&path, &bytes) {
                                                Ok(()) => format!(
                                                    "Exported entry {idx} ({} bytes) to {text}",
                                                    bytes.len()
                                                ),
                                                Err(e) => format!("Export failed: {e}"),
                                            });
                                    }
                                }
                            }
```

(The other arms use `text`; this arm needs the `path: PathBuf` too — bind both from the dialog result, mirroring how `text` is currently derived from `path`.)

- [ ] **Step 5: Help copy** — in `crates/keyroost/src/ui/help.rs`, extend the `"large_blobs"` entry's text with one sentence:

```text
keyroost recognizes its own notes and OpenSSH certificates and shows a capacity meter; anything else is relying-party data, displayed raw and never modified. Any entry can be exported to a file.
```

- [ ] **Step 6: Verify**

Run: `cargo clippy -p keyroost --all-targets --offline -- -D warnings` — clean; `cargo test --workspace --offline` — green; then `cargo build --release -p keyroost` (**required** — the user launches the GUI via a PATH symlink into `target/release`, so a debug-only build looks like "nothing changed").
Hardware smoke with a test key: Storage tab shows the meter; add a note → meter shrinks by the note's size; Export a note entry → file starts `KR1\0`; store the fixture cert (`keyroostctl fido large-blob add "$(cat user_key-cert.pub)" --pin-stdin`) → row shows ssh-cert with key ID `test-key-id`, principals alice, bob, validity 2026-01-01…2027-01-01; Export… with the `-cert.pub` default name → `ssh-keygen -L -f <exported>` prints the certificate.

- [ ] **Step 7: Commit**

```bash
git add crates/keyroost/src/main.rs crates/keyroost/src/ui/help.rs
git -c commit.gpgsign=false commit -m "feat(gui): storage capacity meter, entry classification, ssh-cert view, export"
```

---

## Completion checklist

- [ ] `cargo test --workspace --offline` green
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo clippy -p keyroost --features qr --all-targets --locked -- -D warnings` clean (mirror CI)
- [ ] `cargo build --release -p keyroost` rebuilt (GUI symlink freshness)
- [ ] Hardware smoke on a test key per Tasks 6–7
- [ ] Spec check against Tier A: entry recognition (note / encrypted-note*/ opaque / SSH cert), views (hex exists + parsed + capacity meter), export — *the encrypted-note magic doesn't exist until C1; classification for it lands with C1 by design
- [ ] Branch ready for user re-sign + PR/land per repo flow
