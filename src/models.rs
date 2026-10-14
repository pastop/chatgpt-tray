use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UsageResponse {
    pub plan_type: String,
    pub rate_limit: RateLimit,
}

#[derive(Debug, Deserialize)]
pub struct RateLimit {
    pub allowed: bool,
    pub limit_reached: bool,
    pub primary_window: Option<UsageWindow>,
    pub secondary_window: Option<UsageWindow>,
}

#[derive(Debug, Deserialize)]
pub struct UsageWindow {
    pub used_percent: f64,
    pub limit_window_seconds: u64,
    pub reset_after_seconds: u64,
    pub reset_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct CodexAuth {
    pub tokens: CodexTokens,
}

#[derive(Debug, Deserialize)]
pub struct CodexTokens {
    pub access_token: String,
}
