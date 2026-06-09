//! Aegis / 2FAS / otpauth-list auto-detection path — attacker-supplied files.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = keyroost_import::parse_bulk_any(s);
        let _ = keyroost_import::aegis::is_encrypted(s);
    }
});
