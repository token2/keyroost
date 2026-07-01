# FIDO2 large-blob storage — roadmap & design

- **Date:** 2026-06-30
- **Status:** Design approved in brainstorming; roadmap captured for review. Not
  scheduled to a specific release — tiers land as they're built, around whatever
  v0.7.3 turns up in the field.
- **Scope:** The CTAP 2.1 `authenticatorLargeBlobs` array that is common across
  modern FIDO2 keys (YubiKey 5.5.1+, Token2, Solo/Nitrokey, etc.). Not the
  Molto2 device (see *Out of scope*).

## The one idea everything hangs on

**The user decides what their large blob is for.**

The large blob is a single small (~1 KB), key-global side area — separate from
FIDO credentials, separate from PIV/OpenPGP. keyroost's job is to make that
space *legible* and then let the person *deliberately claim it* for a purpose:

- leave it alone (just inspect what's there),
- keep a plain-text note (world-readable),
- lock a few secrets into an **encrypted note**, or
- hold an **SSH certificate** the way OpenSSH/Yubico intend.

That "choose how to use it" is the top-level UX. Newcomers pick a **purpose
preset**; power users get a **capacity meter + raw hex/ASCII view** underneath.
One space, one deliberate choice, clearly surfaced.

## Why keyroost is the right home for this

keyroost is a friendly, cross-platform, cross-vendor management layer for
hardware security keys. First-party tooling for the large blob is thin and
getting thinner:

- The only established large-blob tooling is `fido2-token`/libfido2 —
  developer-grade, Linux-centric (`/dev/hidraw0` device paths), no GUI
  ([Yubico: Storing SSH Certificates](https://developers.yubico.com/SSH/Storing_SSH_Certificates.html)).
- **YubiKey Manager GUI reached end-of-life on 2026-02-19**; Yubico now points
  users to *Yubico Authenticator*, which does **not** manage large blobs, PIV
  data objects, or SSH-cert-on-key. The friendly-GUI surface for this work just
  disappeared.

keyroost already implements the hard part — a checksum-safe, structural
large-blob read/write layer (`keyroost-ctap/src/large_blobs.rs`) with a
plain-text note layer, exposed via `keyroostctl fido large-blob …` and the GUI
Storage sub-view. This roadmap builds *purpose* on top of that foundation.

## What a large blob actually is (and isn't)

Grounding, because it's widely conflated with SSH auth:

- **SSH auth on a security key** uses a FIDO2 **discoverable/resident
  credential** (`ssh-keygen -t ed25519-sk -O resident`). The private seed never
  leaves the key; `ssh-keygen -K` regenerates the stub files anywhere. This has
  **nothing to do with the large blob** — it's the credential store, which
  keyroost already manages (creds list/delete).
- **The large blob** is a separate ~1 KB scratch area. Its only spec-blessed SSH
  use is holding the **CA-signed SSH certificate** (`-cert.pub`) so the cert
  travels with the resident credential. So the blob adds value for SSH **only if
  you use SSH certificates** — an org/CA/fleet workflow, not plain sk-key auth.

Consequence for the roadmap: the blob is not a storage product. It's a
*legibility problem* plus a *small pool of leftable space*. The tiers below are
ordered by honest value, not novelty.

## The tiers

| Tier | Purpose | Audience | New crypto/transport | Status |
|---|---|---|---|---|
| **A** | **Legibility** — decode & export everything in the blob | anyone curious about their key | none | to build (raw hex exists) |
| **B** | **Plain-text note** | casual | none | **shipped** |
| **C1** | **Encrypted note (passphrase)** — portable | recovery codes, wallet seeds | KDF + AEAD | to build |
| **C2** | **Encrypted note (device-bound)** — hardware-hardened | offline-attack-averse | `hmac-secret` | to build (additive on C1) |
| **D** | **SSH-cert companion** — real interop | SSH-CA / fleet users | `largeBlobKey` AEAD | to build |

Sequencing is **pure engineering layering**, not demand-gated. We build C1, then
layer C2 and D as they come. No tier waits on a usage signal to justify the
next.

### A — Legibility (the foundation)

Make the blob's contents readable and exportable, whatever put them there.

- **Entry recognition.** Classify each entry in the array:
  - keyroost plain-text note (existing magic prefix),
  - keyroost encrypted note (new container, see C),
  - opaque relying-party / AEAD data (read-only; keyroost never rewrites it),
  - **SSH certificate** — recognize the OpenSSH cert format and decode the
    human-relevant fields (type, key-id, principals, valid-from/valid-to,
    critical options).
- **Views.** Per-entry hex + ASCII (exists), plus a parsed/structured view for
  recognized types. A **capacity meter**: total / used / free bytes and entry
  count, so scarcity is visible before the user commits space.
- **Export.** Save any entry's bytes to a file (e.g. dump an SSH cert back to
  `-cert.pub`).
- Everything here is read-only inspection + export; no new crypto, low risk.
  This is keyroost's defensible core and the immediate ykman-GUI-gap filler.

### C — Encrypted note

The value center. AEAD-encrypt small, high-value "break-glass" data that should
live physically on the key: Signal/session recovery codes, a wallet seed, key
info. Stored as a keyroost-authored entry alongside (not replacing) RP data.

**On-blob container format (designed once, in C1, to carry both modes):**

```
magic          keyroost encrypted-note tag (distinct from the plain-note magic)
version         u8   format version
protection_mode u8  0 = passphrase (C1), 1 = device-bound, 2 = passphrase+device (C2)
kdf_id          u8   e.g. Argon2id / scrypt identifier
kdf_params      …    cost params + salt (passphrase modes)
hmac_cred_ref   …    credential/salt reference (device-bound modes; absent in C1)
nonce           …    AEAD nonce
ciphertext+tag  …    AEAD(plaintext)
```

The `version` + `protection_mode` fields are the whole point of doing the format
work in C1: C2 becomes purely additive — new mode value, no migration, existing
notes keep decrypting.

**Capacity reality.** ~1 KB total blob, minus existing entries, minus container
overhead (salt + nonce + tag + header ≈ 60–90 bytes). This holds a handful of
short codes, not a vault. The UI states this plainly and the capacity meter
enforces the expectation.

#### C1 — passphrase mode (portable)

- Key = strong KDF(passphrase, salt): **Argon2id** (preferred) or scrypt, tuned
  for offline-attack resistance since the blob is world-readable.
- AEAD = **AES-256-GCM** or **ChaCha20-Poly1305** over the note text.
- **Portable by design:** the encrypted entry can be exported to a file, backed
  up, and imported into any key/device; the same passphrase decrypts. Matches
  the "travels between devices / lives on the hardware" use cases.
- **Threat note:** world-readable blob ⇒ a stolen key permits an *offline*
  brute-force. Passphrase strength and KDF cost are the only defense; the UI
  must say so and default the KDF cost high.

#### C2 — device-bound mode (additive)

- Fold in the FIDO2 **`hmac-secret`** extension (this key + PIN + touch) so
  decryption requires the physical key — removing the offline-attack path.
- **Composition rule (to finalize):** default to *passphrase AND device* — both
  required — so a stolen blob is uncrackable without the key, and a note is not
  silently single-factor. (Alternative "passphrase OR device" is more convenient
  but weakest-link; decision deferred to C2 design, but the container reserves
  space for either.)
- **Capability handling:** detect `hmac-secret` support; hide/disable the bind
  toggle where unsupported. keyroost already recognizes `hmac-secret`.
- **Tradeoff surfaced in UI:** device-bound notes are *not* portable and die
  with the key — correct for break-glass copies, wrong for sole-copy secrets.

### D — SSH-cert companion (finish what has no GUI)

Genuine interop with the Yubico/OpenSSH large-blob SSH-cert flow, cross-platform
for any key with a large blob.

- Implement CTAP **`largeBlobKey`** + per-credential **AES-256-GCM** so keyroost
  can *author and retrieve* SSH-cert entries that `fido2-token`/OpenSSH
  round-trip — not just display them (A already displays them).
- Store a CA-signed cert against its resident credential; retrieve/export it on a
  fresh machine. This is the "as Yubico intends" path, minus the Linux-only
  `fido2-token` friction.
- Smallest audience, largest build — hence last. A depends on none of this; D
  builds on A's SSH-cert recognition.

## Dependencies & conventions

- **New crypto dependency — needs the standard "vendor over depend" discussion.**
  C needs a KDF (Argon2id/scrypt) + an AEAD (AES-256-GCM / ChaCha20-Poly1305);
  D needs AES-256-GCM. Precedent exists: `keyroost-token2otp` already carries a
  scoped RustCrypto exception (`aes`, `cbc`, `sha2`, `p256`, `zeroize`). Options
  to decide at C1 design time: (a) extend that RustCrypto exception, (b) vendor
  the primitives in-tree as with SM4/SHA-1. `zeroize` for plaintext/secret
  buffers either way.
- **Writes stay checksum-safe and structural** via the existing
  `large_blobs.rs` path (re-read array, apply, recompute checksum, PIN-authed
  write). Encrypted/SSH entries are just new entry *types* in that same array.
- **Honesty in the UI is a feature.** Plain notes: "world-readable, not for
  secrets." Encrypted notes: state the offline-attack caveat (C1) and the
  not-portable/dies-with-key caveat (C2). Capacity meter always visible.

## Out of scope / separate work

- **Molto2 per-profile title (≤12 chars).** A field on the Molto2 device, *not*
  a FIDO2 large blob. Tracked separately in `TODO-v0.7.5.md`.
- **OpenPGP private DOs (0101–0104) and login-data (005E).** The OpenPGP-applet
  analog of arbitrary storage; smaller, PW-gated, gpg-ecosystem-specific. A
  possible future "device storage" story, not part of this large-blob roadmap.
- **PIV data objects (Printed Info / Discovery).** No natural free-text fit;
  everything PIV does is keys/certs. Skip.
- **Storing the SSH pubkey next to the resident key.** Neat but redundant —
  regenerable via `ssh-keygen -K`. Not worth a tier.
- **Nitrokey product integration.** A separate future track; noted so the
  cross-vendor direction is on record.

## Testing

- **A:** fixtures for each entry type (plain note, RP/AEAD opaque, OpenSSH cert);
  assert correct classification + field decode; export byte-exactness.
- **C:** known-answer vectors for the container (KDF + AEAD) so encryption is
  reproducible; round-trip encrypt→store→read→decrypt; wrong-passphrase and
  tamper (bad tag) rejection; capacity-exceeded handling.
- **D:** interop vectors validating a keyroost-written SSH-cert entry is
  retrievable by `fido2-token`/OpenSSH, and vice-versa.
- Any change to command/APDU construction keeps the existing known-answer suites
  green (project convention).

## Open decisions (to settle at each tier's design time)

1. KDF choice + cost defaults (Argon2id vs scrypt) and AEAD choice
   (AES-256-GCM vs ChaCha20-Poly1305) — C1.
2. Where the crypto lives: extend the RustCrypto exception vs vendor in-tree — C1.
3. C2 composition rule: passphrase-AND-device vs passphrase-OR-device.
4. Whether the "purpose preset" picker is per-key state keyroost remembers, or
   purely a per-action framing at add-time — A/C UX.
