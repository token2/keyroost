//! Piping keyroostctl's stdout into a reader that closes early (like
//! `keyroostctl … | head`) must exit quietly, not dump a panic + backtrace.
//!
//! This guards the broken-pipe panic hook in `main()`
//! (`install_broken_pipe_guard` / `is_broken_pipe_panic`). `completions bash`
//! panics through clap_complete's `Debug`-formatted `io::Error`, so this test
//! exercises that shape end-to-end; the std `println!` `Display` shape is
//! covered by the `broken_pipe_panic_detection` unit test in `main.rs`. If the
//! guard regresses, this fails loudly instead of the fix silently breaking.

use std::io::Read;
use std::process::{Command, Stdio};

#[test]
fn broken_pipe_exits_without_panicking() {
    // `completions bash` writes a long script to stdout and needs no hardware.
    let mut child = Command::new(env!("CARGO_BIN_EXE_keyroostctl"))
        .args(["completions", "bash"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn keyroostctl");

    // Read one small chunk, then drop our read end — mimicking `head` closing
    // the pipe. The child's next write then hits a closed pipe.
    {
        let mut out = child.stdout.take().expect("child stdout");
        let mut buf = [0u8; 32];
        let _ = out.read(&mut buf);
        // `out` drops here, closing the read end.
    }

    let output = child.wait_with_output().expect("wait for child");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !stderr.contains("panicked"),
        "a closed output pipe produced a panic on stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("Broken pipe"),
        "a closed output pipe leaked a broken-pipe error to stderr:\n{stderr}"
    );
    // The guard exits 141 (128 + SIGPIPE) on a closed pipe — the conventional
    // broken-pipe status. Without the guard the worker panics and main returns
    // ExitCode::FAILURE (1), so this also distinguishes "guarded" from "panicked".
    assert_eq!(
        output.status.code(),
        Some(141),
        "expected exit 141 on a closed pipe, got {:?}\nstderr:\n{stderr}",
        output.status
    );
}
