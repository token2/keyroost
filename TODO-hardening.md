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
      (Flatpak: **reconsidered for v0.7.0** — the earlier "ruled out" call was
      wrong; it's a solved pattern. See the v0.7.0 section.)
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

> **STATUS (2026-06-17): Phases 1-4 DONE and pushed.** P1 shared device model
> (`keyroost-resolve`), P2 bare overview + `list` correlated summary, P3 the
> breaking nested command tree (`molto`/`fido` groups + tidy renames, `--name`
> everywhere, per-group man pages, docs swept), P4 = Token2 **PR #30** folded in
> (fingerprint enroll `fido fingerprint-*`, FIDO MDS, on-device OTP — Token2
> authorship preserved) + `openpgp change-pin/change-admin-pin/unblock-pin` +
> global `--json` (13 query commands) + `token2_pid_label` on `list`. **NEXT =
> Phase 5** (hardware walkthrough / bug sweep on the new surface; devices on hand:
> YubiKey 5.7, Solo 2, Molto2 — no Token2 PIN+ bio key). **Then Phase 6** = bump
> workspace `0.5.1`→`0.6.0`, CHANGELOG `[Unreleased]`→`[0.6.0]`, final docs gaps.
> Deferred: Token2 PIN+ OATH/OpenPGP/PIV (standards applets) untested-by-us →
> experimental.

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
drifting. (**Decision 2026-06-16: extend the existing `keyroost-resolve` crate
rather than create a new one** — see Design below; this avoids a fresh crates.io
publish and the manual-token/Trusted-Publishing dance entirely.) Replace
reader-name Molto2 detection with stable identifiers:
USB PID (Molto2 = `0x0300`) and/or the architectural fact that the Molto2 is
the only Token2 device with no FIDO HID interface.

**Dependency RESOLVED — Token2 answered the PID issue (#25, 2026-06-15):**
- `0x349E:0x0300` is **always and only** the Molto2 and **will not change** —
  confirmed authoritative. This is now the primary detection signal.
- The full PID→product map is published and captured in code as
  `keyroost_proto::TOKEN2_PRODUCTS` (+ `token2_product`, `is_molto2_usb`,
  `token2_pid_label`). Token2 submits new PIDs to the CCID repo, so the table
  can grow; unknown PIDs fall through to "not provably a Molto2" → cross-checks.
- Token2 cautioned that the `READ_CONFIG` appearance field **can overlap** across
  products, so do **not** key on it — PID + product description is the contract.

Detection plan: where the USB PID is in hand (HID/USB enumeration), use
`is_molto2_usb(vid, pid)`. The bare PC/SC path only has a reader string, so keep
`is_molto2_reader` (name match) there, correlated to the USB side by topology
(the `CHANNEL_ID` bus/address pairing the transport already reads). Retain the
"no FIDO-HID sibling" architectural cross-check as defense in depth.

**ATR option (My1, #25/#21) — keep for the NFC future, verify before relying.**
My1 suggested classifying via the CCID **ATR** (as `pcsc_scan` does) rather than
the reader name. Honest assessment: it's the *right* tool over **NFC**, where
the reader is a generic contactless reader and neither USB PID nor reader name
denotes the card — if keyroost ever grows NFC support, ATR + AID-selection
becomes the only signal, so record it as the NFC strategy. For the **USB** case
it's weaker than the now-confirmed PID: (a) reading the ATR needs `SCardConnect`,
which resets the card — exactly the connect we avoid on the Molto2 during
enumeration; (b) the Token2 line may share a smartcard platform and present
indistinguishable ATRs across Molto2/FIDO — unverified, needs the 3.x hardware
Token2 offered (#21) before we'd trust it to discriminate. So: not the primary
USB discriminator, but a good cross-check and the clear NFC path.

**Design (locked 2026-06-16; hardware-verified against YubiKey 5 / Solo 2 /
Molto2 — see [[phase1-device-correlation-hardware]]).**

*Decisions (these override the older notes above):*
1. **Extend `keyroost-resolve`, no new crate.** It already owns the HID↔CCID
   serial correlation, already depends on hid/transport/keyring, and is already
   consumed by GUI + CLI. The change is additive (existing fns stay): no new
   crates.io publish, no rename, no new dependency edges between the frontends.
   Implementation must verify the dep graph stays acyclic (resolve →
   hid/transport/proto/keyring, never the reverse).
2. **Stable-ID classification, cross-platform, no sysfs.** FIDO keys carry a USB
   VID:PID from HID → classify by PID (`keyroost_proto::token2_product` /
   `token2_pid_label` for Token2 keys; VID for Yubico/Solo/Nitrokey). The Molto2
   is CCID-only (no HID; lsusb confirms `349e:0300` but it is not exposed over
   HID), so it is identified as **the Token2 CCID reader with no FIDO-HID
   sibling** — the #21 defense, since a Token2 PIN+ key always *has* a HID
   sibling and so can never be mis-seen as a Molto2. sysfs USB↔reader PID
   correlation is deferred as an optional Linux-only hardening; not needed now.
3. **One shared `Device` model, consumed by both frontends.** Today's
   `UiDevice` is already pure data (no egui) — move it (drop the `Ui` prefix)
   plus `Caps` and `DeviceKind` into keyroost-resolve. The GUI renders it, the
   CLI prints it; the friendly-name overlay (keyring) is applied in the shared
   layer so naming is identical in both.

*Shape (in `keyroost-resolve`):*
- Types: `Device { id, name, vendor, model, serial, transport, firmware, caps,
  kind, hid_path, reader }`; `Caps` (bitset FIDO2/OATH/PGP/PIV/TOTP/OTP);
  `DeviceKind { Key, Token }`.
- **I/O-vs-pure split (for testability):**
  - `enumerate(keyring: Option<&Keyring>) -> Result<Vec<Device>, String>` — does
    the I/O (HID enumerate + PC/SC probe), then calls the pure correlator.
    Resilient: a reader that fails or times out (cf. the Molto2 libccid init
    timeout) degrades to a partial/absent entry, never a hard error.
  - `correlate(hid: &[HidDevice], readers: &[ReaderProbe], keyring:
    Option<&Keyring>) -> Vec<Device>` — **pure, no I/O.** Pairs HID↔CCID (direct
    serial match → CCID-serial/topology → standalone), unions capabilities,
    classifies via the PID + no-sibling rules, applies friendly names. This is
    the unit-tested core.

*Correlation signals (observed on hardware):* direct serial match (Solo 2 —
serial in both the HID serial and the CCID reader name); CCID-serial/topology
(YubiKey — no HID serial, matched via the existing `ccid_serial_for`); standalone
CCID (Molto2) and standalone HID.

*Phase 1 deliverables:*
- D1 — `Device`/`Caps`/`DeviceKind` + `enumerate`/`correlate` in keyroost-resolve.
- D2 — PID + no-sibling classification replaces the reader-name-only Molto2 path;
  retire the ad-hoc `contains("token2")` / name-based `is_molto2` checks in
  `ui/device.rs`.
- D3 — GUI migrated onto `keyroost_resolve::Device` (its `ui/device.rs::enumerate`
  becomes a thin adapter or is removed; any egui-only per-device state stays in
  the GUI).
- D4 — unit tests for `correlate` with synthetic fixtures: the three real-hardware
  cases plus the no-sibling rule (Token2 reader + HID sibling ⇒ FIDO key, not
  Molto2).

*Out of scope (later phases):* CLI consuming `enumerate` for the bare/`list`
overview (Phase 2); unified `--name` targeting for every group (Phase 3); command
renames (Phase 3). Phase 1 only makes the crate CLI-ready (zero egui deps) and
migrates the GUI.

*Risks:* (a) dependency cycle if transport/hid already depend on resolve — verify
before moving; (b) `Device` must carry only hardware identity so the GUI's view
state stays separate; (c) the friendly-name overlay must reproduce today's
keyring behavior exactly.

### Phase 2 — Bare invocation + `list` redesign
Bare `keyroostctl` → friendly correlated overview (one row per physical device,
capability badges — GUI parity). `keyroostctl list` → repositioned as the
diagnostic dump, enriched with VID:PID + the computed classification (so the
next bug report hands us what My1's did, by design). Bare invocation rewired
exactly once, straight to the friendly form (no interim raw-list step).

**Design (locked 2026-06-16; format chosen with the user, builds on the Phase 1
shared model).**

*Decisions:*
1. **Bare overview = aligned columns.** One line per device:
   `vendor · model-or-name · cap badges · short serial · transport`, columns
   aligned (widths computed across the device list). Serial truncated to ~8 chars
   + `…` (the full serial lives in `list`); transport abbreviated
   (`USB · PC/SC + FIDO HID` → `USB·PC/SC+HID`). Empty → `No devices connected.`;
   an `enumerate()` error prints the message (e.g. HID failure), never panics.
2. **`list` = the raw 3 sections + a `Correlated devices` summary.** Keep PC/SC
   readers / applet probe / FIDO HID (lightly enriched: `token2_pid_label` on the
   HID line) so the *un-correlated* inputs stay visible, then append one line per
   physical device: `kind · vendor model · badges · the reader/HID it paired`.
   Reuses the **pure** `correlate(&hids, &probes, &keyring)` on the same hid+probe
   snapshot `run_list` already gathers — no double enumeration, one consistent
   snapshot (the payoff of Phase 1's I/O-vs-pure split).
3. **Shared badge vocabulary** = `Device::cap_badges() -> Vec<&'static str>` in
   keyroost-resolve (ordered FIDO2/OATH/PGP/PIV/OTP, or `["TOTP token"]` for a
   Token). Single source of truth; the **GUI pills adopt it too** → parity by
   construction, and it fixes the sidebar currently omitting the OTP badge on
   Token2 keys.

*Shape:*
- keyroost-resolve: add `Device::cap_badges()`.
- New `crates/keyroostctl/src/overview.rs`: `print_overview(&[Device])` (bare),
  `print_correlated(&[Device])` (the `list` section), and shared badge-line +
  column-width helpers. The pure formatters are separated from the thin `print_*`
  stdout wrappers so the formatting is unit-testable.
- `keyroostctl/src/main.rs`: bare dispatch (~1436) → `print_overview(enumerate()?)`
  (replacing the Molto2 `print_info`); `run_list` (~2113) keeps its raw sections
  and appends the correlated section.
- `keyroost/src/main.rs` (~4125): the pill loop switches to `dev.cap_badges()`.

*Deliverables:* D1 `cap_badges()` in resolve + GUI adoption; D2 `overview.rs`
formatters + bare rewire; D3 `list` correlated section; D4 unit tests
(`cap_badges`, the badge line, column alignment, serial truncation, the Token
"TOTP token" case, the empty list).

*Out of scope (later phases):* `--json` (Phase 4); unified `--name` targeting and
the command renames — including moving the Molto2 `info` to `molto info` (Phase
3). Until Phase 3, the Molto2 serial/UTC that bare invocation used to show stays
reachable via the existing `info` subcommand, so nothing is lost.

*Risks:* (a) the transport-abbreviation map must cover every `Device.transport`
value; (b) the GUI `cap_badges()` swap intentionally changes sidebar pills (adds
the OTP badge); (c) alignment must handle a friendly name standing in for the
model.

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
with a clear migration note (old → new command map). The README and every
`docs/*.html` page document the *current* flat surface, so they go stale the
instant these renames land — they must be updated in the same change (Phase 6),
not after.

**Carried from Phase 2 (small, fold into this pass):**
- `list` HID line: add the `keyroost_proto::token2_pid_label(pid)` product label
  for Token2 devices. The Phase 2 design said "lightly enriched", but only the
  raw VID:PID shipped (which already carries the bug-report value). Best verified
  with a Token2 PIN+ in hand.
- Reconcile the stale `enumerate(keyring: Option<&Keyring>)` line in the Phase 2
  design text above with the shipped no-arg `enumerate()` (it loads the default
  keyring internally).

**Design (locked 2026-06-17; decided with the user).**

*Naming style — "tidy" (shallow + consistent).* Nest the flat domains under
groups; drop now-redundant prefixes; converge the four resets to `<group> reset`;
shorten verbose Molto2 leaves; keep leaves hyphenated (no deeper sub-groups); keep
the change-vs-set distinction (change a PIN, set a field). Already-nested groups
(`piv`/`oath`/`openpgp`/`otp`/`key-name`) keep their leaf names. Full old→new map:

| Old | New |
|---|---|
| `info` / bare Molto2 info | `molto info` |
| `set-seed` / `set-title` / `configure` | `molto seed` / `molto title` / `molto config` |
| `sync-time` / `set-customer-key` | `molto sync-time` / `molto customer-key` |
| `import` / `import-file` | `molto import` / `molto import-file` |
| `factory-reset` | `molto reset` |
| `probe` | `molto probe` (`hide=true`) |
| `fido-info` / `fido-reset` | `fido info` / `fido reset` |
| `fido-pin-set` / `-pin-change` / `-pin-retries` | `fido pin-set` / `pin-change` / `pin-retries` |
| `fido-creds-list` / `-metadata` / `-delete` | `fido creds-list` / `creds-metadata` / `creds-delete` |

Unchanged: `piv`/`oath`/`openpgp`/`otp`/`key-name` subcommands; top-level
device-agnostic `list` / `doctor` / `completions` / `manpage` + the bare overview.
Bare `keyroostctl` stays the Phase-2 overview; the Molto2 serial/clock is now
`molto info`.

*Targeting — keep the flags, one resolver (user chose "keep").* `--name
<friendly>` works on **every** group, resolved through the Phase-1 device model.
The existing `--reader <substr>` (piv/oath/openpgp), `--path` (fido), `--transport`
(otp) **stay** as additional inputs into the SAME resolver — not parallel code
paths. `--key*` moves from global onto the `molto` group (Molto2-scoped). One
`resolve_target(selector, group)` built on `keyroost_resolve::enumerate()` returns
the right handle (reader name / hidraw path / Molto2 reader); the six `open_*`
helpers (`open_oath`/`open_openpgp`/`open_piv`/`open_piv_authed`/`open_otp` +
`resolve_fido_path`) call it — that shared resolve step is the dedup. `--name` and
`--path` stay mutually exclusive.

*Manual readability (the user's concern — addressed in this phase).* The nesting
itself is the main win: top-level `--help` drops from 27 flat commands to ~12
groups+utilities; `keyroostctl piv --help` shows only PIV. For the manual: make
`manpage` emit a **git-style set** — `keyroostctl.1` (overview + group list) plus
`keyroostctl-<group>.1` per group (`man keyroostctl-piv`), instead of one troff
blob, via clap_mangen's per-subcommand generation. Make `--name`/`--debug` global
so they document once, not on all ~60 leaves.

*Order — restructure first, docs as an end sweep.* (1) Restructure the clap tree
(nest/rename, hide `probe`, move `--key*`) + rewire dispatch. (2) Unify targeting
(`--name` everywhere) + dedup the `open_*` plumbing through `resolve_target`. (3)
**Documentation sweep against the finished tree:** the per-group `manpage`
generator; the README + every `docs/*.html` command example; a migration note
(old→new table) in the README and `CHANGELOG [0.6.0]`. Both man pages (generated
from clap) and the hand-written docs depend on the final names, so they land last,
together — satisfying Phase 6 for the rename surface.

*Testing.* clap `Command::debug_assert()`; parse tests (new paths parse, old flat
names are rejected, `--name` accepted on every group, `--name`+`--path` conflict);
`resolve_target` unit tests on the shared model; a man-page-generation smoke test
(a page per group, no panic). The known-answer / protocol-layer tests are
untouched (this is CLI surface only).

*Out of scope:* `--json` (Phase 4); the Token2 PIN+ experimental applet support
(Phase 4/5); the non-rename parts of the docs (Phase 6 proper) beyond the command
examples + migration note.

### Phase 4 — Feature gaps
Per-device parity audit (esp. the Token2 OTP CLI merged in #24 — confirm it
covers enumerate / add / delete / config / button-HOTP). Evaluate a `--json`
output mode for scripting (everything is human-text today). Note any missing
per-device operations.

**Token2 PIN+ Series — OATH / OpenPGP / PIV management is EXPERIMENTAL.** Token2
confirmed (#28) the PIN+ Series exposes OATH, OpenPGP, and PIV applets over CCID
on top of FIDO2 + the on-device OTP applet (keyroost-token2otp, done). Those
three are *standards* keyroost already speaks generically (`keyroost-oath` /
`-openpgp` / `-piv`), so **if** the PIN+ implements them with the standard AIDs
and protocols, keyroost should manage them with no new code — exactly as it does
a YubiKey or Nitrokey. But we have **no PIN+ hardware** to verify before v0.6.0
ships, so the README / supported-devices table must label PIN+ OATH/OpenPGP/PIV
as **experimental** until it's exercised on a real key (Phase 5 or a later point
release). Likely divergence vs the Yubico/Nitrokey path: the PIV management-key
crypto (AES vs 3DES) and any Yubico-specific PIV extensions (GET METADATA, SET
PIN RETRIES); OATH and OpenPGP are likelier to "just work" if standards-compliant.
**Action:** open a GitHub issue asking Token2 for the PIN+ applet AIDs, the PIV
management-key algorithm + whether the Yubico PIV extensions are supported, and
the OATH applet variant — enough to gauge how close their implementation sits to
the specs keyroost already targets.

**Landed (v0.6.0):** per-device parity audit done — the only gap was OpenPGP PIN
management, now fixed (`openpgp change-pin` / `change-admin-pin` / `unblock-pin`).
The `--json` query-subset shipped (status/info/list-style commands). PR #30 was
folded into v0.6.0 (Token2 PIN+ fingerprint/bio enrollment, FIDO MDS display,
on-device OTP rounding-out), all validated on real PIN+ hardware — so the
"experimental labeling / open-an-issue research" plan above is **superseded for
the FIDO / OTP / bio path**; only the OATH / OpenPGP / PIV smart-card applets
remain experimental (untested on PIN+ by this project).

### Phase 5 — Bug sweep + hardware workflow walkthrough
Fresh per-device end-to-end pass on available hardware (YubiKey, Solo 2,
Molto2; Token2 FIDO via the vendor / @My1). The bare-invocation "is the device
plugged in?" wart is retired here as a side effect of Phase 2.

### Phase 6 — Documentation sync (ships with the release, not after)
The user-facing docs currently describe an incomplete, soon-to-change product;
bring them level with reality before tagging. Concretely:
- **README is stale on Token2.** The "What it does" list, the "Supported
  devices" table, the Quick-start examples, and the GUI-tabs line all still
  frame the project around the Molto2 ("The original target") and omit the
  **Token2 FIDO security keys (T2F2 / PIN+)** entirely — even though on-device
  TOTP/HOTP for them shipped in 0.5.0 (the `otp` group) and #27 adds OTP over
  CCID, an interface enable/disable command, full-serial read, and a touch-HOTP
  GUI dialog. Add the device, its capabilities, and `otp` examples; the
  Contributors note already acknowledges the feature, so the body contradicts
  itself today. *(DONE — `eb6fd85`: Token2 FIDO key added to intro / "What it
  does" / devices table / quick-start, and the stale "PIV read-only" bullet
  corrected to full management.)*
- **Dependency accounting is stale — keep it honest, shrink it over time.** The
  "Vendor over depend" principle claimed the only external deps were
  `pcsc`/`clap`/`eframe`/`serde`/`rsa`, but the tree also pulls in RustCrypto
  (`sha2`/`hmac`/`aes`/`cbc`/`des`/`aes-gcm`/`scrypt`/`p256`), `hidapi` (macOS/
  Windows HID), `base64`, `zeroize`, and the QR stack (`rqrr`/`png`/
  `jpeg-decoder`) — and the workspace table omitted `keyroost-token2otp` and
  `keyroost-qr` outright. The project's stance is to add an external crate only
  when not doing so would be irresponsible (audited crypto we won't hand-roll) or
  impractical (platform glue), and to **reduce the count over time**. *(DONE —
  `eb6fd85`/this commit: principle reframed, per-crate dep column corrected, both
  missing crates added.)* Standing follow-up: revisit each dep at each release and
  drop any that in-tree code can replace.
- **Every command example must follow the Phase 3 renames.** README Quick-start
  + all `docs/*.html` use the flat `fido-*` commands and the bare-Molto2 `info` /
  `import` form; after Phase 3 these become `fido …` and `molto …`. Sweep
  `README.md`, `fido2.html`, `reset.html`, `molto2.html`, `index.html` (the
  already-nested `oath` / `openpgp` / `piv` / `key-name` examples are unaffected).
- **Migration note** (old → new command map) lands in the README and/or
  `CHANGELOG.md [0.6.0]`.
- **CHANGELOG `[0.6.0]`** entry written; **workspace version bumped** to 0.6.0
  (the branch does not bump it yet — still 0.5.1).
- The GitHub Pages site is served from `docs/` on `main`, so it goes live the
  moment this merges — there is no separate publish step to catch a lag.

### Sequencing
Phases 0–2 are additive/safe; Phase 3 is where breaking renames land — keep
them in one change with a clear migration note, and update the docs (Phase 6) in
that same change so the site never serves stale command syntax. Ship v0.6.0 only
once all phases are done, the docs are synced, the version is bumped, and the
release is walked through on hardware.

---

## v0.7.0 — release-pipeline fanout, device polish & packaging (branch: `v0.7.0`)

> **STATUS (2026-06-18): v0.6.0 SHIPPED** — tagged and live on crates.io + the
> GitHub Release. `v0.7.0` branch opened off `main`; `v0.6.0-cli-maturity`
> deleted. **Primary theme (user's call): stand up the rest of the
> package-manager release fanout.** The rest is deferred GUI/device work from
> v0.6.0 plus packaging research. Items are seeded, not yet scoped/locked —
> brainstorm + lock designs per item before building (same flow as v0.6.0).
>
> **Scoping note:** treat the list below as the v0.7.0 *envelope*, but
> deliberately **leave headroom** — (1) issues surfacing from the freshly-shipped
> v0.6.0 release get triaged first as they come in, and (2) a block is **reserved
> at the end of the cycle** for the review & hardening pass (§Z) before we ship.

> **OVERNIGHT PROGRESS (2026-06-20, autonomous run):**
> - **§B PIV GUI — DONE** (FIDO2-style slot sub-tabs, content-width cap, admin
>   folded into the status card; hardware-verified). **§E OpenPGP transport — DONE**
>   (INTERNAL AUTHENTICATE end-to-end, hw-verified).
> - **§I Docs reality sweep — DONE** (`docs-reality-sweep`, merged): README "What
>   it does" bullets, CHANGELOG `[Unreleased]`, `docs/piv.html` clear-slot; the
>   github.io site is **in-repo** (`docs/`, no gh-pages branch). #41 capability
>   matrix reconciled.
> - **§F Token2 PIN+ — ANALYSIS DONE, no code needed:** the OATH/OpenPGP/PIV
>   applets are vendor-neutral standards code (work on any card); "experimental"
>   is purely "untested on PIN+ hardware by us" — **needs a physical PIN+ pass
>   (user/Token2) to lift.** NB: #23 is a *closed PR*; live FIDO issue is **#33**.
> - **OpenPGP GUI** restyle to the FIDO2 vocabulary (key sub-tabs) — landing on
>   branch `gui-openpgp-fido2`, rebased onto this branch once gates pass.
> - **§D Packaging — DRAFTS DONE on branch `packaging-drafts`** (Flatpak self-host
>   manifest + desktop/metainfo, AppImage script, musl CLI runbook, `LINUX-BUNDLES.md`).
>   **NOT merged — has a 10-item maintainer-decisions list** (app-id, repo host,
>   icon, musl scope, runtime version, …). See the branch + the morning summary.
> - **DECISIONS QUEUED FOR USER:** PIN+ hardware verification; the packaging
>   decisions; whether to merge `packaging-drafts` (needs rebase onto current
>   `v0.7.0` first — it was based on the older `origin/v0.7.0`).

### A. Release-pipeline fanout — *primary theme*
First-time channel setup; the `publish.yml` jobs skip cleanly until each is
configured (see `packaging/README.md`):
- [x] **crates.io** — live via OIDC trusted publishing (0.5.x + 0.6.0).
- [x] **`release-publish` environment** — approval gate already in use (user
      approves each fanout).
- [~] **Homebrew** — tap repo `framefilter/homebrew-keyroost` **created
      (2026-06-18)**. Remaining: add `TAP_PUSH_TOKEN` (fine-grained PAT,
      `contents:write` on the tap repo only) + re-dispatch `publish.yml` for
      `v0.6.0` to write `Formula/keyroost.rb`.
- [~] **winget** — v0.6.0 manifests rendered (staged under `/tmp/winget/`).
      Remaining: add `WINGET_TOKEN` (classic PAT, `public_repo`) + submit the
      first PR to `microsoft/winget-pkgs`; future bumps auto-PR.
- [ ] **AUR** — **DEFERRED / BLOCKED.** Account registration is *disabled*
      during the June 2026 "Atomic Arch" AUR supply-chain incident (~1500
      hijacked packages; payload steals SSH keys + GitHub PATs — exactly the
      creds our CI auto-push would create). **Resume signal:** aur.archlinux.org
      no longer shows "Account registration is currently disabled" AND
      archlinux.org/news posts an all-clear. Then: account + dedicated SSH key,
      first `keyroost-bin` push, secret `AUR_SSH_PRIVATE_KEY`.

### B. PIV GUI — issue #31 items 4–6 (items 1–3 shipped in 0.6.0) — **COMPLETE**
- [x] Slot-first PIV view: pick a slot → see/act on its contents — **DONE**
      (`6f5986d`, My1's literal #31 item 4). The five per-operation slot dropdowns
      are gone; the status-card slot rows are now clickable (accent ▸ marker +
      tinted background, painted *under* the row content via a reserved shape;
      hover wash + pointing-hand). One `selected_slot` drives a "Selected: <slot>"
      detail panel (state line: cert/key/empty · algo · DN) with Generate / Create
      cert / Import / Export / Delete, all acting on that slot. `PivState` lost
      `gen/certify/cert/export/del_slot`, gained `selected_slot` +
      `PivSlotSel::from_slot`. GUI eyeball (row hit-testing) left for the user.
      **UX revision (`f748ce0`, user feedback):** two-column master/detail (slots
      left / actions right) claiming the real width; full-width slot click targets
      with generous padding; selection = tint + 3px accent left bar + brighter text
      (confusing ▸ marker removed); verbose notes → `?` help bubbles (`help.rs`
      gained `piv-generate/-certificate/-import/-export/-delete/-admin`); all
      applet-wide ops (PIN&PUK, retries, mgmt-key, reset) consolidated under one
      "Card administration" section (fixes the floating reset + scattered order).
      **Round 2 (`aa67d0d`→`16cb752`, user feedback "make it look like FIDO2"):**
      restyled to FIDO2's exact card vocabulary (full-width cards, right-pinned
      actions, f_sb/f_reg tokens); replaced the two-column rail with a **FIDO2-style
      slot sub-tab strip** (9A/9C/9D/9E as tabs, the layout the user actually wanted);
      added a global **content-width cap (~920px, centered)** fixing the wide-window
      label↔action gap across ALL panes; folded the applet admin (PIN&PUK/retries/
      mgmt-key) into the top "PIV smart card", Reset kept as a bottom red card
      (FIDO2-consistent). Per-slot wiring hardware-verified (ECC key in 9C → only
      that tab shows "key, no cert · ECC P-256"). **PIV GUI COMPLETE.**
- [x] Key/cert deletion — **Option B.** BACKEND DONE (`952c8f4`): byte layer
      (`delete_key` → `00 F6 FF <ref>`, `clear_certificate` = empty PUT DATA, KATs)
      + transport (version-gated `delete_key` ≥5.7, `clear_certificate`) + CLI
      (`piv delete-key` / `piv delete-cert`, `--yes`-guarded). **HARDWARE-VERIFIED
      on YubiKey 5.7.4:** delete-key proven via reject-on-empty (SW 6A88) +
      double-delete; delete-cert via status cert-present→empty. **GUI DONE**
      (`073cc2c`): a "Delete from slot" subsection (slot selector + Delete
      certificate / Delete key buttons) opens the credential modal with a
      destructive warning; key-delete gated on `PivStatus.version ≥ 5.7`;
      auto-refresh after. **OPTION B COMPLETE.** (L)
      *Minor follow-up:* CLI `piv status` is cert-based — doesn't show a
      key-without-cert; could add metadata-based key presence like the GUI. (S)
- [x] Native file-chooser dialogs — DONE (`6387ce9`): `rfd` with the
      **xdg-portal** backend (`default-features=false, features=["xdg-portal",
      "wayland"]`) — ~6 pure-Rust crates, **no GTK/zbus/ashpd** (verified), portal
      = Flatpak-ready, cross-platform. Async via a thread+channel off the frame
      loop. Browse… on OpenPGP/PIV cert+key imports, Save… on PIV CSR/cert export;
      text fields + drag-drop kept.
- [x] **Credential-entry modal + scroll-independent feedback** — *highest-leverage
      GUI fix, from @My1's #31 follow-up.* Built on #38's `modal_window` (FIDO
      aesthetic), auto-clears, shows the op result IN the modal.
      **PIV fully done:** Change-PIN/PUK/Unblock-PIN (`cd61287`) + all the
      management-key-gated ops (generate/import/self-sign/CSR/set-retries/
      change-mgmt-key, `f0aa05c`) now route secrets through the modal; the
      offscreen "Management key" card + inline sign-PIN/new-key fields are gone;
      "use default management key" toggle; secrets wiped per op (Approach A,
      bulk → CLI, documented in `50032c5`). **OpenPGP** (`be6d999`: PW1/PW3 change
      + unblock + all admin-PIN-gated write ops, use-default-PW3) and **OATH**
      (`ada77e2`: add-credential → modal; unlock stays an inline gate) likewise
      done. FIDO already modal (#38). **ROLLOUT COMPLETE** — My1's #31 core ask
      (scroll-independent credential entry + in-place results) delivered app-wide. (M)
- [x] **UI scale / "Text size" control (issue #42)** — DONE (`ff4597a`): egui
      global zoom — a top-bar slider 80–200% with a live %/Reset, `Ctrl +/−/scroll`
      keyboard zoom, persisted via eframe storage (default 100%, unchanged for
      existing users). Scales fonts + symbols + widgets uniformly. (FUTURE:
      broader theming/font customization — roadmap when picked up.)
- [ ] **Responsive layout at high zoom / narrow window** — at ~200% zoom in a
      partial-screen window, horizontal rows overflow and OVERLAP (top-bar Reset
      vs the brand; section-header right-actions over the left text). Fullscreen
      is fine. Fix = truncate/elide the left text in those header rows (egui
      `Label::truncate`) so the right action always has room + tidy/wrap the
      top-bar cluster; cheap partial = bump the min window width. Low-priority
      polish (edge case). (S–M)
- [x] **Bordered text inputs** — visible border on text fields so they don't
      blend into the dark theme — **landed via #38** (the `theme.rs` change),
      app-wide.

#### Candidate GUI items — from @My1's reviews (#31 + the #38 thread); NOT yet scoped
Captured so they're not lost; designs explored but not locked. Polish-first order.
The FIDO-settings items build on the FIDO2 tab that Token2's #38 introduces, so
their detailed design follows #38 landing.
- [x] **Show current `minPinLen`** — DONE (2026-06-19, `c8cda1a`): parse getInfo
      0x0D (+ 0x0C forcePINChange), shown in `fido info`, `--json`, and the GUI
      Settings view.
- [x] **Show security-policy state read-only without a PIN unlock** — DONE
      (`c8cda1a`): read-only "Security policy" summary in the pre-unlock view
      (alwaysUv / minPinLen / forcePINChange / enterprise-attestation), controls
      still gated behind unlock.
- [x] **Auto-lock on a post-unlock PIN error** — DONE (`4934f6e`): broad
      auto-lock across config + passkey + fingerprint ops on CTAP
      0x31/0x33/0x34, protecting the retry counter. Adversarially reviewed.
- [x] **Extend the scroll-independent credential-entry modal to PIV/OATH/OpenPGP**
      — DONE; this is the same work as the §B "Credential-entry modal" item above
      (PIV/OpenPGP/OATH all routed through `modal_window`). Kept here for the
      #31-thread cross-reference.

### C. Device identity — issue #37 (OnlyKey)
- [ ] OnlyKey-aware handling in `keyroost-resolve`: recognize `1d50:60fc` /
      product `ONLYKEY`, label it "OnlyKey", treat serial `1000000000` as a
      non-unique placeholder (fall back to the hidraw path). No OnlyKey on hand →
      verify with the reporter or a device. (Facts in the onlykey-device-facts note.)

### D. Packaging / distribution — issue #34 + the README "native installs" goal
- [ ] **Flatpak (tentative)** — RECONSIDERED: a solved pattern (Yubico
      Authenticator + KeePassXC ship on Flathub). Manifest needs `--socket=pcsc`
      (host pcscd) + `--device=all` (hidraw) + `--filesystem=/run/udev:ro`, and
      must bundle the pcsc-lite *client* lib (runtime lacks `libpcsclite`; ship
      the lib, drop the daemon). Verify end-to-end on hardware.
      **Distribution caveat:** Flathub has a strong stance against AI-generated
      code, and keyroost is openly AI-authored — so Flathub itself may be off the
      table. @errant253 (#34) suggests **self-hosting a Flatpak repo** (or using
      an alt repo like Fedora's) to keep Flatpak's distro-agnostic + auto-update
      benefits without the Flathub gate; that's the likely path. Note Token2
      already ships an AppImage of their OEM edition.
- [ ] **AppImage** — sandbox-free alternative worth weighing alongside Flatpak.
- [ ] **musl static Linux build** — UNDER CONSIDERATION (user deferred deciding).
      Fixes the glibc-version portability caveat; wrinkle is `libpcsclite`
      linking for the PC/SC path. Think it through before committing.

### E. OpenPGP transport completeness (code `TODO(transport)`) — **COMPLETE**
- [x] `INTERNAL AUTHENTICATE` (client/SSH auth signature) — **DONE end-to-end**
      (`032a300`): byte builder `internal_authenticate()` (case-4 `00 88 00 00
      <Lc> <data> 00`, KAT) + transport `OpenPgpSession::internal_authenticate()`
      + CLI `openpgp authenticate --in/--out --hash --pin-*` (PW1 in the *other*
      context `0x82`, `--out` via `write_private_file` 0600) + Auth slot already
      exposed for `generate-key`/`import-key`. **HARDWARE-VERIFIED on YubiKey 5.7**
      (2026-06-19): generated an RSA-2048 Auth key, signed a message, and the sig
      validates against the slot pubkey (PKCS#1 v1.5 block ending in the SHA-256
      DigestInfo). Both old `TODO(transport)` markers retired.
- [x] `GET RESPONSE` reassembly loop for `61xx`-chunked responses — **was already
      DONE** in `keyroost-transport` (`transmit_applet` drives it generically for
      every applet; secret-sticky). Only the stale byte-layer pointer comment
      remained; corrected in `032a300`.

### F. Token2 PIN+ standards applets — issue #23 (experimental)
- [ ] Verify OATH/OpenPGP/PIV over CCID on a real PIN+ key to lift the
      "experimental" label. Needs PIN+ hardware (Token2 may help, given the
      collaboration + their OEM edition).

### G. FIDO / CTAP — issue #33 / PR #38
- [x] CTAP 2.1 `authenticatorConfig` (`fido always-uv` / `set-min-pin` /
      `force-pin-change` / `enterprise-attestation`) + LargeBlob (GUI) + the
      FIDO2 tab redesign + bordered inputs — **MERGED via #38** onto v0.7.0
      (Token2-authored; the unintentional branding was left out). CLI commands
      are correctly nested under `fido`. **Hardware-verified on a YubiKey 5.7
      (2026-06-19):** getInfo read path, GUI redesign (branding confirmed gone),
      LargeBlob write→read round-trip, the `always-uv` config write, and
      capability-gating (Settings/Storage correctly hidden on a Solo 2 that
      lacks `authnrCfg`/`largeBlobs`).
- [x] **LargeBlob CLI parity** — DONE (`47f94b5`): `fido large-blob`
      list/get/add/edit/delete/clear with `--json`, re-read-before-write safety,
      `--yes` + opaque-entry warnings on delete/clear, world-readable help note.
      Reviewed (write-safety + destructive guards verified). Hardware test pending.
- [x] **LargeBlob survives `authenticatorReset` (privacy gap) — DONE.** Confirmed
      on a YubiKey 5.7 (2026-06-19): a FIDO reset wipes sign-in records/PIN but
      **not** the large-blob array — so the plaintext notes the Token2 feature lets
      you store persist after a "reset." keyroost issues the standard reset (correct;
      largeBlob-on-reset is vendor-specific). (a) clear in the CLI (`fido large-blob
      clear`, `47f94b5`); (b) GUI "Clear all storage" action (`ffb2f1b`):
      `clear_large_blob_storage()` writes an empty checksummed array via a
      Large-Blob-Write token, gated on entries>0 + unlocked, two-step Danger
      confirm; (c) note in the FIDO reset dialog (shown when the key supports
      largeBlobs) that reset does not wipe large-blob storage. GUI eyeball + the
      on-hardware clear (needs the user's FIDO PIN) left for the user. (S)
- [~] **Hardware-verify the config WRITE ops** — `always-uv` (reversible)
      verified on a YubiKey 5.7 (2026-06-19). Remaining (optional, low priority,
      one-way): `set-min-pin` + `enterprise-attestation` — exercise with a
      `fido reset` to recover when convenient.

### H. Protocol confirmation
- [ ] Confirm the Molto2 `read_info` 3-byte preamble + 2-byte separator are
      constant (docs/PROTOCOL.md) — the `keyroostctl probe` work item.

### I. Docs — README protocols/standards section (issue #41)
- [x] DONE (`e278f8c`): added a "Standards & protocols" README section (verified
      against the code) incl. the Token2 OTP SDK protocol link (#41 ask).
- [x] (orig) Expand the README's protocols/standards section to enumerate the published
      specs keyroost implements — at minimum the **vendor-specific** ones (Token2
      Molto2 protocol [already cited] + Token2 **On-Device OTP SDK protocol**,
      the #41 ask: https://github.com/token2/token2-otp-cli/blob/main/docs/Token2-OTP-SDK-Protocol.md),
      plus the standards (FIDO CTAP 2.1, OATH HOTP/TOTP RFCs, OpenPGP Card 3.4,
      NIST SP 800-73-4 PIV, SM4 GB/T 32907-2016, SHA-1 RFC 3174, base32/CBOR,
      etc.). Dispatched to a background agent (2026-06-19).

### Post-release triage (ongoing, reserve room)
- [ ] Watch for and triage issues from the v0.6.0 release as they arrive —
      regressions from the breaking command restructure, packaging/install
      reports, device-specific bugs. These take priority over the feature work
      above when they land.

### Z. End-of-cycle review & hardening — *reserve room before shipping v0.7.0*
A deliberate pass after the feature work and before the release, ideally run as a
multi-agent review:
- **Simplification** — collapse accidental complexity, dead code, and any
  duplication the v0.6.0 restructure left behind; keep files focused.
- **Security** — re-audit secret handling (PIN/seed zeroization, `--debug`
  redaction), device-driven loop/length bounds, and every parser against
  untrusted input (incl. the new `keyroost-piv::x509_parse` DER reader); refresh
  `cargo audit`.
- **Quality** — test-coverage gaps, error-path behavior, doc accuracy, and fuzz
  targets for any parser added this cycle.
- **Vendoring / dependency reduction** — revisit deps we couldn't avoid earlier
  and look for ways to drop or vendor them (the standing "shrink deps over time"
  goal): e.g. whether `serde_json` can be replaced by an in-tree emitter now that
  the `--json` shapes are fixed, and re-evaluate anything added this cycle (e.g.
  `rfd` if the file-chooser lands). Keep the audited RustCrypto + platform crates.

### Still deferred (not v0.7.0)
- Full branch protection (require PR + green CI on `main`) — adopt when release
  cadence slows (see the Deferred section above).

### Backlog — investigate (post-v0.7.0, after the planned work)
- [ ] **"Master reset" / key decommission** — one UI + CLI action that runs
      *every applicable* reset on a key in one go: FIDO `authenticatorReset`, PIV
      reset, OATH applet reset, OpenPGP terminate+activate, Molto2 factory reset,
      on-device OTP reset, and clear large-blob storage. Must be
      **capability-aware** — each key exposes a different subset, so it detects
      what's present and only resets those; maximally destructive → strong
      confirmation. **Research demand FIRST:** unclear who wants it. The obvious
      use case is decommissioning / recycling a key between users (e.g. an
      employer reissuing a returned key) — confirm that's a real practice worth
      the footgun before building. (Ties in the large-blob-clear item above.)
- [ ] **largeBlob SSH-certificate helper** — DEFERRED to a future release (NOT
      v0.7.0; user likes it but it's out of scope for now). The one real,
      vendor-emphasized use of the large-blob space (Yubico): store an SSH
      certificate alongside a resident FIDO2 SSH credential (the `ssh:` app
      binding, the `fido2-token`/`ssh-keygen` workflow), surfaced as readable
      cert metadata. Bigger than the generic notes CLI: needs the per-credential
      `largeBlobKey` via a getAssertion-with-extension flow + the CTAP large-blob
      AEAD (DEFLATE + AES-256-GCM) — likely 1-2 new deps (`aes-gcm`, a deflate
      crate), a dep-policy decision. Token2 frames their notes as a generic
      "scratchpad, not for secrets" (no use case); Nitrokey's largeBlob support
      is unreleased. Design separately when picked up.
- [ ] **UI liveness / "the app is working" feedback** — make it obvious to the
      eye that keyroost is *busy*, not *frozen/bugged*, during blocking work.
      Card I/O routinely stalls the visible UI for seconds — touch-required ops
      (sign/decrypt/authenticate, key-gen), on-card RSA generation, the ~30s
      reset re-insert window, PC/SC enumeration — and a static frame reads as a
      hang. Today there's a worker thread + a spinner on imports, but it's
      inconsistent and easy to miss. Figure out a coherent activity language:
      e.g. a global "working…" indicator while the device worker is busy,
      per-action inline spinners/disabled+busy button states, a subtle motion or
      progress cue on long ops, and a heartbeat so the window never looks dead.
      Keep it tasteful (over-animation reads as cheap/AI-generated — see the
      frontend-design notes). Research what feels alive vs. annoying before
      building. (Raised after the v0.7.0 GUI-polish pass; user considers the pane
      redesign itself done enough to gather community feedback.)
