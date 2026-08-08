//! Runtime theme pack validation and installation.

pub mod install;
pub mod validate;

#[allow(unused_imports)] // consumed by Task 9 IPC commands
pub use install::install_theme_zip;
#[allow(unused_imports)]
pub use validate::{
    allowed_theme_extension, validate_pack_contents, validate_pack_dir, validate_theme_zip_path,
    ThemeManifest, THEME_API_VERSION,
};
