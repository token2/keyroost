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

## Working agreements

- All extension work happens on
  `claude/moltoui-security-key-integration-qm12e` until that branch
  merges; only then does new work go elsewhere.
- Don't push to remote without explicit user permission (per CLAUDE.md).
  Local commits and pushing to the working branch are fine; opening PRs
  is not, unless asked.
- This document is the durable anchor. When a session loses context, the
  next session should read `PLAN.md` first.
