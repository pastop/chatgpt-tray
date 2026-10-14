use anyhow::Result;
use reqwest::Client;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::models::UsageResponse;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Debug, Clone, Copy)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    RateLimited { retry_after: Option<Duration> },
    Server,
    Network,
    InvalidResponse,
    Http,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unauthorized => "HTTP 401: требуется повторная авторизация в Codex.",
            Self::Forbidden => "HTTP 403: доступ к статистике запрещён.",
            Self::RateLimited { .. } => {
                "HTTP 429: слишком много запросов. Повтор будет выполнен после паузы."
            }
            Self::Server => "Сервис статистики временно недоступен (HTTP 5xx).",
            Self::Network => "Не удалось подключиться к сервису статистики. Проверьте сеть.",
            Self::InvalidResponse => "Сервис вернул некорректные данные статистики.",
            Self::Http => "Сервис отклонил запрос статистики.",
        })
    }
}
impl std::error::Error for ApiError {}

fn retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let seconds = value.trim().parse::<u64>().ok().or_else(|| {
        let date = chrono::DateTime::parse_from_rfc2822(value.trim()).ok()?;
        let now = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
        Some((date.timestamp().max(0) as u64).saturating_sub(now))
    })?;
    // Bound untrusted server hints: never spin, never suspend retries indefinitely.
    Some(Duration::from_secs(seconds.clamp(60, 86_400)))
}

fn response_error(status: reqwest::StatusCode, hint: Option<&str>) -> ApiError {
    match status.as_u16() {
        401 => ApiError::Unauthorized,
        403 => ApiError::Forbidden,
        429 => ApiError::RateLimited {
            retry_after: hint.and_then(|value| retry_after(value, SystemTime::now())),
        },
        500..=599 => ApiError::Server,
        _ => ApiError::Http,
    }
}

pub async fn fetch_usage(client: &Client, token: &str) -> Result<UsageResponse> {
    fetch_usage_at(client, token, USAGE_URL).await
}

async fn fetch_usage_at(client: &Client, token: &str, url: &str) -> Result<UsageResponse> {
    let response = client
        .get(url)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| ApiError::Network)?;
    if !response.status().is_success() {
        return Err(response_error(
            response.status(),
            response
                .headers()
                .get("Retry-After")
                .and_then(|v| v.to_str().ok()),
        )
        .into());
    }
    response.json::<UsageResponse>().await.map_err(|error| {
        if error.is_decode() {
            ApiError::InvalidResponse
        } else {
            ApiError::Network
        }
        .into()
    })
}

/// Shared automatic/manual gate; the caller owns the single sequential request loop.
pub struct RetryPolicy {
    failures: u32,
    next: Instant,
}

impl RetryPolicy {
    pub fn new(now: Instant) -> Self {
        Self {
            failures: 0,
            next: now,
        }
    }
    pub fn deadline(&self) -> Instant {
        self.next
    }
    pub fn ready(&self, now: Instant) -> bool {
        now >= self.next
    }
    pub fn success(&mut self, now: Instant) {
        self.failures = 0;
        self.next = now + Duration::from_secs(300);
    }
    pub fn failure(&mut self, now: Instant, error: ApiError) {
        self.failures = self.failures.saturating_add(1);
        let backoff = Duration::from_secs(
            (30_u64 * (1_u64 << self.failures.min(6).saturating_sub(1))).min(900),
        );
        let delay = match error {
            ApiError::Unauthorized | ApiError::Forbidden => Duration::from_secs(900),
            ApiError::RateLimited { retry_after } => {
                retry_after.unwrap_or(Duration::from_secs(300)).max(backoff)
            }
            _ => backoff,
        };
        self.next = now + delay;
    }
    pub fn manual_ready(&self, now: Instant) -> bool {
        self.failures == 0 || self.ready(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hints_are_bounded_and_support_http_dates() {
        let now = UNIX_EPOCH + Duration::from_secs(1_445_412_480);
        assert_eq!(
            retry_after("Wed, 21 Oct 2015 07:30:00 GMT", now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(retry_after("0", now), Some(Duration::from_secs(60)));
        assert_eq!(retry_after("999999", now), Some(Duration::from_secs(86400)));
        assert_eq!(retry_after("invalid", now), None);
    }
    #[test]
    fn cooldowns_block_manual_and_reset_only_on_success() {
        let now = Instant::now();
        let mut policy = RetryPolicy::new(now);
        for error in [
            ApiError::Unauthorized,
            ApiError::Forbidden,
            ApiError::RateLimited {
                retry_after: Some(Duration::from_secs(1800)),
            },
        ] {
            policy.failure(now, error);
            assert!(!policy.manual_ready(now + Duration::from_secs(899)));
            assert!(policy.ready(policy.deadline()));
        }
        policy.success(now);
        assert!(policy.manual_ready(now));
        policy.failure(now, ApiError::Network);
        assert_eq!(policy.deadline() - now, Duration::from_secs(30));
        for _ in 0..100 {
            policy.failure(now, ApiError::Server);
        }
        assert_eq!(policy.deadline() - now, Duration::from_secs(900));
    }
    #[tokio::test]
    async fn mock_invalid_json_and_network_errors_are_safe() {
        use std::io::{Read, Write};
        for disconnect in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0; 4096];
                stream.read(&mut buffer).unwrap();
                if !disconnect {
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecret").unwrap();
                }
            });
            let client = Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let error = fetch_usage_at(&client, "secret-token", &url)
                .await
                .unwrap_err();
            assert!(!format!("{error:#}").contains("secret"));
            assert!(!format!("{error:#}").contains(&url));
            let typed = error.downcast_ref::<ApiError>().unwrap();
            if disconnect {
                assert!(matches!(typed, ApiError::Network));
            } else {
                assert!(matches!(typed, ApiError::InvalidResponse));
            }
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn mock_http_errors_do_not_expose_response_or_token() {
        use std::io::{Read, Write};
        for code in [401, 403, 429, 503] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}/", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut buffer = [0; 4096];
                let _ = stream.read(&mut buffer).unwrap();
                write!(stream, "HTTP/1.1 {code} Error\r\nRetry-After: 1800\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecret").unwrap();
            });
            let client = Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let error = fetch_usage_at(&client, "secret-token", &url)
                .await
                .unwrap_err();
            assert!(!format!("{error:#}").contains("secret"));
            let typed = error.downcast_ref::<ApiError>().unwrap();
            if code == 429 {
                assert!(
                    matches!(typed, ApiError::RateLimited { retry_after: Some(d) } if *d == Duration::from_secs(1800))
                );
            }
            server.join().unwrap();
        }
    }
}
