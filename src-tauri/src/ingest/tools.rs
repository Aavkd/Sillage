//! The two binaries Sillage ships with, and how they are launched.
//!
//! ROADMAP phase 03, task 1: « Embarquer `ffmpeg.exe` en ressource Tauri ; **ne jamais dépendre
//! du PATH** ». That rule is what this module exists to make structural rather than
//! aspirational: [`Tools`] holds two absolute paths, every launch goes through
//! [`Tools::command`], and there is no code path anywhere that names a bare `ffmpeg`. A user
//! with a broken ffmpeg on their PATH, or none at all, must get exactly the behaviour we tested.
//!
//! Two Windows details that are not optional:
//!
//! - **`CREATE_NO_WINDOW`.** Without it every ingestion flashes a console window in front of the
//!   application. Tauri gives no warning about this; it is simply visible.
//! - **stdin closed.** ffmpeg reads stdin for interactive keys. Attached to a GUI process it can
//!   consume input meant for nobody and hang; `-nostdin` plus a null stdin removes the question.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::errors::IngestError;

/// Which binary. Carried as an enum so the error messages can name it without the caller
/// repeating a string literal at every site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Ffmpeg,
    Ffprobe,
}

impl Tool {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Ffmpeg => "ffmpeg",
            Self::Ffprobe => "ffprobe",
        }
    }

    /// `ffmpeg.exe` on Windows, `ffmpeg` elsewhere. The application is Windows-only, but the
    /// module has to build and be testable on whatever a contributor is using.
    #[must_use]
    pub fn file_name(self) -> String {
        format!("{}{}", self.name(), std::env::consts::EXE_SUFFIX)
    }
}

/// Where the bundled binaries are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

impl Tools {
    /// The pair sitting side by side in a folder — how Tauri lays resources out.
    #[must_use]
    pub fn in_directory(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref();
        Self {
            ffmpeg: directory.join(Tool::Ffmpeg.file_name()),
            ffprobe: directory.join(Tool::Ffprobe.file_name()),
        }
    }

    #[must_use]
    pub fn new(ffmpeg: impl Into<PathBuf>, ffprobe: impl Into<PathBuf>) -> Self {
        Self {
            ffmpeg: ffmpeg.into(),
            ffprobe: ffprobe.into(),
        }
    }

    #[must_use]
    pub fn path(&self, tool: Tool) -> &Path {
        match tool {
            Tool::Ffmpeg => &self.ffmpeg,
            Tool::Ffprobe => &self.ffprobe,
        }
    }

    /// Whether both binaries are where they are supposed to be.
    ///
    /// Read at startup so the library screen of phase 05 can say so, rather than letting the
    /// first dropped file be the thing that discovers a broken installation.
    #[must_use]
    pub fn are_present(&self) -> bool {
        self.missing().is_none()
    }

    /// The first missing binary, if any.
    #[must_use]
    pub fn missing(&self) -> Option<Tool> {
        [Tool::Ffmpeg, Tool::Ffprobe]
            .into_iter()
            .find(|tool| !self.path(*tool).is_file())
    }

    /// A command for `tool`, or the « installation incomplète » error.
    ///
    /// The existence check is here rather than at the call sites so that no launch can skip it,
    /// and so a missing binary reports itself as a missing binary instead of as the raw
    /// `NotFound` that `spawn` would otherwise return.
    pub fn command(&self, tool: Tool) -> Result<Command, IngestError> {
        let path = self.path(tool);
        if !path.is_file() {
            return Err(IngestError::ToolMissing {
                tool: tool.name(),
                path: path.to_path_buf(),
            });
        }

        // `Command::new` with an absolute path never consults the PATH. That is the whole point.
        let mut command = Command::new(path);
        command.stdin(Stdio::null());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            /// `CREATE_NO_WINDOW` — no console flashes in front of the application.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        Ok(command)
    }

    /// Runs a tool to completion and collects both streams.
    ///
    /// `Command::output` drains stdout and stderr concurrently; doing it by hand in sequence
    /// deadlocks as soon as the tool fills the pipe it is not being read from.
    pub fn run(&self, tool: Tool, args: &[&OsStr]) -> Result<Finished, IngestError> {
        let output =
            self.command(tool)?
                .args(args)
                .output()
                .map_err(|source| IngestError::ToolFailed {
                    tool: tool.name(),
                    source,
                })?;

        Ok(Finished {
            succeeded: output.status.success(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// What a finished tool left behind.
#[derive(Debug)]
pub struct Finished {
    pub succeeded: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The bundled binaries, if `scripts/fetch-resources.ps1` has been run.
    ///
    /// Tests that need to really decode something go through here and say so when they skip:
    /// a silent skip on a fresh clone is how a phase gets declared verified without having been.
    pub(crate) fn bundled() -> Option<Tools> {
        let tools = Tools::in_directory(Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"));
        if tools.are_present() {
            Some(tools)
        } else {
            eprintln!(
                "IGNORÉ : {} est absent. Lancer scripts/fetch-resources.ps1.",
                tools.path(Tool::Ffmpeg).display()
            );
            None
        }
    }

    #[test]
    fn a_directory_gives_both_binaries_side_by_side() {
        let tools = Tools::in_directory(r"C:\Program Files\Sillage\resources");
        assert!(tools.path(Tool::Ffmpeg).ends_with(Tool::Ffmpeg.file_name()));
        assert!(tools
            .path(Tool::Ffprobe)
            .ends_with(Tool::Ffprobe.file_name()));
        assert_eq!(
            tools.path(Tool::Ffmpeg).parent(),
            tools.path(Tool::Ffprobe).parent()
        );
    }

    #[test]
    fn the_paths_used_are_absolute_and_never_a_bare_name() {
        // The acceptance criterion « aucun appel à un ffmpeg du PATH » rests on this: a relative
        // or bare name is exactly what would let Windows search the PATH.
        let tools = Tools::in_directory(r"C:\Program Files\Sillage\resources");
        for tool in [Tool::Ffmpeg, Tool::Ffprobe] {
            let path = tools.path(tool);
            assert!(path.is_absolute(), "{} is not absolute", path.display());
            assert!(path.parent().is_some_and(|parent| parent != Path::new("")));
        }
    }

    #[test]
    fn a_missing_binary_is_named_rather_than_searched_for() {
        let dir = tempfile::tempdir().expect("tempdir");
        let tools = Tools::in_directory(dir.path());

        assert!(!tools.are_present());
        assert_eq!(tools.missing(), Some(Tool::Ffmpeg));

        let error = tools.command(Tool::Ffmpeg).expect_err("must refuse");
        assert_eq!(error.kind(), "outil-manquant");
        assert!(error.to_string().contains("ffmpeg"));
        assert!(error.to_string().contains("réinstallez"));
    }

    #[test]
    fn a_half_installed_pair_reports_the_one_that_is_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(Tool::Ffmpeg.file_name()), b"x").expect("write");

        let tools = Tools::in_directory(dir.path());
        assert!(!tools.are_present());
        assert_eq!(tools.missing(), Some(Tool::Ffprobe));
    }

    #[test]
    fn the_bundled_pair_is_present_and_answers() {
        let Some(tools) = bundled() else { return };

        assert!(tools.are_present());
        assert_eq!(tools.missing(), None);

        let version = tools
            .run(Tool::Ffmpeg, &[OsStr::new("-version")])
            .expect("run");
        assert!(version.succeeded);
        assert!(String::from_utf8_lossy(&version.stdout).contains("ffmpeg version"));
    }

    #[test]
    fn a_tool_that_fails_reports_its_stderr_rather_than_panicking() {
        let Some(tools) = bundled() else { return };

        let output = tools
            .run(Tool::Ffprobe, &[OsStr::new("aucun-fichier-de-ce-nom.m4a")])
            .expect("run");
        assert!(!output.succeeded);
        assert!(!output.stderr.is_empty());
    }
}
