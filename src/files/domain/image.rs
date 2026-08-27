//! Image previews for the Files page: format/dimension probing (cheap, header
//! only) and the terminal graphics picker used to draw them.

use ratatui_image::picker::Picker;
use std::fs;
use std::io::BufReader;
use std::path::Path;

/// Extensions treated as images (must match the decoders enabled for the
/// `image` crate in Cargo.toml).
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp"];

/// Images above this size are not decoded; only their metadata is shown.
pub const IMAGE_MAX_BYTES: u64 = 20 * 1024 * 1024;

/// `image-preview` config modes, first is the default.
pub const IMAGE_PREVIEW_MODES: &[&str] = &["auto", "halfblocks", "none"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePreviewMode {
    /// Query the terminal for a graphics protocol, fall back to halfblocks.
    Auto,
    /// Skip the terminal query and always draw unicode halfblocks.
    Halfblocks,
    /// Never decode images; show metadata only.
    None,
}

impl ImagePreviewMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "halfblocks" => Some(Self::Halfblocks),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

/// Header-only metadata of an image file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    /// Upper-case format name (`PNG`, `JPEG`, ...).
    pub format: String,
    pub width: u32,
    pub height: u32,
}

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
}

/// Read only the header of `path`; `None` if it is not a decodable image.
pub fn probe(path: &Path) -> Option<ImageInfo> {
    let file = fs::File::open(path).ok()?;
    let reader = image::ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .ok()?;
    let format = reader.format()?;
    let (width, height) = reader.into_dimensions().ok()?;
    Some(ImageInfo {
        format: format.extensions_str()[0].to_ascii_uppercase(),
        width,
        height,
    })
}

/// Build the picker for `mode`. Must run before the TUI takes over the
/// terminal, since `auto` writes a query and reads the reply from stdin.
/// Failures (not a tty, no reply) degrade to halfblocks rather than erroring.
pub fn make_picker(mode: ImagePreviewMode) -> Option<Picker> {
    match mode {
        ImagePreviewMode::None => None,
        ImagePreviewMode::Auto => Some(Picker::from_query_stdio().unwrap_or_else(|_| halfblocks())),
        ImagePreviewMode::Halfblocks => Some(halfblocks()),
    }
}

fn halfblocks() -> Picker {
    Picker::halfblocks()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parsing_matches_the_listed_modes() {
        for m in IMAGE_PREVIEW_MODES {
            assert!(ImagePreviewMode::parse(m).is_some(), "{m}");
        }
        assert_eq!(
            ImagePreviewMode::parse("auto"),
            Some(ImagePreviewMode::Auto)
        );
        assert_eq!(ImagePreviewMode::parse("sixel"), None);
    }

    #[test]
    fn image_paths_by_extension() {
        assert!(is_image_path(Path::new("a/b.PNG")));
        assert!(is_image_path(Path::new("x.webp")));
        assert!(!is_image_path(Path::new("x.svg")));
        assert!(!is_image_path(Path::new("png")));
    }

    #[test]
    fn probe_reads_dimensions_and_rejects_non_images() {
        let dir = std::env::temp_dir().join(format!("vig-image-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let png = dir.join("t.png");
        image::RgbImage::new(12, 7).save(&png).unwrap();
        assert_eq!(
            probe(&png),
            Some(ImageInfo {
                format: "PNG".into(),
                width: 12,
                height: 7
            })
        );
        let fake = dir.join("fake.png");
        fs::write(&fake, b"not an image").unwrap();
        assert_eq!(probe(&fake), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
