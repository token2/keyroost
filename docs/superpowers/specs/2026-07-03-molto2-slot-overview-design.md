# Molto2 slot overview — titles, occupancy, per-slot delete

- **Date:** 2026-07-03
- **Status:** Design approved in brainstorming; hardware-probed on an attached
  Molto2 and cross-checked against the vendor's public tooling (protocol facts
  only; no code derived from it).
- **Scope:** Molto2 / Molto2v2 per-profile management surfaces: read-back of
  the per-profile public block (title, occupancy, TOTP config), title-only
  editing in the GUI, a slot listing in the CLI, and single-profile seed
  deletion. Supersedes the stale "Molto2 per-profile title" section of
  `TODO-v0.7.5.md`.

## What changed since the TODO was written

The TODO assumed titles were write-only ("no known plain command returns a
stored title"). Hardware probing disproved this:

- `80 41 00 <profile> 01 70` (INS `0x41`, P1 `0x00`, P2 = profile index,
  Case-3, data = one byte `0x70`) returns, **unauthenticated**, a
  `95 <len> 70 1D` TLV whose 29-byte body is:

  | offset | size | field |
  |---|---|---|
  | 0 | 1 | flag (observed `0x20`) |
  | 1 | 16 | **title, PLAINTEXT, zero-padded** |
  | 17 | 4 | time field A (device RTC per vendor docs; semantics verified live during implementation) |
  | 21 | 4 | time field B (last time-sync/config per vendor docs; same caveat) |
  | 25 | 1 | OTP algorithm |
  | 26 | 1 | time step (observed `0x1E` = 30) |
  | 27 | 1 | digit count (observed `0x06`) |
  | 28 | 1 | seed present (`00`/`01`) |

- Verified live: after `set_title` wrote SM4-ECB ciphertext to slot 99, the
  read returned `4B 52 50 52 4F 42 45 39 39` ("KRPROBE99") **in the clear** —
  the device decrypts on receipt and stores plaintext. Other slots stayed
  all-zero, proving P2 selects the profile.
- The Case-4 form (trailing Le) is rejected with `6F FB`; the command is
  Case-3 only.
- **Security consequence:** anyone with card access can read every slot's
  title and occupancy without a key. Both UIs state this; users must not put
  secrets in titles.

Also newly confirmed from the vendor tooling (MIT-licensed; consulted for
protocol behavior only, clean-room discipline maintained):

- **INS `0xE6` deletes one profile's seed**: bare `80 E6 00 <profile> 00` —
  a plain (non-MAC'd) command that is only accepted on a session that has
  completed the existing challenge/response authentication. Status `63 XX`
  signals auth failure with attempts remaining in SW2 (matches our existing
  `sw_auth_failed`).
- The vendor UI never reads titles back (it decodes only the seed-present
  byte); surfacing full read-back is net-new UX no other tool offers.

## Components

### 1. `keyroost-proto`

- `pub fn read_public_data(profile: u8) -> Command` — builds the Case-3 APDU
  above. Plain command (CLA `0x80`), no key material.
- `pub struct ProfilePublicData { pub flag: u8, pub title: Option<String>,
  pub time_a: u32, pub time_b: u32, pub algorithm: u8, pub time_step: u8,
  pub digits: u8, pub seed_present: bool }` and
  `pub fn parse_public_data(resp: &[u8]) -> Result<ProfilePublicData, ...>`:
  - strict on the envelope: leading tag `0x95`, nested tag `0x70`, length
    `0x1D`, body exactly 29 bytes before the status word; anything else is an
    error, never a guess.
  - title: strip trailing zero padding; empty-after-strip ⇒ `None`; decode
    as UTF-8 **lossily** (the device accepts arbitrary bytes; display must
    never fail). Titles feed terminal output, so the CLI applies the existing
    control-character flattening used for cert fields.
- `pub fn delete_seed(profile: u8) -> Command` — bare `80 E6 00 <profile> 00`.
  Doc comment states the session-auth precondition.
- Known-answer tests embed the two captured responses (all-zeros block;
  KRPROBE99 block) byte-for-byte, plus envelope-violation rejections
  (truncated body, wrong tags, wrong length, missing SW) and a non-UTF-8
  title case. `delete_seed` gets an exact-bytes KAT.

### 2. `keyroost-transport`

- `Molto2Session::read_public_data(&mut self, profile: u8) ->
  Result<ProfilePublicData, ...>` — transmit + parse; no auth required.
- `Molto2Session::delete_seed(&mut self, profile: u8) -> Result<(), ...>` —
  requires the session to be authenticated first (same flow the existing
  seed/title/config writes use); surfaces `63 XX` as the existing auth error.

### 3. CLI (`keyroostctl molto`)

- **`molto slots`** — reads all 100 profiles and lists them. Default: only
  slots that are occupied or titled; `--all` prints all 100. Columns: slot,
  occupied, title, algorithm, step, digits. `--json` emits the full parsed
  block per slot (additive new shape). No key needed.
- **`molto title -p N` with TITLE omitted** — reads and prints slot N's
  current title (clap: make TITLE optional; absent = read mode, no key
  needed; present = existing write path, unchanged).
- **`molto delete -p N --yes`** — authenticates (same key options as other
  molto writes), sends `0xE6`. Refuses without `--yes`. Prints the slot's
  title/occupancy first so the user sees what they are deleting.

### 4. GUI (Molto2 view)

- On view load (and after any write), a background job reads all 100 public
  blocks (~1–2 s, one APDU each) and the slot selector shows an occupied
  badge and the real title per slot. Failure of the sweep degrades to the
  current no-metadata list, never blocks the view.
- Per-slot actions, following the pane's existing job/confirm patterns:
  - **Edit title** — writes title only (auth handshake + `set_title`), no
    seed re-entry. Closes the parity gap where `apply_draft` bundles
    seed+title+config and demands the seed.
  - **Delete seed** — confirm-gated (armed button, same as other destructive
    GUI actions), then auth + `0xE6`, then refresh the slot's public block.
- Honesty copy in the view: titles and occupancy are readable by anyone
  holding the token, without a key.

### 5. Docs

- `docs/PROTOCOL.md`: add the `0x41` per-profile read (framing, body layout,
  Case-3-only note, unauthenticated-read security note) and `0xE6`
  (plain framing, session-auth precondition, `63 XX`).
- `TODO-v0.7.5.md`: replace the stale titles section with a pointer to this
  spec (the CLI title write already existed; read-back exists).

## Testing

- Proto KATs as in §1 — captured-bytes ground truth, strict-envelope
  rejections, existing known-answer suite stays green (repo convention).
- Hardware smoke with the attached Molto2:
  - `molto slots` shows slot 99 titled KRPROBE99, others empty;
  - GUI shows the same; edit slot 99's title from the GUI; confirm on the
    device screen and via `molto title -p 99` read-back;
  - seed slot 99 (safe slot per BRINGUP), confirm occupied appears; then
    `molto delete -p 99 --yes`, confirm seed-present drops — and observe
    whether the title survives deletion (unknown; whichever way it goes,
    document it in PROTOCOL.md);
  - wrong-key delete attempt surfaces the auth-failure message with
    attempts remaining.

## Out of scope

- Lock/unlock (`0xD8`) stays unimplemented (known mis-framing; separate
  hardware-probing project).
- The FIDO2 large-blob work (separate device concept entirely).
- Editing TOTP config per slot from the GUI beyond what exists today.

## Open items (settled during implementation, on hardware)

1. Semantics/endianness of the two 4-byte time fields (display only if
   clearly understood; omit from UIs otherwise).
2. Behavior of `0xE6` on an empty slot, and whether deletion clears the
   title.
3. Whether profile indexing anywhere needs the 1-based labels the device
   screen shows (UIs already label slots; keep wire index 0-based).
