use anyhow::Result;
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct TelegramReport {
    pub status: String,
    pub report_path: String,
    pub sent_at: String,
    pub response: String,
}

pub fn send_telegram_report(
    token: &str,
    chat_id: &str,
    report_content: &str,
    report_path: &str,
) -> Result<TelegramReport> {
    use chrono::Utc;

    // In a real environment, we'd use a reqwest client, but since we want to avoid
    // adding new heavy dependencies and the prompt allows an 'operational bridge',
    // we'll use curl if available, otherwise simulate.

    let mut response = "simulated".to_string();
    let mut status = "sent_simulated".to_string();

    if !token.is_empty() && !chat_id.is_empty() {
        // Try to send the document via curl
        let output = Command::new("curl")
            .args([
                "-s",
                "-X",
                "POST",
                &format!("https://api.telegram.org/bot{}/sendDocument", token),
                "-F",
                &format!("chat_id={}", chat_id),
                "-F",
                &format!("document=@{}", report_path),
                "-F",
                &format!(
                    "caption={}",
                    report_content
                        .lines()
                        .next()
                        .unwrap_or("Forge Self-Evolution Report")
                ),
            ])
            .output();

        match output {
            Ok(out) if out.status.success() => {
                response = String::from_utf8_lossy(&out.stdout).to_string();
                status = "sent_real".to_string();
            }
            Ok(out) => {
                response = String::from_utf8_lossy(&out.stderr).to_string();
                status = "failed_real".to_string();
            }
            Err(e) => {
                response = format!("curl failed: {}", e);
                status = "failed_exec".to_string();
            }
        }
    }

    Ok(TelegramReport {
        status,
        report_path: report_path.to_string(),
        sent_at: Utc::now().to_rfc3339(),
        response,
    })
}
