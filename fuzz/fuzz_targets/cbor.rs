//! CTAP CBOR decoder — fed raw bytes from a potentially malicious device.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = keyroost_ctap::cbor::decode(data);
});
