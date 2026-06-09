//! PIV response parsers — device-supplied BER.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = keyroost_piv::unwrap_data_object(data);
    let _ = keyroost_piv::parse_version(data);
    let _ = keyroost_piv::parse_serial(data);
});
