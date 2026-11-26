//! Build script: embeds the ZAR application icon into the Windows executable.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("../zarlib-gui/assets/application-x-zar.ico");
        res.compile()?;
    }
    Ok(())
}
