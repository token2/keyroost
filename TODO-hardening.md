# Hardening & UX TODO

Working list from the June 2026 security review follow-up. Items are ordered
by the sequence they're being implemented in, not priority. Checked items are
done and committed on this branch.

## Phase 1 — CI / release pipeline (config-only)

- [x] Dependabot: add `cargo` ecosystem (currently only `github-actions`)
- [x] Release: emit `SHA256SUMS` alongside the archives
- [x] Release: GitHub artifact attestation (build provenance) on published archives
- [x] CI: `cargo audit` job (RUSTSEC advisories) on lockfile changes + weekly

## Phase 2 — CLI / GUI quick wins

- [x] Warn when programming Molto2 seeds under the factory-default customer key
- [x] `info`: warn when device clock drifts >30s from host (suggest `sync-time`)
- [x] GUI: clear the seed draft field after a successful write
- [x] GUI: auto-clear clipboard ~45s after copying an OTP code

## Phase 3 — memory hygiene round 2

- [x] Zeroize `PinUvAuthToken` and the CTAP shared secrets on drop
- [x] Zeroize CLI-side secret strings (`read_secret` / `gather_secret` returns)
- [x] Zeroize imported TOTP seeds: `BulkEntry.secret` and `OtpAuth.secret`
      wipe on drop (with Debug redacted to the byte count), and the decrypted
      Aegis plaintext, GA migration buffers, and decoded QR payloads ride in
      `Zeroizing` wrappers end to end

## Phase 4 — documentation

- [x] `SECURITY.md`: threat model, the no-network-access invariant, secret
      handling guarantees, disclosure process
- [x] README: document Windows elevation requirements for FIDO HID access

## Phase 5 — CLI features

- [x] `completions` subcommand (shell completions via clap_complete)
- [x] `manpage` subcommand (troff output via clap_mangen)
- [x] `import-file --dry-run`: print the slot/title/config plan without
      touching the device — *already existed upstream; verified working*
- [x] `doctor` subcommand: diagnose pcscd, readers, udev rules, hidraw
      access, keys.json permissions
- [x] Destructive commands (`fido-reset`, `fido-creds-delete`,
      `factory-reset`): show the resolved friendly name + serial in the
      confirmation/refusal message

## Phase 6 — fuzzing

- [x] `fuzz/` crate with cargo-fuzz targets for the hand-rolled parsers:
      otpauth URI, base32, CBOR, OATH TLV, OpenPGP BER-TLV, PIV BER
- [x] Scheduled CI job running each target briefly (nightly toolchain)

## crates.io publish runbook (readiness verified 2026-06)

All crates carry version/license/description metadata and `cargo package`
succeeds for every crate without in-workspace deps; the rest resolve once
their deps are live (normal first-publish ordering). With a crates.io token
(`cargo login`), publish in this order, waiting ~a minute between tiers for
index propagation:

1. `keyroost-proto`, `keyroost-hid`, `keyroost-keyring`, `keyroost-rsakey`
2. `keyroost-ctap`, `keyroost-oath`, `keyroost-openpgp`, `keyroost-piv`,
   `keyroost-token2otp` (all leaf byte layers — no in-workspace deps),
   `keyroost-import`
3. `keyroost-transport` (needs proto/oath/openpgp/piv/token2otp), then
   `keyroost-resolve` (needs transport) and `keyroost-qr` (needs import)
4. `keyroostctl`, `keyroost`

Afterwards `cargo install keyroostctl` / `cargo install keyroost` work for
anyone with the Linux build prerequisites from the README.

## Deferred — decisions or external work needed

- [x] **QR code import** — done: keyroost-qr crate (rqrr/png/jpeg-decoder
      exception), PNG+JPEG screenshots, Google Authenticator migration
      batches, CLI `import --qr` / `import-file <image>`, GUI drag-drop,
      fuzz targets, end-to-end fixtures.
- [x] **Packaging** — automated fanout in .github/workflows/publish.yml
      (crates.io via OIDC trusted publishing, AUR, Homebrew tap, winget),
      templates + one-time setup steps in packaging/. Remaining manual:
      the account/secret setup and first publishes per packaging/README.md.
      Flatpak ruled out (pcscd/hidraw sandboxing).
- [x] **Branch/tag protection (light)** — repository rulesets: `v*` tag
      creation/update/deletion is admin-only (tag push is release
      authority), and `main` rejects force-pushes and deletion for
      everyone. Direct pushes to `main` remain allowed.
- [ ] **Branch protection (full)** — require PR + green CI for `main`.
      Deliberately deferred until the product is feature-complete and
      stable: it ends the direct-push workflow, so adopt it when release
      cadence slows.
- [x] **PIV write path** — DONE + hardware-verified (2026-06-12). Byte layer
      (GENERAL AUTHENTICATE, GENERATE, PUT DATA cert, CHANGE REFERENCE / RESET
      RETRY COUNTER, Yubico SET MGMT KEY / SET PIN RETRIES / GET METADATA /
      RESET, SPKI→PEM), transport (AES/3DES mutual management-key auth + all
      write ops, scoped aes/des/getrandom dep), CLI (`keyroostctl piv`
      change-pin/puk, unblock-pin, set-retries, change-management-key,
      generate-key, import-cert, export-cert, reset), and the full GUI PIV
      pane. Generalizes across PIV devices since it's a NIST standard.
- [ ] **Publish-channel accounts** — one-time setup per packaging/README.md
      before the first release: the `release-publish` environment approval
      gate, crates.io account + manual first publish + trusted-publisher
      grants, AUR account/SSH key + first `keyroost-bin` push, the Homebrew
      tap repo + `TAP_PUSH_TOKEN`, and the manual first winget submission +
      `WINGET_TOKEN`. Channels can be enabled one at a time; unset secrets
      skip cleanly.
- [x] **GUI: move slow imports off the frame loop** — QR decode, vault
      decrypt, and export parse run on a dedicated import thread (not the
      device worker, which serializes card I/O behind whatever runs on it);
      the dialog shows a spinner and blocks Load / Program all while one is
      in flight.
- [x] **Wayland clipboard clear** — documented in the README as best-effort
      on pure-Wayland sessions without XWayland clipboard sync (no complete
      fix known; wl-data-control is wlroots-only).
- [x] **CI cache for fuzz/audit jobs** — Swatinem/rust-cache (cache-bin
      covers the installed binaries) added to both workflows.
- [x] **Clipboard conditional clear** — done via arboard (already in the
      tree through eframe): clears only when the clipboard still holds the
      copied code; fails open if unreadable.

## v0.6.0 — CLI maturity & device-centric model (branch: `v0.6.0-cli-maturity`)

Holistic pass over `keyroostctl` (and the shared plumbing the GUI uses):
confirm the workflows make sense, dedup, fix the device-identification root
cause, and add the friendly device overview. **Breaking CLI changes** — done
deliberately now while pre-1.0 and the user base is small, landed as one
coherent release with a migration note.

Context: this follows two reader-name misidentification bugs (#21: a Token2
PIN+ then a PIN+R3 "3.2 mini" both mis-seen as a Molto2). v0.5.1 stopped the
bleed by matching only the "molto" product word; v0.6.0 replaces name-matching
with stable identifiers.

### Phase 0 — Command-surface inventory (DONE 2026-06-14; read-only)
Enumerated every `keyroostctl` command from clap: **61 leaf commands across 8
domains** + the bare invocation + 8 global flags. Map and findings below; the
two breaking-rename decisions it surfaced are recorded under Phase 3.

Surface (leaf-command counts):
- **Global/utility (5):** *(bare)* → currently Molto2 `info`, `info`, `list`,
  `doctor`, `completions`/`manpage`.
- **Molto2 (flat, 10):** info, set-seed, set-title, configure, sync-time,
  set-customer-key, import, import-file, probe, factory-reset.
- **FIDO (flat, 8):** fido-info, fido-reset, fido-pin-retries, fido-pin-set,
  fido-pin-change, fido-creds-metadata, fido-creds-list, fido-creds-delete.
- **key-name (nested, 3):** add, list, remove.
- **oath (nested, 6):** list, code, add, delete, set-password, clear-password
  *(Yubico/Trussed applet)*.
- **openpgp (nested, 10):** status, verify, public-key, reset, set-name,
  set-url, generate-key, import-key, sign, decrypt.
- **piv (nested, 12):** status, change-pin, change-puk, unblock-pin,
  set-retries, change-management-key, generate-key, import-cert, export-cert,
  request-cert, self-sign, reset.
- **otp (nested, 8):** list, get, add, delete, erase-all, serial, button-hotp,
  delete-button-hotp *(Token2 OTP-on-FIDO)*.

Findings:
1. **Three command shapes coexist** — Molto2 flat+un-namespaced (`set-seed`),
   FIDO flat+prefixed (`fido-creds-list`), the rest nested (`piv status`).
   Nothing tells a reader `set-seed` is Molto2-only.
2. **Four device-targeting idioms, none unified** — Molto2 implicit-single,
   FIDO `--path` (hidraw), OATH/OpenPGP/PIV `--reader <substr>` (PC/SC), OTP
   `--transport`. The friendly-name `--name` feeds **only** FIDO resolution, so
   a named key can't be targeted for piv/oath/openpgp. Real gap, not cosmetic.
3. **Secret-input flags re-declared per variant** — `<noun>-env`/`<noun>-stdin`
   is consistent but only OATH flattens it (`OathAccess`); OpenPGP and PIV
   repeat `--reader` + PIN flags inline everywhere. Dedup target for Phase 3.
4. **Verb drift** — "set" vs "change" for the same act; "reset" in four places.
5. **Confusable twins** — `oath`(Yubico) vs `otp`(Token2); `info` ≡ bare;
   `list` overlaps `key-name list`.
6. **`probe`** is a bring-up/research tool living in the main user surface —
   hide it (`#[command(hide = true)]` or a `dev`/`debug` namespace).
7. **No `--json` anywhere** — all human-text (→ Phase 4).
8. Bare-invocation Molto2-default confirmed at `main.rs:1411` (`Session::open`
   → `read_info`) — the `SW=6A81` wart, retired in Phase 2.

### Phase 1 — Shared device model
Lift the device-correlation logic (HID↔PC/SC pairing, capability union,
Molto2-vs-key classification) out of the GUI crate (`keyroost/src/ui/device.rs`)
into a shared library crate consumed by **both** GUI and CLI, so they stop
drifting. (This is a **new crate name** — its first crates.io publish must be
manual with the personal token, then add its Trusted Publishing entry, exactly
like `keyroost-token2otp`. Keep the personal token until v0.6.0 ships for this
reason; revoke afterward.) Replace reader-name Molto2 detection with stable identifiers:
USB PID (Molto2 = `0x0300`) and/or the architectural fact that the Molto2 is
the only Token2 device with no FIDO HID interface. **Depends on token2's answer
to the PID issue** (is `0x0300` always-and-only Molto2; canonical FIDO PID list;
`READ_CONFIG` appearance→model map). Fallback if no answer: keep "molto" name
match + a FIDO-HID-sibling cross-check.

### Phase 2 — Bare invocation + `list` redesign
Bare `keyroostctl` → friendly correlated overview (one row per physical device,
capability badges — GUI parity). `keyroostctl list` → repositioned as the
diagnostic dump, enriched with VID:PID + the computed classification (so the
next bug report hands us what My1's did, by design). Bare invocation rewired
exactly once, straight to the friendly form (no interim raw-list step).

### Phase 3 — Consistency pass (the breaking part)
**Decisions (locked from Phase 0, 2026-06-14):**
- **Nest every device under a named group.** FIDO flat → `fido <sub>`, *and*
  Molto2 flat → `molto <sub>` (full symmetry — every device is a group; the
  bare invocation is the only top-level entry that touches a device). So:
  `molto set-seed`, `fido creds list`, `piv status`, `oath list`, `otp list`.
  Top level keeps only the device-agnostic utilities (bare overview, `list`,
  `doctor`, `completions`, `manpage`) + the group names.
- **Unify device targeting fully.** One resolution path: `--name` (friendly)
  works for **every** group; `--reader <substr>` / `--path` / `--transport`
  become aliases/inputs into that one resolver (built on the Phase 1 shared
  device model), not parallel idioms. `--key*` stays Molto2-scoped.

Also: align verb/noun naming across groups ("set" vs "change"; the four
`reset`s become `<group> reset`), hide `probe` from the main surface, and dedup
shared plumbing — secret input (env/stdin), device resolution,
session-open-and-announce — extend the existing `open_piv` / `open_openpgp`
helper pattern to FIDO / OATH / Molto2 / OTP. Land all renames in one change
with a clear migration note (old → new command map).

### Phase 4 — Feature gaps
Per-device parity audit (esp. the Token2 OTP CLI merged in #24 — confirm it
covers enumerate / add / delete / config / button-HOTP). Evaluate a `--json`
output mode for scripting (everything is human-text today). Note any missing
per-device operations.

### Phase 5 — Bug sweep + hardware workflow walkthrough
Fresh per-device end-to-end pass on available hardware (YubiKey, Solo 2,
Molto2; Token2 FIDO via the vendor / @My1). The bare-invocation "is the device
plugged in?" wart is retired here as a side effect of Phase 2.

### Sequencing
Phases 0–2 are additive/safe; Phase 3 is where breaking renames land — keep
them in one change with a clear migration note. Ship v0.6.0 once all five are
done and walked through on hardware.
