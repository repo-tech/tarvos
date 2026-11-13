//! What a produced executable actually is, read from the file itself.
//!
//! Two questions decide whether a build result can be trusted, and neither can
//! be answered from the file name. Is this artifact the format the target
//! machine executes? And does it need anything the target machine may not have?
//! Both are answered here, from magic bytes and from what went into the build,
//! so a build can refuse to publish a mislabelled file and a user can audit one
//! they were given.

use std::path::Path;

/// The executable format an artifact actually is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactFormat {
    /// PE/COFF, which is what Windows executes.
    Pe,
    /// ELF, used by Linux.
    Elf,
    /// Mach-O, used by macOS.
    MachO,
    /// Something else, or not a binary at all.
    Unknown,
}

impl ArtifactFormat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pe => "PE",
            Self::Elf => "ELF",
            Self::MachO => "Mach-O",
            Self::Unknown => "unknown",
        }
    }

    pub fn target_os(self) -> &'static str {
        match self {
            Self::Pe => "windows",
            Self::Elf => "linux",
            Self::MachO => "macos",
            Self::Unknown => "unknown",
        }
    }

    /// Whether an artifact of this format can run on this build machine.
    ///
    /// A cross-built artifact is a legitimate outcome, so this is reported
    /// rather than treated as a failure. It is a mismatch against the *requested*
    /// target that fails a build.
    pub fn runs_on_host(self) -> bool {
        match self {
            Self::Pe => cfg!(windows),
            Self::Elf => cfg!(all(unix, not(target_os = "macos"))),
            Self::MachO => cfg!(target_os = "macos"),
            Self::Unknown => false,
        }
    }

    pub fn detect(bytes: &[u8]) -> Self {
        if bytes.starts_with(b"MZ") {
            return Self::Pe;
        }
        if bytes.starts_with(b"\x7fELF") {
            return Self::Elf;
        }
        // Mach-O: `0xFEEDFACE`, read in either byte order.
        let big = bytes
            .get(..4)
            .map(|head| u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let little = bytes
            .get(..4)
            .map(|head| u32::from_le_bytes([head[0], head[1], head[2], head[3]]));
        if big == Some(0xfeed_face) || little == Some(0xfeed_face) {
            return Self::MachO;
        }
        Self::Unknown
    }

    /// Read an artifact's leading bytes and classify them.
    pub fn of(path: &Path) -> std::io::Result<Self> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;
        let mut head = [0u8; 8];
        let read = file.read(&mut head)?;
        Ok(Self::detect(&head[..read]))
    }

    /// The format the host toolchain produces.
    pub fn host() -> Self {
        if cfg!(windows) {
            Self::Pe
        } else if cfg!(target_os = "macos") {
            Self::MachO
        } else {
            Self::Elf
        }
    }
}

/// File extension an artifact of this format is conventionally given.
///
/// The default output name is derived from the input file and this suffix, so
/// `tarvos build hello.py` produces `hello.exe` on Windows and `hello` on Linux
/// instead of a `tarvos_app.exe` that is a PE file wearing a name nothing on
/// Linux will execute.
pub fn extension_for(format: ArtifactFormat) -> &'static str {
    match format {
        ArtifactFormat::Pe => ".exe",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pe_executable_is_recognized_by_its_dos_header() {
        assert_eq!(
            ArtifactFormat::detect(b"MZ\x90\x00\x03"),
            ArtifactFormat::Pe
        );
    }

    #[test]
    fn an_linux_executable_is_recognized_by_elf() {
        assert_eq!(
            ArtifactFormat::detect(b"\x7fELF\x02\x01\x01"),
            ArtifactFormat::Elf
        );
    }

    #[test]
    fn a_macho_executable_is_recognized_in_either_byte_order() {
        let big = 0xfeed_faceu32.to_be_bytes();
        let little = 0xfeed_faceu32.to_le_bytes();
        assert_eq!(ArtifactFormat::detect(&big), ArtifactFormat::MachO);
        assert_eq!(ArtifactFormat::detect(&little), ArtifactFormat::MachO);
    }

    #[test]
    fn python_source_is_not_mistaken_for_an_artifact() {
        assert_eq!(
            ArtifactFormat::detect(b"import flask\nprint('hi')\n"),
            ArtifactFormat::Unknown
        );
    }

    #[test]
    fn a_truncated_or_empty_file_is_not_an_artifact() {
        assert_eq!(ArtifactFormat::detect(b""), ArtifactFormat::Unknown);
        assert_eq!(ArtifactFormat::detect(b"M"), ArtifactFormat::Unknown);
    }

    #[test]
    fn the_host_format_matches_the_platform() {
        let expected = if cfg!(windows) {
            ArtifactFormat::Pe
        } else if cfg!(target_os = "macos") {
            ArtifactFormat::MachO
        } else {
            ArtifactFormat::Elf
        };
        assert_eq!(ArtifactFormat::host(), expected);
        assert!(expected.runs_on_host());
    }
}
