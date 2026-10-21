use std::time::Duration;

pub struct AppConfig {
    pub name: String,
    pub port: u16,
    pub timeout: Duration,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "my-service".into(),
            port: 3000,
            timeout: Duration::from_secs(10),
        }
    }
}

pub fn start(config: &AppConfig) {
    println!("Starting {}", config.name);
    println!("Listening on port {}", config.port);
}
