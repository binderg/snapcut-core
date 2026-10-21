//! Keep this process's stdio out of the daemon it spawns.
//!
//! Windows children inherit every inheritable handle of their parent, not just
//! the stdio they are given. Our stdout is often a pipe someone is reading to
//! EOF (an agent's shell, `ve summary | cat`, a test harness). A daemon that
//! inherited it would hold that pipe open until it exits, so the caller would
//! hang for the whole idle timeout. Stable Rust has no safe way to stop the
//! inheritance, hence the one `unsafe` call in the workspace (ve-cli uses
//! `deny` instead of `forbid` for it). Unix needs nothing: the child's 0/1/2
//! are replaced and std opens everything else close-on-exec.

/// Mark stdin, stdout and stderr non-inheritable. Call before spawning the daemon.
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn keep_from_children() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    let handles = [std::io::stdin().as_raw_handle(), std::io::stdout().as_raw_handle(), std::io::stderr().as_raw_handle()];
    for h in handles {
        if h.is_null() {
            continue;
        }
        // SAFETY: SetHandleInformation only changes a flag on a handle this
        // process owns for its lifetime (a std handle). An invalid handle makes
        // the call fail, which is harmless: we ignore the result, since the
        // worst case is the hang described above, not memory unsafety.
        unsafe {
            SetHandleInformation(h, HANDLE_FLAG_INHERIT, 0);
        }
    }
}

#[cfg(not(windows))]
pub fn keep_from_children() {}
