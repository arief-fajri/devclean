//! Running a fixed external command under a hard deadline (G-REL-05).
//!
//! Before Wave B, provider probes (`run_tool`, `go_env`, `pgrep`) spawned
//! processes with no timeout. Bounded in practice by `PROVIDER_SCAN_TIMEOUT`
//! returning to the caller, but a hung probe left a thread parked forever.
//!
//! The git calls Wave B adds need a deadline helper anyway, so this is the
//! single place every provider subprocess now goes through: fixed
//! `(program, args)` pairs only, stdout/stderr drained on side threads so a
//! chatty child cannot deadlock us, and the child killed when the deadline
//! fires. A missing program or a timeout is `None` — absence is normal, not
//! an error.

use std::io::{Read, Write};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Run `<program> <args>` and capture its output, killing the child if it
/// outlives `timeout`. `None` when the program cannot be spawned or the
/// deadline fires.
///
/// Never a shell, never user input: `program` and `args` are fixed strings
/// from providers (G-SAFE-02).
pub fn run_with_deadline(program: &str, args: &[&str], timeout: Duration) -> Option<Output> {
    run(program, args, None, timeout)
}

/// Like [`run_with_deadline`], but pipes `input` to the child's stdin and
/// closes it. Used by `git check-ignore --stdin -z`, which answers in one
/// call for every candidate path (G-REL-05, G-SAFE-02).
pub fn run_with_input(
    program: &str,
    args: &[&str],
    input: &[u8],
    timeout: Duration,
) -> Option<Output> {
    run(program, args, Some(input), timeout)
}

fn run(program: &str, args: &[&str], input: Option<&[u8]>, timeout: Duration) -> Option<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    // Feed stdin on a detached thread: `git check-ignore` reads until EOF,
    // and the writer must not block us if the child has already exited.
    if let Some(bytes) = input {
        if let Some(mut stdin) = child.stdin.take() {
            let bytes = bytes.to_vec();
            std::thread::spawn(move || {
                let _ = stdin.write_all(&bytes);
                // Dropping `stdin` closes the pipe → EOF for the child.
            });
        }
    }

    // Drain both pipes on side threads so a child that writes more than the
    // pipe buffer can never block us while we wait for it to exit.
    let out_handle = std::thread::spawn({
        let mut pipe = child.stdout.take();
        move || {
            let mut buf = Vec::new();
            if let Some(ref mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        }
    });
    let err_handle = std::thread::spawn({
        let mut pipe = child.stderr.take();
        move || {
            let mut buf = Vec::new();
            if let Some(ref mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        }
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Some(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_program_is_none_not_a_panic() {
        assert!(run_with_deadline(
            "devclean-definitely-missing-bin",
            &[],
            Duration::from_secs(5)
        )
        .is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_child_is_killed_at_the_deadline() {
        let t0 = Instant::now();
        let out = run_with_deadline("sleep", &["30"], Duration::from_millis(200));
        assert!(out.is_none(), "an overrun must be killed, not awaited");
        assert!(t0.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn a_fast_child_returns_its_output() {
        let out = run_with_deadline("printf", &["hello"], Duration::from_secs(5))
            .expect("printf must run");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hello");
    }

    #[cfg(unix)]
    #[test]
    fn stdin_is_piped_to_the_child() {
        let out = run_with_input("cat", &[], b"piped through", Duration::from_secs(5))
            .expect("cat must run");
        assert_eq!(String::from_utf8_lossy(&out.stdout), "piped through");
    }
}
