//! Relaunch shim with administrator rights, for the one case where a game
//! folder is not writable. The current process exits after the new one is
//! confirmed by UAC; the user clicks Install again in the elevated window.

use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// Start an elevated copy of this executable. Returns an error message when
/// UAC was declined or the launch failed; the caller decides what to do.
pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let verb = wide(std::ffi::OsStr::new("runas"));
    let file = wide(exe.as_os_str());
    // SAFETY: all strings are NUL-terminated UTF-16 and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns an HINSTANCE-shaped value; > 32 means success.
    if (result as isize) > 32 {
        Ok(())
    } else {
        Err(format!(
            "ShellExecuteW(runas) failed with code {}; the UAC prompt was probably declined",
            result as isize
        ))
    }
}
