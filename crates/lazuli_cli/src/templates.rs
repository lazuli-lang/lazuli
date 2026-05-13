#[allow(dead_code)]
pub static DEFAULT_TEMPLATE: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../lazurite/templates/default");
