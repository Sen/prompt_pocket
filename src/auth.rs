use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::http::{header, HeaderMap, HeaderValue};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::Sha256;

use crate::config::Config;

const AUTH_COOKIE_NAME: &str = "prompt_pocket_auth";
const AUTH_COOKIE_VALUE: &str = "authenticated";
const AUTH_COOKIE_MAX_AGE_SECONDS: u64 = 60 * 60 * 24 * 14;
const LOGIN_ATTEMPT_WINDOW_MS: u64 = 10 * 60 * 1000;
const LOGIN_ATTEMPT_LIMIT: u32 = 10;
const LOGIN_BAN_MS: u64 = 60 * 60 * 1000;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AuthStatus {
    pub enabled: bool,
    pub authenticated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginBlockReason {
    RateLimited,
    Banned,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginAttemptBlock {
    pub reason: LoginBlockReason,
    pub retry_after_seconds: u64,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoginAttemptResult {
    Allowed,
    Blocked(LoginAttemptBlock),
}

#[derive(Clone, Debug, Default)]
pub struct LoginAttemptLimiter {
    states: Arc<Mutex<HashMap<String, LoginAttemptState>>>,
}

#[derive(Clone, Debug)]
struct LoginAttemptState {
    window_started_at: u64,
    failures: u32,
    cooldown_until: Option<u64>,
    escalation_expires_at: Option<u64>,
    cooldown_count: u32,
    banned_until: Option<u64>,
}

pub fn get_auth_status(headers: &HeaderMap, config: &Config) -> AuthStatus {
    if !config.auth_enabled() {
        return AuthStatus {
            enabled: false,
            authenticated: true,
        };
    }

    let authenticated = config
        .app_password
        .as_deref()
        .and_then(|password| {
            read_cookie(headers, AUTH_COOKIE_NAME).map(|value| verify_cookie_value(password, value))
        })
        .unwrap_or(false);

    AuthStatus {
        enabled: true,
        authenticated,
    }
}

pub fn verify_app_password(config: &Config, password: Option<&str>) -> bool {
    match config.app_password.as_deref() {
        Some(app_password) => password == Some(app_password),
        None => true,
    }
}

pub fn set_auth_cookie_header(config: &Config) -> Option<HeaderValue> {
    let password = config.app_password.as_deref()?;
    let expires_at = unix_timestamp() + AUTH_COOKIE_MAX_AGE_SECONDS;
    let payload = format!("{AUTH_COOKIE_VALUE}:{expires_at}");
    let signature = sign_payload(password, payload.as_bytes());
    let value = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(payload.as_bytes()),
        URL_SAFE_NO_PAD.encode(signature)
    );
    let secure = if config.production { "; Secure" } else { "" };
    let cookie = format!(
        "{AUTH_COOKIE_NAME}={value}; Max-Age={AUTH_COOKIE_MAX_AGE_SECONDS}; Path=/; HttpOnly; SameSite=Lax{secure}"
    );

    HeaderValue::from_str(&cookie).ok()
}

pub fn clear_auth_cookie_header(config: &Config) -> HeaderValue {
    let secure = if config.production { "; Secure" } else { "" };
    let cookie = format!("{AUTH_COOKIE_NAME}=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax{secure}");

    HeaderValue::from_str(&cookie).expect("clear cookie header is valid")
}

pub fn get_client_ip(headers: &HeaderMap) -> String {
    header_value(headers, "cf-connecting-ip")
        .or_else(|| header_value(headers, "x-real-ip"))
        .or_else(|| {
            header_value(headers, "x-forwarded-for")
                .and_then(|value| value.split(',').next().map(|ip| ip.trim().to_string()))
                .filter(|value| !value.is_empty())
        })
        .or_else(|| header_value(headers, "x-client-ip"))
        .unwrap_or_else(|| "unknown".to_string())
}

impl LoginAttemptLimiter {
    pub fn get_block(&self, ip: &str) -> LoginAttemptResult {
        self.normalize_state(ip, monotonic_ms())
    }

    pub fn record_failure(&self, ip: &str) -> LoginAttemptResult {
        let now = monotonic_ms();
        let block = self.normalize_state(ip, now);

        if matches!(block, LoginAttemptResult::Blocked(_)) {
            return block;
        }

        let mut states = self
            .states
            .lock()
            .expect("login limiter mutex is not poisoned");
        let state = states.entry(ip.to_string()).or_insert(LoginAttemptState {
            window_started_at: now,
            failures: 0,
            cooldown_until: None,
            escalation_expires_at: None,
            cooldown_count: 0,
            banned_until: None,
        });

        if state.cooldown_count > 0
            && state
                .escalation_expires_at
                .map(|expires_at| expires_at > now)
                .unwrap_or(true)
        {
            state.banned_until = Some(now + LOGIN_BAN_MS);
            state.cooldown_until = None;
            state.escalation_expires_at = None;
            state.failures = 0;

            return LoginAttemptResult::Blocked(block_result(
                LoginBlockReason::Banned,
                retry_after_seconds(state.banned_until.expect("banned_until is set"), now),
            ));
        }

        state.failures += 1;

        if state.failures <= LOGIN_ATTEMPT_LIMIT {
            return LoginAttemptResult::Allowed;
        }

        state.cooldown_until = Some(state.window_started_at + LOGIN_ATTEMPT_WINDOW_MS);
        state.escalation_expires_at = state
            .cooldown_until
            .map(|cooldown| cooldown + LOGIN_ATTEMPT_WINDOW_MS);
        state.cooldown_count += 1;

        LoginAttemptResult::Blocked(block_result(
            LoginBlockReason::RateLimited,
            retry_after_seconds(state.cooldown_until.expect("cooldown_until is set"), now),
        ))
    }

    pub fn record_success(&self, ip: &str) {
        self.states
            .lock()
            .expect("login limiter mutex is not poisoned")
            .remove(ip);
    }

    fn normalize_state(&self, ip: &str, now: u64) -> LoginAttemptResult {
        let mut states = self
            .states
            .lock()
            .expect("login limiter mutex is not poisoned");
        let Some(state) = states.get_mut(ip) else {
            return LoginAttemptResult::Allowed;
        };

        if let Some(banned_until) = state.banned_until {
            if banned_until > now {
                return LoginAttemptResult::Blocked(block_result(
                    LoginBlockReason::Banned,
                    retry_after_seconds(banned_until, now),
                ));
            }

            states.remove(ip);
            return LoginAttemptResult::Allowed;
        }

        if let Some(cooldown_until) = state.cooldown_until {
            if cooldown_until > now {
                return LoginAttemptResult::Blocked(block_result(
                    LoginBlockReason::RateLimited,
                    retry_after_seconds(cooldown_until, now),
                ));
            }

            state.cooldown_until = None;
            state.window_started_at = now;
            state.failures = 0;
        }

        if state
            .escalation_expires_at
            .map(|expires_at| expires_at <= now)
            .unwrap_or(false)
        {
            state.cooldown_count = 0;
            state.escalation_expires_at = None;
        }

        if now.saturating_sub(state.window_started_at) >= LOGIN_ATTEMPT_WINDOW_MS {
            state.window_started_at = now;
            state.failures = 0;
        }

        LoginAttemptResult::Allowed
    }
}

fn read_cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;

    cookie_header
        .split(';')
        .filter_map(|part| part.trim().split_once('='))
        .find_map(|(cookie_name, cookie_value)| (cookie_name == name).then_some(cookie_value))
}

fn verify_cookie_value(password: &str, value: &str) -> bool {
    let Some((payload, signature)) = value.split_once('.') else {
        return false;
    };
    let Ok(payload_bytes) = URL_SAFE_NO_PAD.decode(payload) else {
        return false;
    };
    let Ok(signature_bytes) = URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };
    let Ok(payload_text) = std::str::from_utf8(&payload_bytes) else {
        return false;
    };
    let Some((cookie_value, expires_at)) = payload_text.split_once(':') else {
        return false;
    };
    let Ok(expires_at) = expires_at.parse::<u64>() else {
        return false;
    };

    if cookie_value != AUTH_COOKIE_VALUE || expires_at <= unix_timestamp() {
        return false;
    }

    let expected = sign_payload(password, &payload_bytes);
    expected == signature_bytes
}

fn sign_payload(password: &str, payload: &[u8]) -> Vec<u8> {
    let mut mac =
        HmacSha256::new_from_slice(password.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload);

    mac.finalize().into_bytes().to_vec()
}

fn block_result(reason: LoginBlockReason, retry_after_seconds: u64) -> LoginAttemptBlock {
    let message = match reason {
        LoginBlockReason::Banned => "该 IP 已被临时封禁，请 1 小时后再试。",
        LoginBlockReason::RateLimited => "密码错误次数过多，请到下个 10 分钟窗口再试。",
    };

    LoginAttemptBlock {
        reason,
        retry_after_seconds,
        message: message.to_string(),
    }
}

fn retry_after_seconds(until: u64, now: u64) -> u64 {
    until.saturating_sub(now).div_ceil(1000).max(1)
}

fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn monotonic_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_password() -> Config {
        Config {
            app_password: Some("secret".to_string()),
            api_key: None,
            model: "gpt-5.2".to_string(),
            proxy_url: None,
            db_path: ":memory:".to_string(),
            log_limit: 10,
            port: 8787,
            production: false,
        }
    }

    #[test]
    fn auth_is_disabled_when_password_is_missing() {
        let mut config = config_with_password();
        config.app_password = None;

        assert_eq!(
            get_auth_status(&HeaderMap::new(), &config),
            AuthStatus {
                enabled: false,
                authenticated: true
            }
        );
    }

    #[test]
    fn signed_cookie_authenticates_requests() {
        let config = config_with_password();
        let cookie = set_auth_cookie_header(&config).unwrap();
        let cookie_pair = cookie.to_str().unwrap().split(';').next().unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(cookie_pair).unwrap());

        assert_eq!(
            get_auth_status(&headers, &config),
            AuthStatus {
                enabled: true,
                authenticated: true
            }
        );
    }

    #[test]
    fn limiter_blocks_after_repeated_failures_and_clears_on_success() {
        let limiter = LoginAttemptLimiter::default();
        let ip = "203.0.113.10";

        for _ in 0..LOGIN_ATTEMPT_LIMIT {
            assert_eq!(limiter.record_failure(ip), LoginAttemptResult::Allowed);
        }

        assert!(matches!(
            limiter.record_failure(ip),
            LoginAttemptResult::Blocked(LoginAttemptBlock {
                reason: LoginBlockReason::RateLimited,
                ..
            })
        ));

        limiter.record_success(ip);
        assert_eq!(limiter.get_block(ip), LoginAttemptResult::Allowed);
    }

    #[test]
    fn extracts_client_ip_from_forwarded_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.10, 198.51.100.1"),
        );

        assert_eq!(get_client_ip(&headers), "203.0.113.10");
    }
}
