/// Extension point for future plugins (shapes, paint, eraser, text, ...).
///
/// Not wired into the app yet: the core editor only ships `Crop`
/// (see `object::ObjectKind`), implemented directly against `render`.
/// Once a second object kind is needed, this trait grows a way to
/// contribute `ObjectKind` variants, toolbar entries, and inspector UI
/// without the core crate knowing about each plugin concretely. Whether
/// that means in-process trait objects or dynamically loaded libraries is
/// an open question, deliberately deferred until there's a second plugin
/// to design against.
#[allow(dead_code)]
pub trait Plugin {
    fn name(&self) -> &str;
}
