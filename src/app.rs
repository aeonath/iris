use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use uuid::Uuid;

use crate::document::Document;
use crate::object::{CropData, ImageData, ObjectKind, StackObject};
use crate::render;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Crop,
}

/// Tracks an in-progress crop drag: which `Crop` object is being edited and
/// where (in base-image pixel space) the drag started.
struct CropDrag {
    object_id: Uuid,
    start: egui::Pos2,
}

pub struct IrisApp {
    document: Document,
    base_image: image::RgbaImage,
    /// Pixel data for pasted `ObjectKind::Image` objects, keyed by the
    /// owning `StackObject::id`. Kept out of `Document` for the same reason
    /// the base asset's pixels are: it's runtime data, not structure.
    pasted_images: HashMap<Uuid, image::RgbaImage>,
    texture: Option<egui::TextureHandle>,
    selected_object: Option<Uuid>,
    active_tool: Tool,
    crop_drag: Option<CropDrag>,
    clipboard: Option<arboard::Clipboard>,
    status: Option<String>,
}

impl IrisApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Bootstrap placeholder canvas until an asset is opened or pasted.
        let base_image = image::RgbaImage::from_pixel(800, 600, image::Rgba([0, 0, 0, 0]));
        let document = Document::new("Untitled", base_image.width(), base_image.height());

        Self {
            document,
            base_image,
            pasted_images: HashMap::new(),
            texture: None,
            selected_object: None,
            active_tool: Tool::Select,
            crop_drag: None,
            clipboard: arboard::Clipboard::new().ok(),
            status: None,
        }
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    fn refresh_texture(&mut self, ctx: &egui::Context) {
        // While actively cropping, show the full canvas with an overlay
        // instead of the already-clipped result, so the rectangle can be
        // grown back out.
        let apply_crop = self.active_tool != Tool::Crop;
        let pixmap = render::composite(
            &self.document,
            &self.base_image,
            &self.pasted_images,
            apply_crop,
        );
        let size = [pixmap.width() as usize, pixmap.height() as usize];
        let color_image = egui::ColorImage::from_rgba_premultiplied(size, pixmap.data());
        match &mut self.texture {
            Some(texture) => texture.set(color_image, egui::TextureOptions::NEAREST),
            None => {
                self.texture =
                    Some(ctx.load_texture("composite", color_image, egui::TextureOptions::NEAREST))
            }
        }
    }

    // ---- File I/O (PNG only) ----

    fn open_png(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .pick_file()
        else {
            return;
        };
        match image::open(&path) {
            Ok(image) => self.load_base_image(image.to_rgba8(), Some(path)),
            Err(err) => self.set_status(format!("Failed to open {}: {err}", path.display())),
        }
    }

    fn load_base_image(&mut self, image: image::RgbaImage, path: Option<PathBuf>) {
        let name = path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled");
        let mut document = Document::new(name, image.width(), image.height());
        document.base_asset_path = path;

        self.document = document;
        self.base_image = image;
        self.pasted_images.clear();
        self.selected_object = None;
        self.crop_drag = None;
        self.set_status("Opened image");
    }

    fn save_png(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name(format!("{}.png", self.document.name))
            .save_file()
        else {
            return;
        };
        let pixmap = render::composite(&self.document, &self.base_image, &self.pasted_images, true);
        match pixmap.save_png(&path) {
            Ok(()) => self.set_status(format!("Saved to {}", path.display())),
            Err(err) => self.set_status(format!("Failed to save: {err}")),
        }
    }

    // ---- Clipboard ----

    fn copy_to_clipboard(&mut self) {
        let pixmap = render::composite(&self.document, &self.base_image, &self.pasted_images, true);
        let Some(clipboard) = self.clipboard.as_mut() else {
            self.status = Some("Clipboard unavailable".to_string());
            return;
        };
        let bytes = render::pixmap_to_straight_rgba(&pixmap);
        let image_data = arboard::ImageData {
            width: pixmap.width() as usize,
            height: pixmap.height() as usize,
            bytes: std::borrow::Cow::Owned(bytes),
        };
        self.status = Some(match clipboard.set_image(image_data) {
            Ok(()) => "Copied composite to clipboard".to_string(),
            Err(err) => format!("Copy failed: {err}"),
        });
    }

    fn paste_from_clipboard(&mut self) {
        let Some(clipboard) = self.clipboard.as_mut() else {
            self.status = Some("Clipboard unavailable".to_string());
            return;
        };
        let image_data = match clipboard.get_image() {
            Ok(data) => data,
            Err(err) => {
                self.status = Some(format!("Nothing to paste: {err}"));
                return;
            }
        };
        let (width, height) = (image_data.width as u32, image_data.height as u32);
        let Some(pixels) = image::RgbaImage::from_raw(width, height, image_data.bytes.into_owned())
        else {
            self.status = Some("Clipboard image had an unexpected size".to_string());
            return;
        };

        let (canvas_w, canvas_h) = self.document.canvas_size;
        let x = ((canvas_w as f32 - width as f32) / 2.0).max(0.0);
        let y = ((canvas_h as f32 - height as f32) / 2.0).max(0.0);

        let object = StackObject::new("Pasted Image", ObjectKind::Image(ImageData { x, y }));
        let id = object.id;
        self.pasted_images.insert(id, pixels);
        self.selected_object = Some(id);
        self.document.objects.push(object);
        self.status = Some("Pasted image".to_string());
    }

    // ---- Crop tool ----

    /// Returns the id of the document's `Crop` object, creating one
    /// (initially degenerate) if none exists yet.
    fn ensure_crop_object(&mut self) -> Uuid {
        if let Some(existing) = self
            .document
            .objects
            .iter()
            .find(|o| matches!(o.kind, ObjectKind::Crop(_)))
        {
            return existing.id;
        }
        let object = StackObject::new(
            "Crop",
            ObjectKind::Crop(CropData {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            }),
        );
        let id = object.id;
        self.selected_object = Some(id);
        self.document.objects.push(object);
        id
    }

    fn set_crop_rect(&mut self, id: Uuid, a: egui::Pos2, b: egui::Pos2) {
        let Some(object) = self.document.objects.iter_mut().find(|o| o.id == id) else {
            return;
        };
        let ObjectKind::Crop(data) = &mut object.kind else {
            return;
        };
        data.x = a.x.min(b.x);
        data.y = a.y.min(b.y);
        data.width = (a.x - b.x).abs().max(1.0);
        data.height = (a.y - b.y).abs().max(1.0);
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (open, save, copy, paste) = ctx.input(|i| {
            (
                i.modifiers.ctrl && i.key_pressed(egui::Key::O),
                i.modifiers.ctrl && i.key_pressed(egui::Key::S),
                i.modifiers.ctrl && i.key_pressed(egui::Key::C),
                i.modifiers.ctrl && i.key_pressed(egui::Key::V),
            )
        });
        if open {
            self.open_png();
        }
        if save {
            self.save_png();
        }
        if copy {
            self.copy_to_clipboard();
        }
        if paste {
            self.paste_from_clipboard();
        }
    }

    // ---- UI panels ----

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open PNG...").clicked() {
                    self.open_png();
                    ui.close();
                }
                if ui.button("Save As PNG...").clicked() {
                    self.save_png();
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Copy Composite").clicked() {
                    self.copy_to_clipboard();
                    ui.close();
                }
                if ui.button("Paste Image").clicked() {
                    self.paste_from_clipboard();
                    ui.close();
                }
            });
            if let Some(status) = &self.status {
                ui.separator();
                ui.weak(status);
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.vertical_centered(|ui| {
            if ui
                .selectable_label(self.active_tool == Tool::Select, "\u{2726}")
                .on_hover_text("Select")
                .clicked()
            {
                self.active_tool = Tool::Select;
            }
            if ui
                .selectable_label(self.active_tool == Tool::Crop, "\u{2702}")
                .on_hover_text("Crop (drag on canvas)")
                .clicked()
            {
                self.active_tool = Tool::Crop;
            }
        });
    }

    fn object_stack_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Object Stack");
        ui.separator();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if self.document.objects.is_empty() {
                ui.weak("No objects yet");
                return;
            }
            // Top of the stack (last evaluated) is listed first.
            for object in self.document.objects.iter_mut().rev() {
                ui.horizontal(|ui| {
                    let eye = if object.visible { "\u{1F441}" } else { "\u{2013}" };
                    if ui.small_button(eye).clicked() {
                        object.visible = !object.visible;
                    }
                    let selected = self.selected_object == Some(object.id);
                    if ui.selectable_label(selected, &object.name).clicked() {
                        self.selected_object = Some(object.id);
                    }
                });
            }
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        let selected = self.selected_object;
        let object = selected.and_then(|id| self.document.objects.iter_mut().find(|o| o.id == id));

        match object {
            None => {
                ui.weak("Select an object on the stack to edit its properties.");
            }
            Some(object) => {
                ui.horizontal(|ui| {
                    ui.label(&object.name);
                    ui.separator();
                    match &mut object.kind {
                        ObjectKind::Crop(data) => {
                            ui.label("X");
                            ui.add(egui::DragValue::new(&mut data.x));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut data.y));
                            ui.label("W");
                            ui.add(egui::DragValue::new(&mut data.width).range(1.0..=f32::MAX));
                            ui.label("H");
                            ui.add(egui::DragValue::new(&mut data.height).range(1.0..=f32::MAX));
                        }
                        ObjectKind::Image(data) => {
                            ui.label("X");
                            ui.add(egui::DragValue::new(&mut data.x));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut data.y));
                        }
                    }
                });
            }
        }
    }

    /// Draws the transparent checkerboard background and the composited
    /// image scaled to fit, centered in the available viewport space. When
    /// the crop tool is active, also handles drag-to-crop and draws the
    /// crop overlay.
    fn canvas_viewport(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());

        let Some(texture) = self.texture.clone() else {
            return;
        };
        let image_size = texture.size_vec2();
        if image_size.x <= 0.0 || image_size.y <= 0.0 {
            return;
        }

        let scale = (rect.width() / image_size.x)
            .min(rect.height() / image_size.y)
            .min(1.0);
        let draw_size = image_size * scale;
        let draw_rect = egui::Rect::from_center_size(rect.center(), draw_size);

        let to_image = |p: egui::Pos2| -> egui::Pos2 {
            let rel = p - draw_rect.min;
            egui::pos2(
                (rel.x / draw_rect.width() * image_size.x).clamp(0.0, image_size.x),
                (rel.y / draw_rect.height() * image_size.y).clamp(0.0, image_size.y),
            )
        };
        let to_screen = |p: egui::Pos2| -> egui::Pos2 {
            draw_rect.min
                + egui::vec2(
                    p.x / image_size.x * draw_rect.width(),
                    p.y / image_size.y * draw_rect.height(),
                )
        };

        if self.active_tool == Tool::Crop {
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
            {
                let id = self.ensure_crop_object();
                self.crop_drag = Some(CropDrag {
                    object_id: id,
                    start: to_image(pos),
                });
            }
            if let Some(drag) = &self.crop_drag
                && let Some(pos) = response.interact_pointer_pos()
            {
                self.set_crop_rect(drag.object_id, drag.start, to_image(pos));
            }
            if response.drag_stopped() {
                self.crop_drag = None;
            }
        }

        let painter = ui.painter_at(rect);
        draw_checkerboard(&painter, draw_rect, 12.0);
        painter.image(
            texture.id(),
            draw_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        if self.active_tool == Tool::Crop {
            let crop = self.document.objects.iter().find_map(|o| match &o.kind {
                ObjectKind::Crop(data) => Some(*data),
                _ => None,
            });
            if let Some(data) = crop {
                let crop_rect = egui::Rect::from_min_size(
                    to_screen(egui::pos2(data.x, data.y)),
                    egui::vec2(data.width, data.height) * scale,
                );
                draw_crop_overlay(&painter, draw_rect, crop_rect);
            }
        }
    }
}

impl eframe::App for IrisApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui.ctx());
        self.refresh_texture(ui.ctx());

        egui::Panel::top("menu_bar").show(ui, |ui| self.menu_bar(ui));

        egui::Panel::left("toolbar")
            .resizable(false)
            .exact_size(48.0)
            .show(ui, |ui| self.toolbar(ui));

        egui::Panel::right("object_stack")
            .resizable(true)
            .default_size(240.0)
            .size_range(160.0..=400.0)
            .show(ui, |ui| self.object_stack_panel(ui));

        egui::Panel::top("inspector").show(ui, |ui| self.inspector(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(ui.style()).fill(egui::Color32::from_gray(40)))
            .show(ui, |ui| self.canvas_viewport(ui));
    }
}

fn draw_checkerboard(painter: &egui::Painter, rect: egui::Rect, tile: f32) {
    let light = egui::Color32::from_gray(205);
    let dark = egui::Color32::from_gray(165);
    let cols = (rect.width() / tile).ceil() as i32;
    let rows = (rect.height() / tile).ceil() as i32;
    for row in 0..rows {
        for col in 0..cols {
            let color = if (row + col) % 2 == 0 { light } else { dark };
            let min = rect.min + egui::vec2(col as f32 * tile, row as f32 * tile);
            let tile_rect = egui::Rect::from_min_size(min, egui::vec2(tile, tile)).intersect(rect);
            painter.rect_filled(tile_rect, 0.0, color);
        }
    }
}

/// Dims everything outside `crop_rect` within `canvas_rect` and outlines the
/// crop boundary, both in screen space.
fn draw_crop_overlay(painter: &egui::Painter, canvas_rect: egui::Rect, crop_rect: egui::Rect) {
    let dim = egui::Color32::from_black_alpha(140);
    painter.rect_filled(
        egui::Rect::from_min_max(canvas_rect.min, egui::pos2(canvas_rect.max.x, crop_rect.min.y)),
        0.0,
        dim,
    );
    painter.rect_filled(
        egui::Rect::from_min_max(egui::pos2(canvas_rect.min.x, crop_rect.max.y), canvas_rect.max),
        0.0,
        dim,
    );
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(canvas_rect.min.x, crop_rect.min.y),
            egui::pos2(crop_rect.min.x, crop_rect.max.y),
        ),
        0.0,
        dim,
    );
    painter.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(crop_rect.max.x, crop_rect.min.y),
            egui::pos2(canvas_rect.max.x, crop_rect.max.y),
        ),
        0.0,
        dim,
    );
    painter.rect_stroke(
        crop_rect,
        0.0,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 165, 0)),
        egui::StrokeKind::Inside,
    );
}
