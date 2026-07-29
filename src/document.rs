use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::object::StackObject;

/// The serializable description of an IRIS document: the base asset it
/// references and the ordered stack of non-destructive objects applied on
/// top of it. Pixel data for the base asset is loaded separately at
/// runtime and is not part of this struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Uuid,
    pub name: String,
    pub canvas_size: (u32, u32),
    pub base_asset_path: Option<PathBuf>,
    pub objects: Vec<StackObject>,
}

impl Document {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            canvas_size: (width, height),
            base_asset_path: None,
            objects: Vec::new(),
        }
    }
}
