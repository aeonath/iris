use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single entry in the document's ordered object stack. Every canvas
/// operation (crop, transform, paint stroke, shape, ...) is represented as
/// one of these rather than baked into pixels, so the stack can be
/// re-ordered, hidden, or edited after the fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackObject {
    pub id: Uuid,
    pub name: String,
    pub visible: bool,
    pub kind: ObjectKind,
}

impl StackObject {
    pub fn new(name: impl Into<String>, kind: ObjectKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            visible: true,
            kind,
        }
    }
}

/// Built-in object kinds. Plugin-contributed kinds (shapes, paint strokes,
/// eraser, text, ...) will extend this once the plugin loading story lands;
/// for now the core editor only needs `Crop` and `Image`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectKind {
    Crop(CropData),
    Image(ImageData),
}

/// Non-destructive crop: defines the rectangle (in base-asset pixel space)
/// that the composite is clipped to. The last visible `Crop` in the stack
/// wins.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CropData {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// A pasted image, placed on top of the stack below it. The pixel buffer
/// itself lives outside the document (see `IrisApp::pasted_images`, keyed by
/// this object's `id`) since it's runtime asset data, not document
/// structure — the same reasoning that keeps the base asset's pixels out of
/// `Document`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ImageData {
    pub x: f32,
    pub y: f32,
}
