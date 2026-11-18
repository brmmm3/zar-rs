use crate::settings::ExtractionChecksum;
use std::path::Path;

impl ExtractionChecksum {
    pub fn recheck(self, archive: &Path) -> bool {
        match self {
            Self::Always => true,
            Self::Never => false,
            Self::Auto => match zarlib::operations::extraction_needs_checksum_recheck(archive) {
                Ok(recheck) => recheck,
                Err(error) => {
                    eprintln!(
                        "zarg: cannot determine filesystem for {}: {error}; rechecking block checksums",
                        archive.display()
                    );
                    true
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_modes_and_detection_failure() {
        let missing = Path::new("/nonexistent-zarg-archive");
        assert!(ExtractionChecksum::Auto.recheck(missing));
        assert!(ExtractionChecksum::Always.recheck(missing));
        assert!(!ExtractionChecksum::Never.recheck(missing));
        assert!(!ExtractionChecksum::Never.recheck(Path::new(".")));
    }
}
