use chatgpt_tray::{api, auth, models};

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Local, Utc};
use models::UsageWindow;

fn print_window(name: &str, window: &UsageWindow) {
    let remaining = (100.0 - window.used_percent).clamp(0.0, 100.0);

    let reset_time = DateTime::<Utc>::from_timestamp(window.reset_at, 0)
        .map(|time| time.with_timezone(&Local))
        .map(|time| time.format("%d.%m.%Y %H:%M:%S").to_string())
        .unwrap_or_else(|| "Неизвестно".to_string());

    println!("\n{name}");
    println!("  Использовано: {:.1}%", window.used_percent);
    println!("  Осталось:     {:.1}%", remaining);
    println!(
        "  Сброс через:  {} ч {} мин",
        window.reset_after_seconds / 3600,
        (window.reset_after_seconds % 3600) / 60
    );
    println!("  Время сброса: {reset_time}");
}

#[tokio::main]
async fn main() -> Result<()> {
    let token = auth::read_access_token()?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;

    let usage = api::fetch_usage(&client, &token).await?;

    println!("ChatGPT Usage Tray");
    println!("Тариф: {}", usage.plan_type);
    println!("Доступ: {}", usage.rate_limit.allowed);
    println!("Лимит достигнут: {}", usage.rate_limit.limit_reached);

    if let Some(window) = &usage.rate_limit.primary_window {
        print_window("5-часовой лимит", window);
    }

    if let Some(window) = &usage.rate_limit.secondary_window {
        print_window("Недельный лимит", window);
    }

    Ok(())
}
