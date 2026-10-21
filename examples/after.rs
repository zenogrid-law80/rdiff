use std::time::Duration;

pub struct AppConfig {
    pub name: String,
    pub port: u16,
    pub timeout: Duration,
    pub retries: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "rdiff-service".into(),
            port: 8080,
            timeout: Duration::from_secs(30),
            retries: 3,
        }
    }
}

pub fn start(config: &AppConfig) {
    println!("Starting {} on port {}", config.name, config.port);
    println!("Ready to accept connections");
}
