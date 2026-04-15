use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::sync::Arc;

pub struct ConfigManager {
    config: Arc<RwLock<super::Config>>,
}

impl ConfigManager {
    pub fn new(config: super::Config) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    fn read_config(&self, _location: &str) -> RwLockReadGuard<'_, super::Config> {
        self.config.read()
    }

    fn write_config(&self, _location: &str) -> RwLockWriteGuard<'_, super::Config> {
        self.config.write()
    }

    pub fn get_server_name(&self) -> String {
        let config = self.read_config("get_server_name");
        config.server.name.clone()
    }

    pub fn get_server_host(&self) -> String {
        let config = self.read_config("get_server_host");
        config.server.host.clone()
    }

    pub fn get_server_port(&self) -> u16 {
        let config = self.read_config("get_server_port");
        config.server.port
    }

    pub fn get_database_url(&self) -> String {
        let config = self.read_config("get_database_url");
        format!(
            "postgres://{}:{}@{}:{}/{}",
            config.database.username,
            config.database.password,
            config.database.host,
            config.database.port,
            config.database.name
        )
    }

    pub fn get_redis_url(&self) -> String {
        let config = self.read_config("get_redis_url");
        if let Some(password) = &config.redis.password {
            if !password.is_empty() {
                return format!("redis://:{}@{}:{}", password, config.redis.host, config.redis.port);
            }
        }
        format!("redis://{}:{}", config.redis.host, config.redis.port)
    }

    pub fn get_config(&self) -> super::Config {
        let config = self.read_config("get_config");
        config.clone()
    }

    pub fn update_config<F>(&self, f: F)
    where
        F: FnOnce(&mut super::Config),
    {
        let mut config = self.write_config("update_config");
        f(&mut config);
    }
}

impl Clone for ConfigManager {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
        }
    }
}
