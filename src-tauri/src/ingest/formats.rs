//! What Sillage accepts, in one place.
//!
//! ROADMAP phase 03 task 2 fixes the list: audio (m4a, mp3, wav, flac, ogg, opus, aac, wma) and
//! video (mp4, mov, mkv, avi, webm), the video files having their audio track extracted.
//!
//! The list is an **entry filter**, not a claim about the codec inside. A `.mkv` can hold almost
//! anything; whether the track actually decodes is ffmpeg's answer, given in [`super::probe`].
//! Filtering on the extension first is still worth it: telling someone their `.psd` is not an
//! audio file should not require launching a decoder.

/// Which of the two families an extension belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Audio,
    /// The audio track is extracted; the picture is dropped (CONCEPTION.md §2, decision #21).
    Video,
}

/// The accepted extensions, without their dot, lowercase.
const SUPPORTED: &[(&str, MediaKind)] = &[
    ("m4a", MediaKind::Audio),
    ("mp3", MediaKind::Audio),
    ("wav", MediaKind::Audio),
    ("flac", MediaKind::Audio),
    ("ogg", MediaKind::Audio),
    ("opus", MediaKind::Audio),
    ("aac", MediaKind::Audio),
    ("wma", MediaKind::Audio),
    ("mp4", MediaKind::Video),
    ("mov", MediaKind::Video),
    ("mkv", MediaKind::Video),
    ("avi", MediaKind::Video),
    ("webm", MediaKind::Video),
];

/// The family of an extension, given with or without its dot, in any case.
#[must_use]
pub fn kind_of(extension: &str) -> Option<MediaKind> {
    let extension = extension.trim_start_matches('.').to_ascii_lowercase();
    SUPPORTED
        .iter()
        .find(|(candidate, _)| *candidate == extension)
        .map(|(_, kind)| *kind)
}

#[must_use]
pub fn is_supported(extension: &str) -> bool {
    kind_of(extension).is_some()
}

#[must_use]
pub fn audio_extensions() -> Vec<&'static str> {
    extensions_of(MediaKind::Audio)
}

#[must_use]
pub fn video_extensions() -> Vec<&'static str> {
    extensions_of(MediaKind::Video)
}

fn extensions_of(kind: MediaKind) -> Vec<&'static str> {
    SUPPORTED
        .iter()
        .filter(|(_, candidate)| *candidate == kind)
        .map(|(extension, _)| *extension)
        .collect()
}

/// Every accepted extension — what the file picker and the Explorer context menu of phase 09
/// will register.
#[must_use]
pub fn all_extensions() -> Vec<&'static str> {
    SUPPORTED.iter().map(|(extension, _)| *extension).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn the_list_is_exactly_the_one_the_roadmap_fixes() {
        assert_eq!(
            audio_extensions(),
            vec!["m4a", "mp3", "wav", "flac", "ogg", "opus", "aac", "wma"]
        );
        assert_eq!(video_extensions(), vec!["mp4", "mov", "mkv", "avi", "webm"]);
    }

    #[test]
    fn an_extension_is_recognised_however_it_is_written() {
        for spelling in ["mp3", ".mp3", "MP3", ".Mp3"] {
            assert_eq!(kind_of(spelling), Some(MediaKind::Audio), "{spelling}");
        }
        assert_eq!(kind_of(".MKV"), Some(MediaKind::Video));
    }

    #[test]
    fn anything_else_is_refused() {
        for extension in ["psd", "txt", "pdf", "mp", "mp33", "", "."] {
            assert!(!is_supported(extension), "{extension} should be refused");
        }
    }

    #[test]
    fn no_extension_is_listed_twice_or_in_both_families() {
        let all = all_extensions();
        let unique: HashSet<_> = all.iter().collect();
        assert_eq!(all.len(), unique.len(), "duplicate in {all:?}");
        assert_eq!(
            all.len(),
            audio_extensions().len() + video_extensions().len()
        );
    }

    #[test]
    fn the_table_is_stored_lowercase_and_dotless() {
        for extension in all_extensions() {
            assert_eq!(extension, extension.to_ascii_lowercase());
            assert!(!extension.starts_with('.'));
        }
    }
}
