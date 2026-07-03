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
    assert_eq!(
        parse_public_data(&bad),
        Err(PublicDataError::BadOuterLength)
    );

    // Wrong inner tag.
    let mut bad = good.clone();
    bad[2] = 0x71;
    assert_eq!(parse_public_data(&bad), Err(PublicDataError::BadInnerTag));

    // Wrong inner length.
    let mut bad = good.clone();
    bad[3] = 0x1C;
    assert_eq!(
        parse_public_data(&bad),
        Err(PublicDataError::BadInnerLength)
    );

    // Trailing garbage after the body.
    let mut bad = good.clone();
    bad.push(0x00);
    assert_eq!(
        parse_public_data(&bad),
        Err(PublicDataError::BadOuterLength)
    );

    // Self-consistent forged outer length: buffer extended AND resp[1]
    // updated to match. Must still be rejected, never silently truncated.
    let mut bad = krprobe99_block();
    bad.extend_from_slice(&[0u8; 7]);
    bad[1] = (bad.len() - 2) as u8;
    assert_eq!(
        parse_public_data(&bad),
        Err(PublicDataError::BadOuterLength)
    );
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
