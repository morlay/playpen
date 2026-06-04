#[allow(clippy::module_inception)]
mod model;
mod registry;
mod deepseek;

pub use model::*;
pub use registry::Registry;
pub use deepseek::builtin_providers;
