//! Named-pipe IPC between the zarmount client, daemon, and elevated worker.
//!
//! Provides permissive security attributes (`D:(A;;GA;;;WD)S:(ML;;NW;;;LW)`)
//! ensuring processes across different integrity levels (Medium and High/Elevated)
//! can connect and communicate without access denied errors.

use std::io;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE, HANDLE,
    HLOCAL, LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ,
    FILE_SHARE_WRITE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::IO::OVERLAPPED;
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects};
use windows::core::PCWSTR;

/// The well-known name of the zarmount daemon pipe.
pub const PIPE_NAME: &str = r"\\.\pipe\zarmount";

/// Maximum size of a single message (command or response).
const MAX_MESSAGE: usize = 8192;

fn to_wide(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Permissive security descriptor wrapper:
/// - DACL: Generic All (`GA`) to Everyone (`WD`)
/// - SACL: Mandatory Label (`ML`) No-Write-Up (`NW`) at Low Integrity (`LW`)
///
/// This allows standard user processes (Medium integrity) and elevated worker
/// processes (High integrity) to connect with full read/write access.
struct PermissiveSecurityAttributes {
    sa: SECURITY_ATTRIBUTES,
    sd: PSECURITY_DESCRIPTOR,
}

impl PermissiveSecurityAttributes {
    fn new() -> Option<Self> {
        let sddl = "D:(A;;GA;;;WD)S:(ML;;NW;;;LW)\0";
        let sddl_u16: Vec<u16> = sddl.encode_utf16().collect();
        let mut sd = PSECURITY_DESCRIPTOR::default();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl_u16.as_ptr()),
                1, // SDDL_REVISION_1
                &mut sd,
                None,
            )
        };
        if ok.is_ok() {
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: false.into(),
            };
            Some(Self { sa, sd })
        } else {
            None
        }
    }

    fn as_ptr(&self) -> *const SECURITY_ATTRIBUTES {
        &self.sa
    }
}

impl Drop for PermissiveSecurityAttributes {
    fn drop(&mut self) {
        if !self.sd.0.is_null() {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.sd.0)));
            }
        }
    }
}

/// Connect to a named pipe and return the handle.
pub fn connect_pipe(pipe_name: &str) -> io::Result<HANDLE> {
    let name = to_wide(pipe_name);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    };
    match handle {
        Ok(h) if !h.is_invalid() => Ok(h),
        Ok(_) => Err(io::Error::other("invalid pipe handle")),
        Err(e) => Err(io::Error::other(e.to_string())),
    }
}

/// Creates a dedicated named pipe server for the elevated worker.
pub fn create_worker_server_pipe(pipe_name: &str) -> io::Result<HANDLE> {
    let name = to_wide(pipe_name);
    let sec_attr = PermissiveSecurityAttributes::new();
    let p_sa = sec_attr.as_ref().map(|s| s.as_ptr() as *const _);

    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            MAX_MESSAGE as u32,
            MAX_MESSAGE as u32,
            0,
            p_sa,
        )
    };
    if handle.is_invalid() {
        return Err(io::Error::other(format!(
            "CreateNamedPipeW failed: {}",
            io::Error::last_os_error()
        )));
    }
    Ok(handle)
}

/// Waits for the elevated worker process to connect to the server pipe, or
/// detects if the worker process exits prematurely before connecting.
pub fn wait_for_worker_connection(
    pipe_handle: HANDLE,
    process_handle: HANDLE,
    timeout_ms: u32,
) -> io::Result<()> {
    let event = unsafe { CreateEventW(None, true, false, None) }
        .map_err(|e| io::Error::other(e.to_string()))?;
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };

    let connect_res = unsafe { ConnectNamedPipe(pipe_handle, Some(&mut overlapped)) };
    if connect_res.is_ok() {
        let _ = unsafe { CloseHandle(event) };
        return Ok(());
    }

    let err = unsafe { windows::Win32::Foundation::GetLastError() };
    if err == ERROR_PIPE_CONNECTED {
        let _ = unsafe { CloseHandle(event) };
        return Ok(());
    }
    if err != ERROR_IO_PENDING {
        let _ = unsafe { CloseHandle(event) };
        return Err(io::Error::other(format!(
            "ConnectNamedPipe failed: {err:?}"
        )));
    }

    let wait_res =
        unsafe { WaitForMultipleObjects(&[event, process_handle], false, timeout_ms) };

    let _ = unsafe { CloseHandle(event) };

    match wait_res {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(io::Error::other(
            "Timed out waiting for elevated worker to start",
        )),
        _ => Err(io::Error::other(
            "Elevated worker process terminated unexpectedly without connecting",
        )),
    }
}

/// Connect to the named pipe and exchange a single command/response pair.
pub fn call(pipe_name: &str, command: &str) -> Result<String, Box<dyn std::error::Error>> {
    let name = to_wide(pipe_name);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| format!("cannot connect to zarmount daemon (is it running?): {e}"))?;

    let result = (|| -> Result<String, Box<dyn std::error::Error>> {
        let mut payload = command.as_bytes().to_vec();
        payload.push(b'\n');
        let mut written = 0u32;
        unsafe { WriteFile(handle, Some(payload.as_slice()), Some(&mut written), None) }
            .map_err(|e| format!("WriteFile failed: {e}"))?;

        let mut buf = vec![0u8; MAX_MESSAGE];
        let mut read = 0u32;
        unsafe { ReadFile(handle, Some(buf.as_mut_slice()), Some(&mut read), None) }
            .map_err(|e| format!("ReadFile failed: {e}"))?;
        buf.truncate(read as usize);

        let text = String::from_utf8_lossy(&buf).into_owned();
        let line = text.lines().next().unwrap_or("").to_string();
        Ok(line)
    })();

    unsafe {
        let _ = CloseHandle(handle);
    }

    result
}

/// Create a named-pipe instance and wait for a single client connection.
pub fn accept_client(pipe_name: &str) -> Result<HANDLE, Box<dyn std::error::Error>> {
    let name = to_wide(pipe_name);
    let sec_attr = PermissiveSecurityAttributes::new();
    let p_sa = sec_attr.as_ref().map(|s| s.as_ptr() as *const _);

    let handle = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            MAX_MESSAGE as u32,
            MAX_MESSAGE as u32,
            0,
            p_sa,
        )
    };
    if handle.is_invalid() {
        return Err(format!("CreateNamedPipeW failed: {}", io::Error::last_os_error()).into());
    }

    unsafe {
        let _ = ConnectNamedPipe(handle, None);
    }

    Ok(handle)
}

/// Read a single line of text from a connected pipe handle.
pub fn read_line(handle: HANDLE) -> io::Result<String> {
    let mut buf = vec![0u8; MAX_MESSAGE];
    let mut read = 0u32;
    unsafe { ReadFile(handle, Some(buf.as_mut_slice()), Some(&mut read), None) }
        .map_err(|e| io::Error::other(e.to_string()))?;
    buf.truncate(read as usize);
    let text = String::from_utf8_lossy(&buf).into_owned();
    Ok(text.lines().next().unwrap_or("").to_string())
}

/// Write a single line of text to a connected pipe handle.
pub fn write_line(handle: HANDLE, line: &str) -> io::Result<()> {
    let mut payload = line.as_bytes().to_vec();
    payload.push(b'\n');
    let mut written = 0u32;
    unsafe { WriteFile(handle, Some(payload.as_slice()), Some(&mut written), None) }
        .map_err(|e| io::Error::other(e.to_string()))?;
    Ok(())
}
