use std::path::Path;
use std::sync::Arc;

use crate::config;
use crate::sandbox::Sandbox;

pub fn create(config: &config::Config, cwd: &Path) -> Arc<dyn Sandbox> {
    Arc::new(crate::macos::MacosSandbox::new(config, cwd))
}
