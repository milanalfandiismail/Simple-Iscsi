use std::sync::Arc;
use parking_lot::RwLock;
use crate::config::Config;

#[derive(Clone)]
pub struct SharedConfig {
    pub inner: Arc<RwLock<Arc<Config>>>,
}

impl SharedConfig {
    pub fn new(config: Config) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(config))),
        }
    }

    pub fn read(&self) -> Arc<Config> {
        self.inner.read().clone()
    }

    pub fn update(&self, new_config: Config) {
        *self.inner.write() = Arc::new(new_config);
    }
}
