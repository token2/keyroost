# MoltoUI: extension plan toward a general security-key manager

## Goal

Extend MoltoUI from its current single-purpose role (programming Token2
Molto2 TOTP tokens over PC/SC) into a general-purpose security-key
manager. The long-term feature target is rough parity with Yubico's
`ykman` GUI: FIDO2/U2F first, then OATH, PIV, OpenPGP, and OTP.

The shorthand for this scope is **C-lite**: start with FIDO2/U2F support
and grow from there.

## Current state (as of this branch)

- `molto2-proto` — pure-Rust Molto2 wire protocol (SM4, SHA-1, APDU, MAC).
- `molto2-transport` — PC/SC reader discovery and Molto2 session.
- `molto2-import` — Aegis / 2FAS / otpauth-list bulk import.
- `moltoctl` — CLI binary.
- `moltoui` — egui desktop GUI.

See `docs/PROTOCOL.md` and `CLAUDE.md` for the existing protocol layer.

## Naming policy

`molto2-*` crate names and `moltoctl` / `moltoui` binary names stay for
now. A rename to something neutral (e.g. `keytool-*`) happens once the
FIDO2 work is far enough along that the new identity is obvious — not
before.

## Phases

Sequenced smallest-to-largest. Each phase ends in a working binary; no
half-finished features carried across phase boundaries.

### Phase 0 — Device discovery
USB HID enumeration of FIDO devices via `/dev/hidraw*`. udev rules so an
unprivileged user can talk to FIDO keys. `moltoctl list` learns to show
both PC/SC readers and HID FIDO devices side-by-side.

Linux only at this stage; macOS/Windows are separate later phases.

### Phase 1 — FIDO2/U2F core transport
CTAP HID transport layer (frame assembly, channel `INIT`, channel
allocation), plus a minimal CBOR encoder/decoder. Implement
`authenticatorGetInfo` and `authenticatorReset`. Wire a "Security Keys"
pane into `moltoui` that lists connected keys and shows their CTAP info.

### Phase 2 — FIDO2 credential management
List / add / delete resident credentials (`credentialManagement`
subcommands). PIN set / change / verify.

### Phase 3+ — Reach toward ykman parity
OATH (overlaps with existing TOTP code), then PIV / OpenPGP / OTP in
subsequent phases. Each gets its own entry here when its turn comes.

## Dependency posture

`CLAUDE.md` mandates "vendor over depend." Restated here so context
compression doesn't lose it:

- HID enumeration: raw `/dev/hidraw*` ioctls. Whether we lean on the
  `nix` crate for ioctl plumbing or hand-write it is a Phase 0 decision.
- CTAP HID framing and CBOR: vendored in-tree.
- **No new heavyweight FIDO crates** (`authenticator`, `ctap-hid-fido2`,
  `fido-device-onboard`, etc.) without explicit discussion first.

## Non-goals (for now)

- Cross-platform support (macOS/Windows) before the Linux story works.
- Renaming the project off the `molto2-*` prefix.
- A web UI or background daemon.

## Deferred follow-ups (not blocking, revisit with hardware)

- **PIN protocol v2 wiring.** `pin.rs` already implements v2 (HKDF-derived
  split keys, random IV) but `client_pin.rs` hardcodes v1 in every request.
  Should negotiate from `getInfo.pinUvAuthProtocols` and prefer the device's
  first-listed protocol. Solo 2 reports `[v2, v1]` — v1 works, but we ignore
  the stated preference. Wire v2 through the command layer when convenient.
- **GUI worker thread.** All CTAP calls block egui synchronously; listing
  many credentials or running Reset (30s touch window) freezes the window.
  Offload to a thread + channel.
- **Reset in the GUI.** Currently CLI-only because of the touch-window
  blocking issue above.
- **CredentialManager double token fetch.** Unlock fetches the pinUvAuthToken
  twice because the manager consumes it; split the listing helpers off the
  manager or make the token `Clone`.
- **Bootloader-mode detection.** A Solo 2 in DFU enumerates as `1209:b000`
  and won't speak FIDO; detect and message clearly rather than hang on INIT.

## Hardware compatibility notes

- **Solo 2 / Solo 2A+** (Trussed firmware, Nitrokey-maintained): spec-faithful.
  Standard `credMgmt` (0x0A), 64-byte CTAPHID, reset = re-plug then touch
  within 30s (our `RESET_TIMEOUT` already handles this). USB IDs: app
  `1209:beee`, bootloader `1209:b000`. Firmware management uses a separate
  HID app + NXP ROM protocol, not CTAP2 vendor commands — out of our scope.
- **Nitrokey 3** shares the Solo 2 firmware stack; USB ID `20a0:42b2`.

## Working agreements

- All extension work happens on
  `claude/moltoui-security-key-integration-qm12e` until that branch
  merges; only then does new work go elsewhere.
- Don't push to remote without explicit user permission (per CLAUDE.md).
  Local commits and pushing to the working branch are fine; opening PRs
  is not, unless asked.
- This document is the durable anchor. When a session loses context, the
  next session should read `PLAN.md` first.
