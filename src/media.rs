//! Images in messages: attaching them and turning them into something egui can draw.

use agentai::{Media, MediaSource};
use base64::Engine;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

/// The MIME type of an image file, from its extension.
pub fn image_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// Media from a file dropped on the window, if it is an image. Browsers read dropped files
/// only asynchronously, so on the web this gives nothing yet.
#[cfg(not(target_arch = "wasm32"))]
pub fn dropped_image(file: &egui::DroppedFileHandle) -> Option<Media> {
    let path = file.path();
    let content_type = image_type(path)?;
    let bytes = file.bytes().ok()?;
    let media = Media::from_bytes(content_type, bytes);
    Some(match path.file_name() {
        Some(name) => media.with_name(name.to_string_lossy()),
        None => media,
    })
}

#[cfg(target_arch = "wasm32")]
pub fn dropped_image(_file: &egui::DroppedFileHandle) -> Option<Media> {
    None
}

/// Decoded images, so base64 is decoded once and not every frame.
#[derive(Default)]
pub struct MediaCache {
    decoded: HashMap<u64, egui::load::Bytes>,
}

impl MediaCache {
    /// An image for `media`, or `None` if it is not an image or cannot be decoded.
    pub fn image(&mut self, media: &Media) -> Option<egui::Image<'static>> {
        if !media.is_image() {
            return None;
        }
        match &media.source {
            MediaSource::Url(url) => Some(egui::Image::new(url.clone())),
            MediaSource::Base64(data) => {
                let key = key(data);
                let bytes = match self.decoded.entry(key) {
                    Entry::Occupied(entry) => entry.get().clone(),
                    Entry::Vacant(entry) => {
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(data)
                            .ok()?;
                        entry.insert(bytes.into()).clone()
                    }
                };
                // The extension tells the image loader the format
                let extension = media.content_type.rsplit('/').next().unwrap_or("png");
                let uri = format!("bytes://egui-agent-chat/{key:016x}.{extension}");
                Some(egui::Image::from_bytes(uri, bytes))
            }
        }
    }
}

/// A cheap key for base64 data: its length and both ends, not all of a large image.
fn key(data: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    data.len().hash(&mut hasher);
    data.as_bytes()[..data.len().min(256)].hash(&mut hasher);
    data.as_bytes()[data.len().saturating_sub(256)..].hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_types_come_from_extensions() {
        assert_eq!(image_type(Path::new("/a/b.PNG")), Some("image/png"));
        assert_eq!(image_type(Path::new("photo.jpeg")), Some("image/jpeg"));
        assert_eq!(image_type(Path::new("notes.txt")), None);
    }

    #[test]
    fn base64_images_are_decoded_once() {
        let mut cache = MediaCache::default();
        let media = Media::from_bytes("image/png", b"not really a png");
        assert!(cache.image(&media).is_some());
        assert!(cache.image(&media).is_some());
        assert_eq!(cache.decoded.len(), 1);
        assert!(cache.image(&Media::from_bytes("audio/wav", b"x")).is_none());
    }
}
