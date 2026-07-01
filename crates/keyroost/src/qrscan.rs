//! Scan a TOTP QR code from the screen(s) (feature `qr`).
//!
//! Grabs the screen via the lightweight per-platform backend in
//! [`crate::screengrab`] and hands each frame to `keyroost-qr`, which shares the
//! same QR detection and otpauth parsing the file/paste import paths use — so
//! every input path runs one decoder and one parser. The only capability this
//! module adds is turning a captured screen into the first usable TOTP URI.
//!
//! Capture and decode happen locally — nothing is sent anywhere.

use crate::screengrab::{self, Capture};

/// Capture all screens and return the first `otpauth://totp` URI found, or an
/// error describing why nothing usable was scanned. The returned string is the
/// raw URI, to be fed to `keyroost_import::parse_otpauth` like a pasted URI.
pub fn scan_screens_for_otpauth() -> Result<String, String> {
    let captures = screengrab::capture_screens()?;
    if captures.is_empty() {
        return Err("no screens available to scan".into());
    }

    let mut found_any_qr = false;
    let mut saw_hotp = false;

    for capture in &captures {
        // Raw pixels decode directly; PNG frames reuse the file/paste decoder.
        let texts = match capture {
            Capture::Rgba {
                width,
                height,
                data,
            } => keyroost_qr::texts_from_rgba(*width, *height, data),
            Capture::Png(bytes) => keyroost_qr::texts_from_image(bytes),
        };
        let texts = match texts {
            Ok(t) => t,
            Err(_) => continue, // nothing decodable on this screen
        };
        for text in texts.iter() {
            let trimmed = text.trim();
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("otpauth://totp/") {
                return Ok(trimmed.to_string());
            }
            if lower.starts_with("otpauth://") {
                found_any_qr = true;
                if lower.starts_with("otpauth://hotp/") {
                    saw_hotp = true;
                }
            } else if !trimmed.is_empty() {
                found_any_qr = true;
            }
        }
    }

    if saw_hotp {
        Err("this is an HOTP code; only TOTP is supported".into())
    } else if found_any_qr {
        Err("a QR code was found, but it isn't a TOTP (otpauth://totp) code".into())
    } else {
        Err("no QR code found on screen — make sure the TOTP QR is visible".into())
    }
}
