use std::collections::HashMap;

use tiny_skia::{IntRect, Pixmap, PixmapPaint, PremultipliedColorU8, Transform};
use uuid::Uuid;

use crate::document::Document;
use crate::object::ObjectKind;

/// Composites the base asset and the document's object stack into a single
/// premultiplied-alpha pixmap. This runs on every frame the document
/// changes; individual object kinds interpret themselves here (or, once
/// plugins exist, via a registry keyed on `ObjectKind`).
///
/// `assets` holds pixel data for object kinds that need it (currently just
/// pasted images), keyed by the owning `StackObject::id`; it lives outside
/// `Document` for the same reason the base asset's pixels do.
///
/// A `Crop` object clips the final output regardless of where it sits in
/// the stack — it defines the composite's viewport, not a paint step. When
/// `apply_crop` is false the clip is skipped entirely so the caller can show
/// the full canvas with a crop overlay while the crop tool is active.
pub fn composite(
    document: &Document,
    base: &image::RgbaImage,
    assets: &HashMap<Uuid, image::RgbaImage>,
    apply_crop: bool,
) -> Pixmap {
    let (base_w, base_h) = (base.width().max(1), base.height().max(1));
    let mut canvas = Pixmap::new(base_w, base_h).expect("non-zero canvas size");

    if let Some(base_pixmap) = image_to_pixmap(base) {
        canvas.draw_pixmap(
            0,
            0,
            base_pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }

    let mut crop_rect = IntRect::from_xywh(0, 0, base_w, base_h);

    for object in &document.objects {
        if !object.visible {
            continue;
        }
        match &object.kind {
            ObjectKind::Crop(data) => {
                crop_rect = IntRect::from_xywh(
                    data.x.round() as i32,
                    data.y.round() as i32,
                    (data.width.round() as u32).max(1),
                    (data.height.round() as u32).max(1),
                )
                .or(crop_rect);
            }
            ObjectKind::Image(data) => {
                if let Some(pixels) = assets.get(&object.id) {
                    if let Some(pixmap) = image_to_pixmap(pixels) {
                        canvas.draw_pixmap(
                            data.x.round() as i32,
                            data.y.round() as i32,
                            pixmap.as_ref(),
                            &PixmapPaint::default(),
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }
    }

    if !apply_crop {
        return canvas;
    }

    match crop_rect {
        Some(rect) => crop_to(&canvas, rect),
        None => canvas,
    }
}

/// Converts a pixmap's premultiplied pixels back to straight-alpha RGBA8,
/// the format the system clipboard (via arboard) expects.
pub fn pixmap_to_straight_rgba(pixmap: &Pixmap) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(pixmap.pixels().len() * 4);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        bytes.extend_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
    }
    bytes
}

/// `image` stores straight (non-premultiplied) alpha; tiny-skia requires
/// premultiplied alpha for its pixmaps.
fn image_to_pixmap(image: &image::RgbaImage) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(image.width(), image.height())?;
    for (dst, src) in pixmap.pixels_mut().iter_mut().zip(image.pixels()) {
        let [r, g, b, a] = src.0;
        *dst = PremultipliedColorU8::from_rgba(
            (r as u16 * a as u16 / 255) as u8,
            (g as u16 * a as u16 / 255) as u8,
            (b as u16 * a as u16 / 255) as u8,
            a,
        )
        .unwrap_or(PremultipliedColorU8::TRANSPARENT);
    }
    Some(pixmap)
}

fn crop_to(canvas: &Pixmap, rect: IntRect) -> Pixmap {
    let mut out = Pixmap::new(rect.width(), rect.height()).expect("non-zero crop size");
    out.draw_pixmap(
        -rect.x(),
        -rect.y(),
        canvas.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    out
}
