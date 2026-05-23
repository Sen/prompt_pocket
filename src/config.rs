use std::{collections::HashMap, env};

pub const DEFAULT_MODEL: &str = "gpt-5.2";
pub const DEFAULT_DB_PATH: &str = "data/prompt-pocket-rust.sqlite";
pub const DEFAULT_LOG_LIMIT: i64 = 2000;
pub const DEFAULT_PORT: u16 = 8787;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub app_password: Option<String>,
    pub api_key: Option<String>,
    pub model: String,
    pub proxy_url: Option<String>,
    pub db_path: String,
    pub log_limit: i64,
    pub port: u16,
    pub production: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let values = env::vars().collect::<HashMap<_, _>>();

        Self::from_map(&values)
    }

    pub fn from_map(values: &HashMap<String, String>) -> Self {
        let app_env = read_trimmed(values, "APP_ENV").unwrap_or_default();

        Self {
            app_password: read_trimmed(values, "APP_PASSWORD"),
            api_key: read_trimmed(values, "OPENAI_API_KEY"),
            model: read_trimmed(values, "OPENAI_MODEL")
                .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
            proxy_url: read_trimmed(values, "OPENAI_PROXY_URL"),
            db_path: read_trimmed(values, "PROMPT_DB_PATH")
                .unwrap_or_else(|| DEFAULT_DB_PATH.to_string()),
            log_limit: read_i64(values, "PROMPT_LOG_LIMIT", DEFAULT_LOG_LIMIT, 1),
            port: read_u16(values, "PORT", DEFAULT_PORT),
            production: app_env == "production",
        }
    }

    pub fn auth_enabled(&self) -> bool {
        self.app_password.is_some()
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    pub fn has_proxy(&self) -> bool {
        self.proxy_url.is_some()
    }
}

fn read_trimmed(values: &HashMap<String, String>, key: &str) -> Option<String> {
    let value = values.get(key)?.trim();

    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn read_i64(values: &HashMap<String, String>, key: &str, default: i64, minimum: i64) -> i64 {
    values
        .get(key)
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value >= minimum)
        .unwrap_or(default)
}

fn read_u16(values: &HashMap<String, String>, key: &str, default: u16) -> u16 {
    values
        .get(key)
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_runtime_config_from_map() {
        let values = HashMap::from([
            ("APP_PASSWORD".to_string(), " secret ".to_string()),
            ("OPENAI_API_KEY".to_string(), " key ".to_string()),
            ("OPENAI_MODEL".to_string(), " custom-model ".to_string()),
            (
                "OPENAI_PROXY_URL".to_string(),
                " http://127.0.0.1:7890 ".to_string(),
            ),
            (
                "PROMPT_DB_PATH".to_string(),
                " data/test.sqlite ".to_string(),
            ),
            ("PROMPT_LOG_LIMIT".to_string(), "10".to_string()),
            ("PORT".to_string(), "9000".to_string()),
            ("APP_ENV".to_string(), "production".to_string()),
        ]);

        let config = Config::from_map(&values);

        assert_eq!(config.app_password.as_deref(), Some("secret"));
        assert_eq!(config.api_key.as_deref(), Some("key"));
        assert_eq!(config.model, "custom-model");
        assert_eq!(config.proxy_url.as_deref(), Some("http://127.0.0.1:7890"));
        assert_eq!(config.db_path, "data/test.sqlite");
        assert_eq!(config.log_limit, 10);
        assert_eq!(config.port, 9000);
        assert!(config.production);
    }

    #[test]
    fn uses_defaults_for_missing_or_invalid_values() {
        let values = HashMap::from([
            ("PROMPT_LOG_LIMIT".to_string(), "0".to_string()),
            ("PORT".to_string(), "bad".to_string()),
        ]);

        let config = Config::from_map(&values);

        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.db_path, DEFAULT_DB_PATH);
        assert_eq!(config.log_limit, DEFAULT_LOG_LIMIT);
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(!config.production);
    }
}
