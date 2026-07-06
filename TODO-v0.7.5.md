# v0.7.5 TODO

Deferred items that are too large or too risky to fold into a same-day patch.
Captured here so they don't get lost. Unchecked = not started.

## Release-day playbook (SOP)

- [ ] Write a **release-day playbook** — a single checklist doc the release
      runs from, start to finish. Releases so far have been ad-hoc; as the UI
      matures the cut needs to be boring and repeatable. Should cover, in
      order, at least:
  - pre-flight: clean main, CI green, `cargo audit` green, deps-outdated scan
    reviewed;
  - **packaging test branch**: flatpak + AppImage built green BEFORE the tag
    (rule already in CLAUDE.md "Release process");
  - new-crate check: any crate added since the last release needs its one-time
    manual `cargo publish` + Trusted Publishing entry first;
  - version bump + changelog + tag (signed, `v*` tags are admin-only);
  - `release.yml` → `publish.yml` fanout watch (GH Release artifacts, crates.io
    OIDC job, env approval);
  - **signed binaries (manual, Token2)**: Token2 signs the Windows + macOS
    builds on their DigiCert hardware token, which **cannot be automated** in
    CI (physical token access required; see #77). So after the release is cut,
    obtain their signed builds, attach them as the recommended Windows/macOS
    assets, and keep keyroost's own attested CI builds alongside — label
    signed-vs-attested in the release notes so users know who signed what.
    Folds into the pipeline if/when keyroost gets its own signing identity.
  - post-release: install-matrix spot check (`cargo install`, flatpak,
    AppImage, Homebrew tap, winget manifest refresh), GUI/CLI version sanity;
  - announcement/notes if any.
- [ ] Decide where it lives (likely `packaging/RELEASING.md`) and whether any
      steps can become a workflow-dispatch dry-run instead of prose.

## PC/SC: load libpcsclite at runtime, degrade gracefully (the real #47 fix)

- [ ] Stop hard-linking libpcsclite; **`dlopen` it at runtime** in
      `keyroost-transport` (and wherever the `pcsc` crate is used), so:
  - the **host's** libpcsclite is always used — the only client guaranteed to
    match the host's `pcscd` daemon (fixes the version-mismatch root cause of
    [#47](https://github.com/framefilter/keyroost/issues/47) for **every**
    distribution channel, not just the AppImage); and
  - when libpcsclite is **absent**, keyroost still launches and FIDO/USB-HID
    keeps working — the PC/SC panes show a clear "PC/SC unavailable" state
    instead of the binary failing to start.
- [ ] This **removes the AppImage limitation** noted in
      `packaging/appimage/build-appimage.sh` (the 0.7.x AppImage drops the
      bundled libpcsclite and so needs the host's to even launch).
- [ ] The `pcsc` crate links at build time; check whether it exposes a
      dlopen/dynamic-load path or whether we wrap libpcsclite via a thin FFI
      loader ourselves. Design before implementing; verify on a host WITH and a
      host WITHOUT libpcsclite.

## egui / eframe version bump

- [ ] Bump **egui / eframe / egui-winit 0.29.1 → 0.34.3** (current latest).
      Five minor versions of breaking API changes across the ~11k-line GUI —
      treat as its own project with a full pass + regression check (zoom/slider,
      modals, layout, light/dark themes).
- [ ] **winit stays 0.30.13** either way (0.31 is beta only; egui 0.34 still
      rides the 0.30.x line), so this is **not** guaranteed to fix the Wayland
      text-input regression in
      [#48](https://github.com/framefilter/keyroost/issues/48) — but check
      whether egui-winit's glue changes incidentally resolve it on Fedora-44 KWin
      while we're here.

## Molto2 — slot overview (titles, occupancy, per-slot delete)

Superseded by `docs/superpowers/specs/2026-07-03-molto2-slot-overview-design.md`
and its implementation plan. The old read-back assumption here was wrong:
hardware probing found `80 41 00 <profile> 01 70` returns title, occupancy,
and config in the clear (no key), and `80 E6 00 <profile> 00` deletes a
seed keylessly. Wire format now in `docs/PROTOCOL.md`.

## Hygiene follow-ups from the slot-overview branch

The user reviewed the branch's review findings and chose which to fix now vs
defer. Fixed on-branch: serial sanitization in the refusal messages, the
PROTOCOL empty-slot note, and the GUI slot-list refresh (factory reset clears
the stale list; a write re-sweeps when the list was blank). Promoted to its
own follow-up branch: the EPIPE panic. Remaining deferred items below.

- [ ] `impl std::error::Error for PublicDataError` so
      `TransportError::PublicData` chains via `source()` like its
      OATH/OpenPGP siblings.
- [ ] `molto slots`: on a mid-sweep read failure, print the slots already
      read plus an error row instead of aborting the whole command.
- [x] Repo-wide: keyroostctl panicked on EPIPE when stdout was piped to
      `head`/early-closing consumers. **Fixed** on `fix/cli-broken-pipe` via a
      panic hook that intercepts the broken-pipe panic and exits 0, guarded by
      `tests/broken_pipe.rs`. See the stabilization-watch item below for the
      cleaner replacement.
- [ ] **Watch for stable Rust to land the SIGPIPE fix and swap the workaround
      out.** The clean fix — libstd resetting `SIGPIPE` to `SIG_DFL` itself, no
      `unsafe` and no dep in our code — exists only on nightly today as the
      `-Zon-broken-pipe=kill` compiler flag (formerly the `#[unix_sigpipe]`
      attribute; tracking issue rust-lang/rust#97889, Unstable Book:
      `compiler-flags/on-broken-pipe`). When it (or an equivalent) reaches
      **stable**, delete `install_broken_pipe_guard()` in `keyroostctl/src/main.rs`
      and its `tests/broken_pipe.rs` guard, and adopt the built-in. Check
      periodically — it will leave nightly eventually. (Same applies to the
      `keyroost` GUI binary if it ever grows piped stdout.)
- [ ] GUI (optional, user's call): an explicit "Refresh slots" control by the
      slot-list header for on-demand re-read — deferred to avoid worsening the
      already-crowded six-button action row.

## GUI — Text-size control polish ([#42](https://github.com/framefilter/keyroost/issues/42), @token2)

- [ ] Add discrete **"−" / "+" buttons** on the ends of the zoom slider; mouse
      dragging is unpredictable near the boundaries.
- [ ] **Light theme:** the slider track/handle is almost invisible — restyle it
      so it reads on the light palette (it's currently tuned for dark only).
