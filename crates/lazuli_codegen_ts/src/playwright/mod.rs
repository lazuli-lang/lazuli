pub mod api_policy;
pub mod lifecycle_gate;
pub mod scalar_fixtures_barrel;

pub use api_policy::{Command, emit_playwright_api_policy};
pub use lifecycle_gate::emit_playwright_lifecycle_gate;
pub use scalar_fixtures_barrel::{ScalarPlugin, emit_playwright_scalar_fixtures_barrel};
