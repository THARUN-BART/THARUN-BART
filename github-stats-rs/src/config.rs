use dotenvy::{dotenv, from_path};
use std::env;
use std::path::Path;

pub struct Config {
    pub token: String,
    pub username: String,
}

pub fn load() -> Config {
    dotenv().ok();
    from_path(Path::new("github-stats-rs/.env")).ok();
    from_path(Path::new("../.env")).ok();

    let token = env::var("ACCESS_TOKEN").unwrap_or_else(|_| {
        eprintln!("WARNING: ACCESS_TOKEN not found in environment.");
        "DUMMY_TOKEN".to_string()
    });
    let username = env::var("USER_NAME").unwrap_or_else(|_| "THARUN-BART".to_string());

    Config { token, username }
}
