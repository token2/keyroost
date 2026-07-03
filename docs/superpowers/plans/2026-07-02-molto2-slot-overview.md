# Molto2 Slot Overview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface each Molto2 profile slot's public block (plaintext title, occupancy, TOTP config) in the CLI and GUI, add title-only editing in the GUI, and add keyless per-slot seed deletion — all built on the hardware-verified `0x41 P2=profile` read and `0xE6` delete.

**Architecture:** The new APDU builders and the strict envelope parser live in `keyroost-proto` (the only crate with a test seam — all byte-level tests go there). `keyroost-transport` adds two thin `Session` methods, one of which special-cases the benign `6A 83` via the existing `transmit_raw` escape hatch. The CLI adds `molto slots` / `molto delete` and a read mode for `molto title`, all on the existing no-auth handler path. The GUI folds a 100-slot public-block sweep into the existing `open_molto` job and refreshes single slots after writes.

**Tech Stack:** Pure Rust; no new dependencies anywhere. clap derive (CLI), eframe/egui (GUI), existing `keyroost_proto::apdu` primitives.

**Spec:** `docs/superpowers/specs/2026-07-03-molto2-slot-overview-design.md` (approved).

## Global Constraints

- Work on the existing branch `feat/molto2-slot-overview` (it already carries the spec commits). Sync `origin/main` first (project rule).
- Commit with `git -c commit.gpgsign=false commit …` — the user re-signs before push (project rule; the hardware signing key is not available to agents).
- **No new external dependencies.** No Cargo.toml dependency-set changes at all in this plan.
- Workspace MSRV is **1.85**; the GUI crate `keyroost` is pinned **1.92**. Don't use APIs newer than these floors in the respective crates.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` must stay clean.
- Run `cargo test --workspace --offline` after every task; the existing known-answer suite must stay green.
- Wire profile indexing stays **0-based** everywhere (spec open-item 2); UIs keep their existing `Slot NN` / `#N` labels.
- The two 4-byte time fields have unconfirmed semantics (spec open-item 1): **never display them in the CLI table or GUI**; expose them only as raw `u32` (parsed big-endian, matching the protocol's stated BE convention) in `--json` output and doc comments.
- Honesty copy is a spec requirement: both UIs must state that titles and occupancy are readable by anyone holding the token, without a key.
- Hardware steps (Task 8 only) need the Molto2 on a **direct USB port** (not the dock — libccid init-timeout otherwise). Slot **99** is the sanctioned test slot. All new commands in this plan are keyless; `molto delete` is destructive but slot 99 is disposable.
- Only `docs/PROTOCOL.md` and `TODO-v0.7.5.md` get documentation edits (repo convention: no new doc files).

## File Structure

- `crates/keyroost-proto/src/commands.rs` — add `read_public_data()`, `delete_seed()`, `ProfilePublicData`, `PublicDataError`, `parse_public_data()`.
- `crates/keyroost-proto/src/lib.rs` — extend the `pub use commands::{…}` re-export list.
- `crates/keyroost-proto/tests/public_data_kat.rs` — **new**: known-answer tests for the two builders and the parser (captured-layout vectors + strict-envelope rejections).
- `crates/keyroost-transport/src/lib.rs` — `Session::read_public_data()`, `Session::delete_seed()`, `SeedDeleteOutcome`, one new `TransportError` variant.
- `crates/keyroostctl/src/main.rs` — `MoltoCmd::Slots`, `MoltoCmd::Delete`, optional-title read mode, `json_out::MoltoSlotJson`, `molto_algo_label()`.
- `crates/keyroost/src/main.rs` — `slot_meta` state + sweep in `open_molto`, slot-list titles/occupancy badges, `apply_title_only()`, `delete_seed_selected()`, `molto_delete_confirm`, honesty copy.
- `docs/PROTOCOL.md` — `0x41 P2=profile` and `0xE6` documentation; rewrite Known-unknowns item 1; `6A83` status-word row.
- `TODO-v0.7.5.md` — replace the stale titles section with a pointer to the spec.

## Key existing code facts (verified by survey — trust these anchors)

- `Command { label: &'static str, apdu: Vec<u8> }` at `crates/keyroost-proto/src/commands.rs:66-71`. Builders return it pre-serialized; there is no separate serialize step.
- APDU helpers in `crates/keyroost-proto/src/apdu.rs`: `build_apdu(cla, ins, p1, p2, data)` (Case-3), `build_apdu_get(cla, ins, p1, p2, le)` (Case-2), `CLA_PLAIN = 0x80`.
- The transport session type is **`Session`** (NOT "Molto2Session"), `crates/keyroost-transport/src/lib.rs:248`. Its `transmit()` (private, lib.rs:319) strips the status word and errors on anything but `9000`/`9060`; `transmit_raw()` (**pub**, lib.rs:358) returns `(Vec<u8>, u8, u8)` without status checking.
- Status-word predicates live in keyroost-proto `commands.rs:251-271`: `sw_ok`, `sw_completed`, `sw_auth_failed`.
- CLI: clap derive; `MoltoCmd` enum at `crates/keyroostctl/src/main.rs:1296-1458`; dispatch fn `run_molto` at main.rs:2251 — no-auth commands (`Info` at ~2286, `Reset` at ~2305) are handled with early `if let … return` blocks **before** the shared authenticate block at ~2359. Global `--json` via `json_output()` (main.rs:34) + `emit_json` (main.rs:38) + shapes in `mod json_out` (main.rs:45+). Control-char flattener: `sanitize_cert_field(&str) -> String` at main.rs:5484.
- GUI: everything on the `App` struct in `crates/keyroost/src/main.rs`. `const PROFILES: u8 = 100` (line 76); `slot: u8` (line 1147); `session: Option<Session>` (line 1119); jobs via `spawn_job` (line 1564) + `take_molto_session` (line 1710); `open_molto` (line 4261); `apply_draft` (line 1758); `select_device` state reset at ~4238-4246; factory-reset confirm-card pattern at ~10956-10985 (`molto_reset_confirm: bool`, line ~1215); slot-list render at ~11008-11043; stale "write-only" copy at ~10996.

---

### Task 1: keyroost-proto — public-block read, parse, and seed-delete builders

**Files:**
- Modify: `crates/keyroost-proto/src/commands.rs`
- Modify: `crates/keyroost-proto/src/lib.rs` (re-export list, ~line 14)
- Create: `crates/keyroost-proto/tests/public_data_kat.rs`

**Interfaces:**
- Produces: `pub fn read_public_data(profile: u8) -> Command`; `pub fn delete_seed(profile: u8) -> Command`; `pub struct ProfilePublicData { pub flag: u8, pub title: Option<String>, pub time_a: u32, pub time_b: u32, pub algorithm: u8, pub time_step: u8, pub digits: u8, pub seed_present: bool }` (derives `Debug, Clone, PartialEq, Eq`); `pub enum PublicDataError` with `impl fmt::Display`; `pub fn parse_public_data(resp: &[u8]) -> Result<ProfilePublicData, PublicDataError>`. All re-exported from the crate root. Consumed by Tasks 2, 3, 5.

- [ ] **Step 1: Write the failing tests**

Create `crates/keyroost-proto/tests/public_data_kat.rs` with exactly:

```rust
//! Known-answer tests for the Molto2 per-profile public block (INS 0x41,
//! P2 = profile) and the keyless seed delete (INS 0xE6).
//!
//! The APDU byte sequences and the response envelope/body layout were
//! captured from real hardware during the 2026-07 probing session (see
//! docs/superpowers/specs/2026-07-03-molto2-slot-overview-design.md).
//! The title/config values in `krprobe99_block` reproduce the observed
//! slot-99 capture ("KRPROBE99", step 30, 6 digits); the two 4-byte time
//! fields are synthetic constants — their semantics are unconfirmed and the
//! parser treats them as opaque big-endian u32s.

use keyroost_proto::commands::{
    delete_seed, parse_public_data, read_public_data, ProfilePublicData, PublicDataError,
};

/// `95 1F 70 1D` + 29-byte body, as the transport hands it over (status word
/// already stripped).
fn envelope(body: &[u8; 29]) -> Vec<u8> {
    let mut v = vec![0x95, 0x1F, 0x70, 0x1D];
    v.extend_from_slice(body);
    v
}

fn krprobe99_block() -> Vec<u8> {
    let mut body = [0u8; 29];
    body[0] = 0x20; // flag as observed
    body[1..10].copy_from_slice(b"KRPROBE99"); // 9 bytes, zero-padded to 16
    body[17..21].copy_from_slice(&[0x00, 0x00, 0x0E, 0x10]); // time A (synthetic)
    body[21..25].copy_from_slice(&[0x00, 0x01, 0x51, 0x80]); // time B (synthetic)
    body[25] = 0x01; // SHA1
    body[26] = 0x1E; // 30 s step
    body[27] = 0x06; // 6 digits
    body[28] = 0x01; // seed present
    envelope(&body)
}

#[test]
fn read_public_data_apdu_bytes() {
    // Case-3 only: `80 41 00 <profile> 01 70`. Le must NOT be appended
    // (hardware rejects the Case-4 form with 6F FB).
    assert_eq!(
        read_public_data(99).apdu,
        [0x80, 0x41, 0x00, 0x63, 0x01, 0x70]
    );
    assert_eq!(
        read_public_data(0).apdu,
        [0x80, 0x41, 0x00, 0x00, 0x01, 0x70]
    );
}

#[test]
fn delete_seed_apdu_bytes() {
    // `80 E6 00 <profile> 00` — plain, keyless (hardware-verified).
    assert_eq!(delete_seed(99).apdu, [0x80, 0xE6, 0x00, 0x63, 0x00]);
    assert_eq!(delete_seed(7).apdu, [0x80, 0xE6, 0x00, 0x07, 0x00]);
}

#[test]
fn parses_all_zero_block_as_empty_untitled() {
    let got = parse_public_data(&envelope(&[0u8; 29])).unwrap();
    assert_eq!(
        got,
        ProfilePublicData {
            flag: 0,
            title: None,
            time_a: 0,
            time_b: 0,
            algorithm: 0,
            time_step: 0,
            digits: 0,
            seed_present: false,
        }
    );
}

#[test]
fn parses_krprobe99_block() {
    let got = parse_public_data(&krprobe99_block()).unwrap();
    assert_eq!(got.flag, 0x20);
    assert_eq!(got.title.as_deref(), Some("KRPROBE99"));
    assert_eq!(got.time_a, 0x0000_0E10);
    assert_eq!(got.time_b, 0x0001_5180);
    assert_eq!(got.algorithm, 0x01);
    assert_eq!(got.time_step, 0x1E);
    assert_eq!(got.digits, 0x06);
    assert!(got.seed_present);
}

#[test]
fn non_utf8_title_decodes_lossily_never_errors() {
    let mut body = [0u8; 29];
    body[1] = 0xFF;
    body[2] = 0xFE;
    let got = parse_public_data(&envelope(&body)).unwrap();
    assert_eq!(got.title.as_deref(), Some("\u{FFFD}\u{FFFD}"));
}

#[test]
fn strict_envelope_rejections() {
    let good = krprobe99_block();

    // Truncated body.
    assert_eq!(
        parse_public_data(&good[..good.len() - 1]),
        Err(PublicDataError::BadOuterLength)
    );
    // Too short for even the envelope header.
    assert_eq!(parse_public_data(&[0x95]), Err(PublicDataError::Truncated));
    assert_eq!(parse_public_data(&[]), Err(PublicDataError::Truncated));

    // Wrong outer tag.
    let mut bad = good.clone();
    bad[0] = 0x94;
    assert_eq!(parse_public_data(&bad), Err(PublicDataError::BadOuterTag));

    // Outer length not covering exactly the nested TLV.
    let mut bad = good.clone();
    bad[1] = 0x20;
    assert_eq!(parse_public_data(&bad), Err(PublicDataError::BadOuterLength));

    // Wrong inner tag.
    let mut bad = good.clone();
    bad[2] = 0x71;
    assert_eq!(parse_public_data(&bad), Err(PublicDataError::BadInnerTag));

    // Wrong inner length.
    let mut bad = good.clone();
    bad[3] = 0x1C;
    assert_eq!(parse_public_data(&bad), Err(PublicDataError::BadInnerLength));

    // Trailing garbage after the body.
    let mut bad = good.clone();
    bad.push(0x00);
    assert_eq!(parse_public_data(&bad), Err(PublicDataError::BadOuterLength));
}

#[test]
fn title_trailing_zeros_stripped_interior_kept() {
    let mut body = [0u8; 29];
    // "AB\0C" then zero padding: trailing zeros are padding, interior is data.
    body[1] = b'A';
    body[2] = b'B';
    body[4] = b'C';
    let got = parse_public_data(&envelope(&body)).unwrap();
    assert_eq!(got.title.as_deref(), Some("AB\u{0}C"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p keyroost-proto --offline --test public_data_kat`
Expected: **compile error** — `read_public_data`, `delete_seed`, `parse_public_data`, `ProfilePublicData`, `PublicDataError` not found in `keyroost_proto::commands`.

- [ ] **Step 3: Implement in `commands.rs`**

In `crates/keyroost-proto/src/commands.rs`, add after the `get_info()` function (~line 80), keeping the crate's existing doc-comment style (full APDU bytes in the first line). `build_apdu`, `build_apdu_get`, and `CLA_PLAIN` are already imported at the top of the file. Add `use core::fmt;` (or `std::fmt` matching the file's existing imports — check the top of the file; if neither is imported, add `use std::fmt;`).

```rust
/// `80 41 00 <profile> 01 70` — read a profile's public block: title,
/// occupancy, and TOTP config. Case-3 only — the device rejects a trailing
/// Le byte with `6F FB`. No authentication required: anyone with card
/// access can read every slot's title and occupancy (hardware-verified).
pub fn read_public_data(profile: u8) -> Command {
    Command {
        label: "read public data",
        apdu: build_apdu(CLA_PLAIN, 0x41, 0x00, profile, &[0x70]),
    }
}

/// `80 E6 00 <profile> 00` — delete one profile's seed. Plain command;
/// hardware-verified to need NO authentication. Returns `90 00` on a
/// populated slot and `6A 83` (referenced data not found) on an empty one.
/// The stored title survives the delete — title and seed have independent
/// lifecycles.
pub fn delete_seed(profile: u8) -> Command {
    Command {
        label: "delete seed",
        apdu: build_apdu_get(CLA_PLAIN, 0xE6, 0x00, profile, 0x00),
    }
}

/// A profile's public block, as returned by [`read_public_data`].
///
/// The two time fields are opaque big-endian u32s; their exact semantics
/// (device RTC / last sync per vendor docs) are unconfirmed, so UIs must
/// not render them as timestamps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePublicData {
    pub flag: u8,
    /// Stored title with trailing zero padding stripped; `None` when the
    /// slot has no title. Decoded lossily — display never fails.
    pub title: Option<String>,
    pub time_a: u32,
    pub time_b: u32,
    pub algorithm: u8,
    pub time_step: u8,
    pub digits: u8,
    pub seed_present: bool,
}

/// Strict-envelope violations from [`parse_public_data`]. Anything that
/// deviates from the captured `95 1F 70 1D <29 bytes>` shape is an error,
/// never a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicDataError {
    /// Shorter than the 4-byte TLV envelope header.
    Truncated,
    /// Leading tag was not `0x95`.
    BadOuterTag,
    /// Outer length did not cover exactly the nested TLV.
    BadOuterLength,
    /// Nested tag was not `0x70`.
    BadInnerTag,
    /// Nested length was not `0x1D` (29).
    BadInnerLength,
}

impl fmt::Display for PublicDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            PublicDataError::Truncated => "response truncated",
            PublicDataError::BadOuterTag => "leading tag is not 0x95",
            PublicDataError::BadOuterLength => "outer TLV length mismatch",
            PublicDataError::BadInnerTag => "nested tag is not 0x70",
            PublicDataError::BadInnerLength => "nested length is not 0x1D",
        };
        f.write_str(s)
    }
}

/// Parse a [`read_public_data`] response (status word already stripped).
///
/// Expected envelope, hardware-captured: `95 1F 70 1D` followed by exactly
/// 29 body bytes — flag, title[16] (plaintext, zero-padded), two u32 BE
/// time fields, algorithm, time step, digit count, seed-present.
pub fn parse_public_data(resp: &[u8]) -> Result<ProfilePublicData, PublicDataError> {
    if resp.len() < 4 {
        return Err(PublicDataError::Truncated);
    }
    if resp[0] != 0x95 {
        return Err(PublicDataError::BadOuterTag);
    }
    if resp[1] as usize != resp.len() - 2 {
        return Err(PublicDataError::BadOuterLength);
    }
    if resp[2] != 0x70 {
        return Err(PublicDataError::BadInnerTag);
    }
    if resp[3] != 0x1D {
        return Err(PublicDataError::BadInnerLength);
    }
    let body = &resp[4..];
    debug_assert_eq!(body.len(), 29); // guaranteed by the two length checks
    let raw_title = &body[1..17];
    let title_len = raw_title.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    let title = if title_len == 0 {
        None
    } else {
        Some(String::from_utf8_lossy(&raw_title[..title_len]).into_owned())
    };
    // Length checks above guarantee these slices; unwraps cannot fail.
    let time_a = u32::from_be_bytes(body[17..21].try_into().unwrap());
    let time_b = u32::from_be_bytes(body[21..25].try_into().unwrap());
    Ok(ProfilePublicData {
        flag: body[0],
        title,
        time_a,
        time_b,
        algorithm: body[25],
        time_step: body[26],
        digits: body[27],
        seed_present: body[28] != 0,
    })
}
```

Note on the length math: outer length `0x1F` (31) covers the nested `70 1D` header (2) plus the 29-byte body, so a well-formed response is 33 bytes and `resp[1] == resp.len() - 2` enforces both truncation and trailing garbage in one check (which is why the truncation/trailing tests expect `BadOuterLength`).

Then extend the re-export list in `crates/keyroost-proto/src/lib.rs` (~line 14). Change:

```rust
pub use commands::{
    answer_challenge, derive_sm4_key, factory_reset, get_challenge, get_info, set_config,
    set_customer_key, set_seed, set_title, sw_auth_failed, sw_ok, sync_time, Command,
    DisplayTimeout, HmacAlgo, OtpDigits, ProfileConfig, TimeStep, DEFAULT_CUSTOMER_KEY,
};
```

to:

```rust
pub use commands::{
    answer_challenge, delete_seed, derive_sm4_key, factory_reset, get_challenge, get_info,
    parse_public_data, read_public_data, set_config, set_customer_key, set_seed, set_title,
    sw_auth_failed, sw_ok, sync_time, Command, DisplayTimeout, HmacAlgo, OtpDigits,
    ProfileConfig, ProfilePublicData, PublicDataError, TimeStep, DEFAULT_CUSTOMER_KEY,
};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p keyroost-proto --offline`
Expected: all tests PASS, including the pre-existing `known_answer_vs_python` suite and the inline `commands.rs` tests (this task changes no existing bytes, so they must stay green untouched).

- [ ] **Step 5: Workspace check**

Run: `cargo test --workspace --offline && cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: green / clean.

- [ ] **Step 6: Commit**

```bash
git add crates/keyroost-proto/src/commands.rs crates/keyroost-proto/src/lib.rs crates/keyroost-proto/tests/public_data_kat.rs
git -c commit.gpgsign=false commit -m "feat(proto): Molto2 per-profile public read (0x41) and keyless seed delete (0xE6)

Hardware probing established that 80 41 00 <profile> 01 70 returns each
slot's title/occupancy/config unauthenticated (Case-3 only), and that
80 E6 00 <profile> 00 deletes a seed with no auth — 90 00 populated,
6A 83 empty, title intact. Strict-envelope parser: any deviation from the
captured 95 1F 70 1D shape is an error, never a guess."
```

---

### Task 2: keyroost-transport — `Session::read_public_data` / `Session::delete_seed`

**Files:**
- Modify: `crates/keyroost-transport/src/lib.rs` (imports ~line 21; `TransportError` enum ~line 56 + `Display` ~line 122; `impl Session` — add methods near `read_info` at ~line 382)

**Interfaces:**
- Consumes (Task 1): `commands::read_public_data`, `commands::delete_seed`, `commands::parse_public_data`, `ProfilePublicData`, `PublicDataError`, `sw_completed`.
- Produces: `Session::read_public_data(&mut self, profile: u8) -> Result<ProfilePublicData, TransportError>`; `Session::delete_seed(&mut self, profile: u8) -> Result<SeedDeleteOutcome, TransportError>`; `pub enum SeedDeleteOutcome { Deleted, AlreadyEmpty }` (derives `Debug, Clone, Copy, PartialEq, Eq`); `TransportError::PublicData(PublicDataError)`. Consumed by Tasks 3, 4, 5, 6.

**Testing note:** `Session` has no card-free test seam (repo convention — the transport crate has only redaction unit tests; all byte-level coverage lives in keyroost-proto, done in Task 1). This task is verified by compile + clippy + the existing suite, then live in Task 8.

- [ ] **Step 1: Extend imports and the error type**

In `crates/keyroost-transport/src/lib.rs`, extend the keyroost-proto import (~line 21) to include the new items. It currently reads (approximately — match the actual list in place):

```rust
use keyroost_proto::commands::{
    self, derive_sm4_key, sw_auth_failed, sw_ok, Command, ProfileConfig, ...
};
```

Add `sw_completed`, `ProfilePublicData`, and `PublicDataError` to that list (keep alphabetical-ish ordering as found).

Add a variant to `TransportError` (enum at ~line 56), next to `MalformedResponse`:

```rust
    /// The per-profile public block failed strict envelope validation.
    PublicData(PublicDataError),
```

And a `Display` arm (impl at ~line 122), next to the `MalformedResponse` arm:

```rust
            TransportError::PublicData(e) => {
                write!(f, "malformed per-profile public block: {}", e)
            }
```

- [ ] **Step 2: Add the two `Session` methods**

In `impl Session`, directly after `read_info` (~line 414), add:

```rust
    /// Read a profile's public block (title, occupancy, TOTP config).
    /// No auth required — the device answers any card holder.
    pub fn read_public_data(&mut self, profile: u8) -> Result<ProfilePublicData, TransportError> {
        let cmd = commands::read_public_data(profile);
        let data = self.transmit(&cmd)?;
        commands::parse_public_data(&data).map_err(TransportError::PublicData)
    }

    /// Delete one profile's seed. No authentication required
    /// (hardware-verified); the destructive-action gate is the caller's
    /// confirmation, not a device auth step. The stored title survives.
    pub fn delete_seed(&mut self, profile: u8) -> Result<SeedDeleteOutcome, TransportError> {
        let cmd = commands::delete_seed(profile);
        // transmit() treats any non-9000 SW as an error, but 6A 83 here just
        // means the slot had no seed — benign for a delete. Go through
        // transmit_raw and map the status words ourselves.
        let (_, sw1, sw2) = self.transmit_raw(&cmd)?;
        if sw_completed(sw1, sw2) {
            return Ok(SeedDeleteOutcome::Deleted);
        }
        if sw1 == 0x6A && sw2 == 0x83 {
            return Ok(SeedDeleteOutcome::AlreadyEmpty);
        }
        Err(TransportError::Apdu {
            label: cmd.label,
            sw1,
            sw2,
        })
    }
```

Add the outcome enum at module level, near `DeviceInfo` (~line 240):

```rust
/// Outcome of a per-profile seed delete (idempotent: both are success).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedDeleteOutcome {
    /// The slot had a seed and it is now gone (`90 00`).
    Deleted,
    /// The slot had no seed to begin with (`6A 83`).
    AlreadyEmpty,
}
```

Check `transmit_raw`'s exact signature at lib.rs:358 before writing the call — the survey reports `(Vec<u8>, u8, u8)`; if the tuple order differs, adapt the destructuring, not the logic.

- [ ] **Step 3: Workspace check**

Run: `cargo test --workspace --offline && cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: green / clean (no behavior change for existing callers).

- [ ] **Step 4: Commit**

```bash
git add crates/keyroost-transport/src/lib.rs
git -c commit.gpgsign=false commit -m "feat(transport): per-profile public read and idempotent seed delete on Session

delete_seed maps 6A 83 to SeedDeleteOutcome::AlreadyEmpty instead of an
error: deleting an empty slot is a benign no-op, and the safety gate for
this keyless command belongs to the calling UI's confirmation."
```

---

### Task 3: CLI — `molto slots`

**Files:**
- Modify: `crates/keyroostctl/src/main.rs` (`mod json_out` ~line 45+; `MoltoCmd` enum ~line 1296; `run_molto` ~line 2251)

**Interfaces:**
- Consumes (Tasks 1-2): `Session::read_public_data`, `ProfilePublicData` fields.
- Produces: `keyroostctl molto slots [--all] [--json]`; `json_out::MoltoSlotJson`; `fn molto_algo_label(algo: u8) -> String` (reused by nothing else yet, but keep it a free fn beside `sanitize_cert_field`).

- [ ] **Step 1: Add the subcommand variant**

In the `MoltoCmd` enum (~line 1296), after the `Info` variant, add:

```rust
    /// List the 100 profile slots: occupancy, title, TOTP config.
    /// Titles and occupancy are readable by anyone holding the token —
    /// no customer key is needed (or used).
    Slots {
        /// Show all 100 slots, including empty untitled ones.
        #[arg(long)]
        all: bool,
    },
```

- [ ] **Step 2: Add the JSON shape**

In `mod json_out`, next to `MoltoInfoJson` (~line 64):

```rust
    /// One element of `keyroostctl molto --json slots` (full parsed block).
    /// `time_a`/`time_b` are raw big-endian u32s with unconfirmed semantics.
    #[derive(Serialize)]
    pub struct MoltoSlotJson {
        pub slot: u8,
        pub occupied: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
        pub flag: u8,
        pub algorithm: u8,
        pub time_step: u8,
        pub digits: u8,
        pub time_a: u32,
        pub time_b: u32,
    }
```

- [ ] **Step 3: Add the handler**

In `run_molto`, directly after the `MoltoCmd::Info` early-return block (~line 2300) and following its exact shape (no-auth path), add:

```rust
    // Slots is read-only and needs no auth — the public block answers any
    // card holder (that's also why the output warns about title privacy).
    if let MoltoCmd::Slots { all } = cmd {
        let mut session = Session::open()?;
        session.set_debug(debug);
        let info = session.read_info()?;
        print_info(&info);
        let mut slots = Vec::with_capacity(100);
        for p in 0..=99u8 {
            slots.push(session.read_public_data(p)?);
        }
        if json_output() {
            let out: Vec<json_out::MoltoSlotJson> = slots
                .iter()
                .enumerate()
                .map(|(i, b)| json_out::MoltoSlotJson {
                    slot: i as u8,
                    occupied: b.seed_present,
                    title: b.title.clone(),
                    flag: b.flag,
                    algorithm: b.algorithm,
                    time_step: b.time_step,
                    digits: b.digits,
                    time_a: b.time_a,
                    time_b: b.time_b,
                })
                .collect();
            emit_json(&out)?;
            return Ok(());
        }
        let shown: Vec<_> = slots
            .iter()
            .enumerate()
            .filter(|(_, b)| *all || b.seed_present || b.title.is_some())
            .collect();
        if shown.is_empty() {
            println!("no occupied or titled slots (use --all to list all 100)");
            return Ok(());
        }
        println!(
            "{:>4}  {:>8}  {:<16}  {:<6}  {:>4}  {:>6}",
            "slot", "occupied", "title", "algo", "step", "digits"
        );
        for (i, b) in shown {
            println!(
                "{:>4}  {:>8}  {:<16}  {:<6}  {:>4}  {:>6}",
                i,
                if b.seed_present { "yes" } else { "-" },
                b.title.as_deref().map(sanitize_cert_field).unwrap_or_default(),
                molto_algo_label(b.algorithm),
                b.time_step,
                b.digits,
            );
        }
        return Ok(());
    }
```

Add the label helper as a free fn next to `sanitize_cert_field` (~line 5484):

```rust
/// Human label for the public-block algorithm byte. Same coding as the
/// config TLV's hmac_algo (1=SHA1, 2=SHA256); anything else prints raw.
fn molto_algo_label(algo: u8) -> String {
    match algo {
        0x01 => "SHA1".into(),
        0x02 => "SHA256".into(),
        other => format!("0x{other:02X}"),
    }
}
```

Exhaustiveness: `run_molto`'s tail `match` (the authenticated arms) must still compile. Mirror however `MoltoCmd::Info` and `MoltoCmd::Reset` are excluded there — if they have `unreachable!()` arms, add `MoltoCmd::Slots { .. } => unreachable!("handled before auth"),`; if the early returns mean they're absent under a catch-all, no arm is needed. Look at the actual tail match before editing.

- [ ] **Step 4: Build + workspace check**

Run: `cargo build --offline -p keyroostctl && cargo test --workspace --offline && cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: green / clean. Also sanity-check help renders: `cargo run --offline -p keyroostctl -- molto slots --help` — expected: the new help text, exit 0 (no device needed for `--help`).

- [ ] **Step 5: Commit**

```bash
git add crates/keyroostctl/src/main.rs
git -c commit.gpgsign=false commit -m "feat(cli): molto slots — list occupancy, titles, and TOTP config per slot

Reads all 100 public blocks keylessly (the device answers any card
holder — the help text says so, since that privacy property surprises).
Default view hides empty untitled slots; --all shows the full table;
--json emits the complete parsed block per slot. Titles pass through the
same control-character flattening as certificate fields."
```

---

### Task 4: CLI — `molto title` read mode + `molto delete --yes`

**Files:**
- Modify: `crates/keyroostctl/src/main.rs` (`MoltoCmd::Title` ~line 1331; new `Delete` variant; `run_molto` early blocks + the `Title` tail-match arm ~line 2433)

**Interfaces:**
- Consumes (Tasks 1-2): `Session::read_public_data`, `Session::delete_seed`, `SeedDeleteOutcome` (add to the `use keyroost_transport::{...}` import at the top of main.rs).
- Produces: `molto title -p N` (read, keyless) / `molto title -p N TITLE` (write, unchanged); `molto delete -p N --yes`.

- [ ] **Step 1: Make TITLE optional**

Change the `Title` variant (~line 1331) from:

```rust
    /// Write a profile title (1..=12 ASCII chars).
    Title {
        #[arg(short, long)]
        profile: u8,
        title: String,
    },
```

to:

```rust
    /// Write a profile title (1..=12 ASCII chars), or print the current
    /// one when TITLE is omitted (reading needs no customer key).
    Title {
        #[arg(short, long)]
        profile: u8,
        /// New title; omit to read the slot's stored title instead.
        title: Option<String>,
    },
```

- [ ] **Step 2: Add the `Delete` variant**

After `Title` in the enum:

```rust
    /// Delete one profile's seed. The title, if any, survives. Keyless:
    /// the device accepts this from any card holder (hardware-verified),
    /// so the only gate is --yes.
    Delete {
        #[arg(short, long)]
        profile: u8,
        /// Confirm you really want to delete this slot's seed.
        #[arg(long)]
        yes: bool,
    },
```

- [ ] **Step 3: Add the two no-auth handlers**

In `run_molto`, after the `Slots` block from Task 3, add both early-return blocks:

```rust
    // Title with TITLE omitted is a read — keyless, like Info/Slots.
    if let MoltoCmd::Title {
        profile,
        title: None,
    } = cmd
    {
        let mut session = Session::open()?;
        session.set_debug(debug);
        let block = session.read_public_data(*profile)?;
        match &block.title {
            Some(t) => println!("slot #{} title: {}", profile, sanitize_cert_field(t)),
            None => println!("slot #{} has no title", profile),
        }
        println!("occupied: {}", if block.seed_present { "yes" } else { "no" });
        return Ok(());
    }

    // Delete needs no auth (hardware-verified) — gate on --yes, and show
    // what's in the slot before touching it.
    if let MoltoCmd::Delete { profile, yes } = cmd {
        let mut session = Session::open()?;
        session.set_debug(debug);
        let info = session.read_info()?;
        print_info(&info);
        let block = session.read_public_data(*profile)?;
        println!(
            "slot #{}: occupied: {}, title: {}",
            profile,
            if block.seed_present { "yes" } else { "no" },
            block
                .title
                .as_deref()
                .map(sanitize_cert_field)
                .unwrap_or_else(|| "(none)".into()),
        );
        if !yes {
            return Err(format!(
                "refusing to delete slot #{}'s seed on device serial {} without --yes",
                profile, info.serial
            )
            .into());
        }
        match session.delete_seed(*profile)? {
            SeedDeleteOutcome::Deleted => {
                println!("seed deleted from slot #{}; the title (if any) remains", profile)
            }
            SeedDeleteOutcome::AlreadyEmpty => println!("slot #{} was already empty", profile),
        }
        return Ok(());
    }
```

- [ ] **Step 4: Fix the authenticated `Title` write arm**

The tail-match arm (~line 2433) currently destructures `title: String`. Change it to:

```rust
        MoltoCmd::Title { profile, title } => {
            let title = title.as_deref().expect("title read mode is handled before auth");
            if title.is_empty() || title.len() > 12 {
                return Err("title must be 1..=12 bytes".into());
            }
            session.set_title(*profile, title)?;
            println!("title set on profile #{}", profile);
        }
```

Handle `MoltoCmd::Delete` exhaustiveness in the tail match the same way as `Slots` in Task 3 (mirror the Info/Reset pattern).

- [ ] **Step 5: Build + workspace check**

Run: `cargo build --offline -p keyroostctl && cargo test --workspace --offline && cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: green / clean. Sanity-check: `cargo run --offline -p keyroostctl -- molto delete --help` and `-- molto title --help` render the new texts.

- [ ] **Step 6: Commit**

```bash
git add crates/keyroostctl/src/main.rs
git -c commit.gpgsign=false commit -m "feat(cli): molto title read-back and molto delete with --yes gate

Omitting TITLE turns molto title into a keyless read of the stored title
and occupancy. molto delete prints the slot's contents before acting,
refuses without --yes (naming the device serial, like reset), and reports
'already empty' on 6A 83 instead of erroring — the delete is idempotent."
```

---

### Task 5: GUI — public-block sweep + titles/occupancy in the slot list

**Files:**
- Modify: `crates/keyroost/src/main.rs` (`App` fields ~line 1147; `open_molto` ~line 4261; `select_device` reset ~line 4244; `apply_draft` ~line 1758; slot-list render ~line 11008; honesty copy ~line 10996)

**Interfaces:**
- Consumes (Tasks 1-2): `Session::read_public_data`, `ProfilePublicData`. Add `use keyroost_proto::commands::ProfilePublicData;` (the GUI already depends on keyroost-proto; match the file's existing import style near `main.rs:25`).
- Produces: `App.slot_meta: Option<Vec<ProfilePublicData>>` (always 100 entries when `Some`); `fn sanitize_title(&str) -> String`. Task 6 updates `slot_meta` entries after its writes.

- [ ] **Step 1: Add state**

Next to `slot: u8` (~line 1147):

```rust
    /// Per-slot public blocks (title / occupancy / config) from the sweep
    /// that runs when the device is opened. `None` until the sweep has
    /// succeeded — the slot list then degrades to bare slot numbers.
    /// Indexed by slot; always 100 entries when `Some`.
    slot_meta: Option<Vec<ProfilePublicData>>,
```

Initialize `slot_meta: None,` in the `App` constructor (find where `slot`/`draft` are initialized) and reset it in `select_device` next to `self.session = None;` (~line 4244): `self.slot_meta = None;`.

- [ ] **Step 2: Sweep inside `open_molto`**

In `open_molto` (~line 4261), extend the job closure. Current shape:

```rust
        let result = (|| -> Result<(Session, DeviceInfo), TransportError> {
            let mut s = Session::open_named(&reader)?;
            let info = s.read_info()?;
            Ok((s, info))
        })();
```

becomes:

```rust
        let result = (|| -> Result<(Session, DeviceInfo, Option<Vec<ProfilePublicData>>), TransportError> {
            let mut s = Session::open_named(&reader)?;
            let info = s.read_info()?;
            // Public-block sweep, one APDU per slot (~1-2 s). A failed sweep
            // degrades to the bare slot list; it must never block the open.
            let meta = (0..PROFILES)
                .map(|p| s.read_public_data(p))
                .collect::<Result<Vec<_>, _>>()
                .ok();
            Ok((s, info, meta))
        })();
```

and in the apply closure change the `Ok` arm binding to `Ok((s, info, meta))` and add `app.slot_meta = meta;` next to `app.info = Some(info);`. Everything else in the arm (the log line, the device serial/name refresh) stays untouched.

- [ ] **Step 3: Refresh the written slot in `apply_draft`**

In `apply_draft` (~line 1758), after the existing `result` chain (`set_seed` → `set_title` → `set_config`) and before `Box::new(...)`, add a soft re-read (a failed refresh must not turn a successful write into an error):

```rust
        let refreshed = result.is_ok().then(|| s.read_public_data(p).ok()).flatten();
```

and in the apply closure's `Ok(())` arm, after the existing `wipe(...)` line:

```rust
                    if let (Some(meta), Some(block)) = (app.slot_meta.as_mut(), refreshed) {
                        if let Some(slot) = meta.get_mut(p as usize) {
                            *slot = block;
                        }
                    }
```

(The closure now also captures `refreshed`; no other changes.)

- [ ] **Step 4: Titles + occupancy badges in the slot list**

In the slot-list `ScrollArea` (~line 11008-11043), the row currently paints `format!("Slot {s:02}")`. Change the loop body: before the paint, look up the metadata, and after painting the (possibly extended) label, draw an occupancy dot:

```rust
                let meta = self
                    .slot_meta
                    .as_ref()
                    .and_then(|m| m.get(s as usize));
                let label = match meta.and_then(|b| b.title.as_deref()) {
                    Some(t) => format!("Slot {s:02} \u{00B7} {}", sanitize_title(t)),
                    None => format!("Slot {s:02}"),
                };
```

use `label` in the existing `ui.painter().text(...)` call (replacing the `format!("Slot {s:02}")` argument, everything else identical), and after that call add:

```rust
                if meta.is_some_and(|b| b.seed_present) {
                    ui.painter().circle_filled(
                        egui::pos2(rect.right() - 12.0, rect.center().y),
                        3.0,
                        p.brand,
                    );
                }
```

Add the sanitize helper as a free fn near the bottom of main.rs (beside other free helpers like `editor_row` ~line 11750):

```rust
/// Flatten control characters out of a device-provided title before display
/// (same policy as the CLI's sanitize_cert_field).
fn sanitize_title(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}
```

- [ ] **Step 5: Honesty copy**

Replace the stale card text at ~line 10996 — currently:

```
The token is write-only: pick a slot and program it. The Molto2 shows codes on its own screen — they can't be read back here.
```

with:

```
Pick a slot and program it. Slot titles and occupancy are readable by anyone holding the token — no key needed — so don't put secrets in titles. Seeds and codes can't be read back; codes show on the device's own screen.
```

(keep the surrounding `egui::RichText` styling as-is; the string uses `\u{2014}` for the em-dashes, matching the file's existing escapes). Also fix the stale module doc at main.rs:4 (`100-slot grid on the left`) to say `slot list on the left` — it predates the list redesign.

- [ ] **Step 6: Build + workspace check + release rebuild**

Run: `cargo test --workspace --offline && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo build --release --offline -p keyroost`
Expected: green / clean. (The release build keeps the user's `keyroost` PATH symlink fresh.)

- [ ] **Step 7: Commit**

```bash
git add crates/keyroost/src/main.rs
git -c commit.gpgsign=false commit -m "feat(gui): show real titles and occupancy in the Molto2 slot list

Opening the device now also sweeps the 100 per-profile public blocks
(one keyless APDU each); the slot list shows each slot's stored title and
an occupancy dot, degrading to bare numbers if the sweep fails. Writes
refresh the affected slot's block. The old 'write-only' honesty copy was
wrong since the 0x41 read was found — replaced with the real privacy
story: anyone holding the token can read titles and occupancy."
```

---

### Task 6: GUI — title-only write + confirm-gated seed delete

**Files:**
- Modify: `crates/keyroost/src/main.rs` (`App` fields ~line 1215; methods near `apply_draft` ~line 1758; `molto_view` button row ~line 11125; `select_device` reset ~line 4241)

**Interfaces:**
- Consumes: Task 5's `slot_meta`; Tasks 1-2's `Session::set_title` path, `Session::delete_seed`, `SeedDeleteOutcome` (extend the `use keyroost_transport::{DeviceInfo, Session, TransportError};` import at main.rs:28 with `SeedDeleteOutcome`); existing `ensure_auth` (~line 2167), `take_molto_session` (~line 1710), `spawn_job`, `wipe`, `Severity`.
- Produces: `App::apply_title_only()`, `App::delete_seed_selected()`, `App.molto_delete_confirm: bool`.

- [ ] **Step 1: Add the confirm flag**

Next to `molto_reset_confirm: bool` (~line 1215):

```rust
    /// True while the per-slot seed-delete confirmation card is showing.
    molto_delete_confirm: bool,
```

Initialize `molto_delete_confirm: false,` in the constructor and reset it in `select_device` next to `molto_reset_confirm` (~line 4241): `self.molto_delete_confirm = false;`. Also reset it on slot switch — in `molto_view`, where `if let Some(s) = pick { self.slot = s; }` applies the slot pick, extend to:

```rust
                if let Some(s) = pick {
                    self.slot = s;
                    self.molto_delete_confirm = false;
                }
```

(a confirm armed for one slot must not carry over to another).

- [ ] **Step 2: Add `apply_title_only`**

Directly after `apply_draft` (~line 1758's fn end):

```rust
    /// Write only the selected slot's title — no seed re-entry, no config.
    /// Needs auth (set_title is a secure command); the seed is untouched.
    fn apply_title_only(&mut self) {
        if !self.ensure_auth() {
            return;
        }
        let title = self.draft.title.trim().to_owned();
        if title.is_empty() || title.len() > 12 {
            self.log(Severity::Err, "title must be 1..=12 bytes");
            return;
        }
        let p = self.slot;
        let Some(mut s) = self.take_molto_session() else {
            return;
        };
        self.spawn_job(format!("Writing title on #{p}\u{2026}"), move || {
            let result = s
                .set_title(p, &title)
                .map_err(|e| format!("set_title #{}: {}", p, e));
            let refreshed = result.is_ok().then(|| s.read_public_data(p).ok()).flatten();
            Box::new(move |app: &mut App| {
                app.session = Some(s);
                match result {
                    Ok(()) => {
                        if let (Some(meta), Some(block)) = (app.slot_meta.as_mut(), refreshed) {
                            if let Some(slot) = meta.get_mut(p as usize) {
                                *slot = block;
                            }
                        }
                        app.log(Severity::Ok, format!("title written on slot #{}", p));
                    }
                    Err(e) => app.log(Severity::Err, e),
                }
            })
        });
    }

    /// Delete the selected slot's seed. Keyless on the wire (the device
    /// accepts it from any card holder); the GUI's gate is the confirm
    /// card. The stored title survives — the refreshed block shows it.
    fn delete_seed_selected(&mut self) {
        let p = self.slot;
        let Some(mut s) = self.take_molto_session() else {
            return;
        };
        self.spawn_job(format!("Deleting seed on #{p}\u{2026}"), move || {
            let result = s
                .delete_seed(p)
                .map_err(|e| format!("delete seed #{}: {}", p, e));
            let refreshed = result.is_ok().then(|| s.read_public_data(p).ok()).flatten();
            Box::new(move |app: &mut App| {
                app.session = Some(s);
                match result {
                    Ok(outcome) => {
                        if let (Some(meta), Some(block)) = (app.slot_meta.as_mut(), refreshed) {
                            if let Some(slot) = meta.get_mut(p as usize) {
                                *slot = block;
                            }
                        }
                        match outcome {
                            SeedDeleteOutcome::Deleted => app.log(
                                Severity::Ok,
                                format!("seed deleted from slot #{} (title kept)", p),
                            ),
                            SeedDeleteOutcome::AlreadyEmpty => {
                                app.log(Severity::Ok, format!("slot #{} was already empty", p))
                            }
                        }
                    }
                    Err(e) => app.log(Severity::Err, e),
                }
            })
        });
    }
```

- [ ] **Step 3: Wire the buttons + confirm card into `molto_view`**

In the button row at ~line 11125 (currently `Write to slot` / `Import otpauth…` / etc.), add after the `Write to slot` button:

```rust
                    if theme::button(ui, p, BtnKind::Default, "Write title only").clicked() {
                        self.apply_title_only();
                    }
                    ui.add_space(6.0);
                    if theme::button(ui, p, BtnKind::Danger, "Delete seed\u{2026}").clicked() {
                        self.molto_delete_confirm = true;
                    }
```

Below the row (same level as the factory-reset confirm card pattern at ~10956-10985 — copy its exact `egui::Frame` styling), add:

```rust
                if self.molto_delete_confirm {
                    ui.add_space(10.0);
                    egui::Frame::NONE
                        .fill(p.err_soft())
                        .inner_margin(egui::Margin::same(12))
                        .corner_radius(egui::CornerRadius::same(8))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Delete slot {:02}'s seed? Only the seed is wiped \u{2014} the title stays. No key is needed for this: anyone holding the token could do the same.",
                                    self.slot
                                ))
                                .font(theme::f_reg(12.5))
                                .color(p.txt),
                            );
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if theme::button(ui, p, BtnKind::Danger, "Yes, delete seed").clicked() {
                                    self.molto_delete_confirm = false;
                                    self.delete_seed_selected();
                                }
                                ui.add_space(6.0);
                                if theme::button(ui, p, BtnKind::Default, "Cancel").clicked() {
                                    self.molto_delete_confirm = false;
                                }
                            });
                        });
                }
```

If the row is getting cramped (four+ buttons), it's fine to put `Delete seed…` on its own line under the row — match the pane's existing spacing idiom; the confirm card structure is the non-negotiable part.

- [ ] **Step 4: Build + workspace check + release rebuild**

Run: `cargo test --workspace --offline && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo build --release --offline -p keyroost`
Expected: green / clean.

- [ ] **Step 5: Commit**

```bash
git add crates/keyroost/src/main.rs
git -c commit.gpgsign=false commit -m "feat(gui): per-slot title-only write and confirm-gated seed delete

Write title only closes the parity gap where the bundled write demanded
seed re-entry just to rename a slot. Delete seed uses the same confirm-
card pattern as factory reset, sends the keyless 0xE6, and re-reads the
slot's public block — which still shows the title, because the device
keeps titles across seed deletion."
```

---

### Task 7: Docs — PROTOCOL.md + TODO-v0.7.5.md

**Files:**
- Modify: `docs/PROTOCOL.md` (plain-commands table ~line 79; new subsections after the get-info layout ~line 98; status-words table ~line 141; Known unknowns item 1 ~line 157)
- Modify: `TODO-v0.7.5.md` (the titles section, lines 38-48)

**Interfaces:** none (prose only). Copy the byte facts exactly as written below — they are hardware-verified and must not drift from the code written in Tasks 1-2.

- [ ] **Step 1: PROTOCOL.md — plain-commands table**

Add two rows to the table at ~line 79-84 (after the existing `0x41` row):

```markdown
| `0x41` | `00` | profile (0..99) | `70` (Lc=`01`) | Per-profile public block | Title + occupancy + TOTP config |
| `0xE6` | `00` | profile (0..99) | — (Lc=`00`) | — (sw=`9000`/`6A83`) | Delete one profile's seed (keyless) |
```

- [ ] **Step 2: PROTOCOL.md — new subsections**

After the `#### 0x41 get info response layout` block (insert after ~line 98, before `### Secure commands`):

```markdown
#### `0x41` per-profile public block (P2 = profile)

`80 41 00 <profile> 01 70` — the same INS as get-info, but P2 selects a
profile and the body is the single byte `0x70`. **Case-3 only:** appending
an Le byte is rejected with `6F FB`. The response is a TLV followed by the
status word:

```
95 1F
   70 1D
      offset  length  field
      0       1       flag (observed 0x20 on a written slot)
      1       16      title, PLAINTEXT, zero-padded
      17      4       time field A (u32 BE; semantics unconfirmed)
      21      4       time field B (u32 BE; semantics unconfirmed)
      25      1       OTP algorithm (1=SHA1, 2=SHA256 — same coding as the config TLV)
      26      1       time step in seconds (0x1E = 30)
      27      1       digit count (e.g. 0x06)
      28      1       seed present (00/01)
```

**No authentication is required**, and the title comes back in the clear:
the device decrypts the `set_title` ciphertext on receipt and stores
plaintext (verified live — a title written encrypted read back verbatim).
Anyone with card access can read every slot's title and occupancy without
the customer key. Don't put secrets in titles.

#### `0xE6` delete profile seed (keyless)

`80 E6 00 <profile> 00` — deletes one profile's seed. Hardware-verified
(the vendor tooling happens to send it after authenticating, but auth is
NOT a precondition — reproduced twice with no auth at all):

- `90 00` on a populated slot; `6A 83` (referenced data not found) on an
  already-empty one.
- The stored title survives the delete — title and seed have independent
  lifecycles. There is no title-delete command short of a factory reset.
- **Security note:** any party with card access can wipe any profile's
  seed without the customer key. That is device behavior, documented here
  so users can weigh it; keyroost gates the operation behind explicit
  confirmation in both UIs.
```

- [ ] **Step 3: PROTOCOL.md — status words + known unknowns**

Add a row to the status-word table (~line 141-146), before the `other` row:

```markdown
| `6A83` | Referenced data not found (e.g. `0xE6` on a slot with no seed) |
```

Replace Known-unknowns item 1 (~lines 157-160) — currently the "Slot read-back … treats slots as write-only and tracks state in a local sidecar" paragraph — with:

```markdown
1. **Seed read-back.** The per-profile public block (`0x41` with
   P2 = profile) returns each slot's title, occupancy, and TOTP config —
   but no command is known to return a profile's *seed*, and the two
   4-byte time fields' semantics are unconfirmed. Seeds remain write-only.
```

- [ ] **Step 4: TODO-v0.7.5.md — replace the stale section**

Replace the whole `## Molto2 — surface the per-profile title (≤12 bytes), per slot` section (lines 38-48, all three checkbox items) with:

```markdown
## Molto2 — slot overview (titles, occupancy, per-slot delete)

Superseded by `docs/superpowers/specs/2026-07-03-molto2-slot-overview-design.md`
and its implementation plan. The old read-back assumption here was wrong:
hardware probing found `80 41 00 <profile> 01 70` returns title, occupancy,
and config in the clear (no key), and `80 E6 00 <profile> 00` deletes a
seed keylessly. Wire format now in `docs/PROTOCOL.md`.
```

- [ ] **Step 5: Commit**

```bash
git add docs/PROTOCOL.md TODO-v0.7.5.md
git -c commit.gpgsign=false commit -m "docs(protocol): document the 0x41 per-profile read and keyless 0xE6 delete

Both hardware-verified during the slot-overview probing session. The
notable facts: titles are stored (and returned) in PLAINTEXT with no auth,
0xE6 needs no auth either (vendor ordering was incidental), and a title
survives its seed's deletion. Known-unknowns narrows from 'slot read-back'
to 'seed read-back'; TODO's stale titles section now points at the spec."
```

---

### Task 8: Hardware smoke (requires the attached Molto2)

**Files:** none (verification only; fixes, if any, become follow-up commits on the affected task's files).

**Preconditions:** Molto2 on a **direct USB port**. Slot 99 is the sanctioned test slot (currently titled `KRPROBE99`, seedless). Default customer key `TOKEN2MOLTO1-KEY` is in effect unless the user says otherwise. If the device is not attached, stop and report — do not skip silently.

Rebuild both release binaries first so the user's PATH symlinks are fresh:

```bash
cargo build --release --offline -p keyroostctl -p keyroost
```

- [ ] **Step 1: `molto slots` ground truth**

Run: `keyroostctl molto slots`
Expected: table shows slot 99 with title `KRPROBE99`, occupied `-`; no other rows (all other slots empty/untitled). Then `keyroostctl molto slots --all` shows all 100, and `keyroostctl --json molto slots | head -30` emits valid JSON.
If the parse fails with `malformed per-profile public block`, capture the raw bytes with `keyroostctl --debug molto slots 2>&1 | head -40`, diff the envelope against the KAT vectors, and fix the parser + KATs together with a written justification (repo convention for changing expected bytes).

- [ ] **Step 2: Title read + write round-trip**

Run: `keyroostctl molto title -p 99`
Expected: `slot #99 title: KRPROBE99`, `occupied: no`.
Run: `keyroostctl molto title -p 99 KRSMOKE` then `keyroostctl molto title -p 99`
Expected: write succeeds after auth; read-back shows `KRSMOKE`. Also confirm the title on the device screen. Restore: `keyroostctl molto title -p 99 KRPROBE99`.

- [ ] **Step 3: Seed → delete → idempotent delete (slot 99 only)**

```bash
keyroostctl molto seed -p 99 --base32 GEZDGNBVGY3TQOJQ    # check `molto seed --help` for the exact seed flag spelling first
keyroostctl molto slots        # expect slot 99 occupied: yes
keyroostctl molto delete -p 99 --yes
keyroostctl molto slots        # expect occupied back to '-', title still KRPROBE99
keyroostctl molto delete -p 99 --yes    # expect: "slot #99 was already empty"
```

Also verify `keyroostctl molto delete -p 99` (no `--yes`) refuses and shows the slot contents first.

- [ ] **Step 4: GUI pass**

Launch `keyroost`. Select the Molto2. Expected: after the open job (~1-2 s longer than before), the slot list shows `Slot 99 · KRPROBE99`; other slots bare. Exercise: select slot 99, authenticate, `Write title only` with a new title (confirm list + device screen update); seed slot 99 via the normal write, confirm the occupancy dot appears; `Delete seed…` → confirm card → confirm dot disappears and title remains; check the log lines. Restore title `KRPROBE99` and leave slot 99 seedless.

- [ ] **Step 5: Record results**

Report each check's outcome to the user (pass/fail with observed output). Any deviation from expected bytes/behavior is a finding to fix before the branch is offered for review — not a note to skip past.

---

## Self-review (done at plan-writing time)

- **Spec coverage:** proto builders/struct/parse/KATs → Task 1; transport methods + idempotent delete → Task 2; `molto slots`/`--all`/`--json` → Task 3; title read mode + `molto delete --yes` with pre-delete display → Task 4; GUI sweep, badges, degrade-on-failure, honesty copy → Task 5; GUI edit-title-only + confirm-gated delete + post-delete refresh → Task 6; PROTOCOL.md + TODO replacement → Task 7; the spec's hardware-smoke list → Task 8. Open item 1 (time fields): parsed BE, JSON/doc-comment only, never rendered. Open item 2: 0-based wire indexing kept everywhere.
- **Known deviation:** the spec says KATs "embed the two captured responses byte-for-byte", but the raw hex captures were not recorded in the spec — Task 1's vectors reproduce the captured layout and observed values (flag/title/step/digits) with synthetic time fields, clearly labeled. Task 8 Step 1 closes the loop against the live device.
- **Type consistency:** `ProfilePublicData` fields match between Task 1 (definition), Task 2 (transport), Task 3 (JSON mapping), Task 5 (GUI). `SeedDeleteOutcome::{Deleted, AlreadyEmpty}` consistent in Tasks 2/4/6. `sanitize_cert_field` (CLI) vs `sanitize_title` (GUI) are intentionally two helpers — different crates, no shared util crate for string helpers.
