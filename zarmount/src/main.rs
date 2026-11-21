#[cfg(target_os = "linux")]
mod mount;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    use clap::Parser;
    match mount::run(mount::Options::parse()) {
        Ok(Some(signal)) => std::process::ExitCode::from(128 + signal as u8),
        Ok(None) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("zarmount: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("zarmount currently supports Linux only");
    std::process::exit(1);
}
