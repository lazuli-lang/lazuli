//! Frontend web scaffold templates — split into the shell + ui
//! sub-modules so neither half balloons past the 500-LOC rail.
//!
//! Every `FRONTEND_WEB_*` / `FRONTEND_THEME_*` / `FRONTEND_TAILWIND_*` /
//! `FRONTEND_TSCONFIG_*` / `FRONTEND_VITE_*` / `FRONTEND_PACKAGE_*` /
//! `FRONTEND_GITIGNORE` constant continues to be reachable via
//! `crate::templates::FRONTEND_*` thanks to the `pub use *::*`
//! re-exports below.
//!
//! Wave R7-3 extract.

mod shell;
mod ui;

pub use shell::*;
pub use ui::*;
