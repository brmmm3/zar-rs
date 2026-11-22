#[cfg(not(target_os = "wasi"))]
use std::io;

pub(super) fn worker_priority() -> Option<u32> {
    let value = match std::env::var("ZARLIB_CPU_PRIORITY") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return None,
        Err(error) => {
            eprintln!("warning: invalid ZARLIB_CPU_PRIORITY: {error}");
            return None;
        }
    };
    match value.parse::<u32>() {
        Ok(0) => None,
        Ok(value) => {
            if cfg!(any(target_os = "linux", target_os = "macos", windows)) {
                Some(value)
            } else {
                eprintln!("warning: ZARLIB_CPU_PRIORITY is not supported on this platform");
                None
            }
        }
        Err(_) => {
            eprintln!(
                "warning: invalid ZARLIB_CPU_PRIORITY {value:?}; expected a nonnegative integer"
            );
            None
        }
    }
}

#[cfg(not(target_os = "wasi"))]
pub(super) fn apply_worker_priority(value: u32) {
    if let Err(error) = set_worker_priority(value) {
        eprintln!("warning: cannot apply ZARLIB_CPU_PRIORITY to worker thread: {error}");
    }
}

#[cfg(target_os = "linux")]
fn set_worker_priority(value: u32) -> io::Result<()> {
    // On Linux PRIO_PROCESS with who=0 addresses the calling thread.
    unsafe {
        *libc::__errno_location() = 0;
        let current = libc::getpriority(libc::PRIO_PROCESS, 0);
        if current == -1 && *libc::__errno_location() != 0 {
            return Err(io::Error::last_os_error());
        }
        let target = current.saturating_add(value.min(39) as i32).min(19);
        if libc::setpriority(libc::PRIO_PROCESS, 0, target) == -1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_worker_priority(_value: u32) -> io::Result<()> {
    if unsafe { libc::setpriority(libc::PRIO_DARWIN_THREAD, 0, libc::PRIO_DARWIN_BG) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn set_worker_priority(value: u32) -> io::Result<()> {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, GetThreadPriority, SetThreadPriority, THREAD_PRIORITY_IDLE,
        THREAD_PRIORITY_LOWEST, THREAD_PRIORITY_TIME_CRITICAL,
    };
    use windows_sys::Win32::System::WindowsProgramming::THREAD_PRIORITY_ERROR_RETURN;

    unsafe {
        let thread = GetCurrentThread();
        let current = GetThreadPriority(thread);
        if current == THREAD_PRIORITY_ERROR_RETURN as i32 {
            return Err(io::Error::last_os_error());
        }
        if current == THREAD_PRIORITY_IDLE {
            return Ok(());
        }
        let current = if current == THREAD_PRIORITY_TIME_CRITICAL {
            2
        } else {
            current
        };
        let target = current
            .saturating_sub(value.min(4) as i32)
            .max(THREAD_PRIORITY_LOWEST);
        if SetThreadPriority(thread, target) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(all(
    not(target_os = "wasi"),
    not(any(target_os = "linux", target_os = "macos", windows))
))]
fn set_worker_priority(_value: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "unsupported platform",
    ))
}

#[cfg(not(target_os = "wasi"))]
pub(super) fn worker_pool(
    workers: usize,
    name: &'static str,
    priority: Option<u32>,
) -> Result<rayon::ThreadPool, rayon::ThreadPoolBuildError> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .thread_name(move |index| format!("{name}-{index}"))
        .start_handler(move |_| {
            if let Some(priority) = priority {
                apply_worker_priority(priority);
            }
        })
        .build()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn priority_pool_lowers_workers_without_changing_caller() {
        let caller_priority = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        let pool = worker_pool(2, "priority-test", Some(5)).unwrap();
        let worker_priority = pool.install(|| unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) });
        assert_eq!(worker_priority, (caller_priority + 5).min(19));
        assert_eq!(
            unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) },
            caller_priority
        );
    }
}
