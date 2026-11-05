//! Running a helper CLI (`docker`, `agy`) with a deadline and an output cap.

use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Apps opened from Finder get a bare `PATH`; give children the usual install locations.
pub const PATH: &str = "/usr/local/bin:/opt/homebrew/bin:/usr/bin:/bin";

/// Runs `command` and returns its stdout. `None` when it fails, prints more than `cap` bytes,
/// or is still running after `timeout` (it is then killed).
pub fn output(command: &mut Command, timeout: Duration, cap: usize) -> Option<String> {
    let child = command
        .env("PATH", PATH)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let pid = child.id();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(child.wait_with_output());
    });
    match receiver.recv_timeout(timeout) {
        Ok(Ok(output)) if output.status.success() && output.stdout.len() <= cap => {
            String::from_utf8(output.stdout).ok()
        }
        Ok(_) => None,
        Err(_) => {
            // SAFETY: plain signal to the child we spawned; the waiting thread reaps it.
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_stdout_and_enforces_limits() {
        let ok = output(
            Command::new("/bin/echo").arg("hi"),
            Duration::from_secs(5),
            64,
        );
        assert_eq!(ok.as_deref(), Some("hi\n"));
        assert_eq!(
            output(
                Command::new("/bin/echo").arg("hi"),
                Duration::from_secs(5),
                2
            ),
            None
        );
        assert_eq!(
            output(
                &mut Command::new("/usr/bin/false"),
                Duration::from_secs(5),
                64
            ),
            None
        );
        let slow = output(
            Command::new("/bin/sleep").arg("5"),
            Duration::from_millis(100),
            64,
        );
        assert_eq!(slow, None);
    }
}
