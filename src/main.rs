// Cargo.toml (additions)
// ----------------------
// anyhow      = "1"
// clap        = { version = "4.5", features = ["derive"] }
// indicatif   = "0.17"
// owo-colors  = "4"
// reqwest     = { version = "0.12", features = ["json", "rustls-tls"] }
// textwrap    = "0.16"
// tokio       = { version = "1",   features = ["macros", "rt-multi-thread"] }

use anyhow::{ Context, Result};
use chrono::{Datelike, Local, NaiveDate, Timelike};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use textwrap::{fill, termwidth};

// ---------- helpers ---------------------------------------------------------

/// Validate an ISO-639-1 language code (two ASCII letters).
fn parse_lang_code(s: &str) -> std::result::Result<String, String> {
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(s.to_ascii_lowercase())
    } else {
        Err(format!(
            "'{s}' is not a valid ISO-639-1 language code (two ASCII letters)"
        ))
    }
}

// ---------- CLI -------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "time-cli", author, version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Display the current time (optionally with statistics)
    Time {
        /// Also show progress through the day / year
        #[arg(short, long)]
        statistics: bool,
    },
    /// Fetch “On This Day” events from Wikipedia
    History(HistoryArgs),
}

#[derive(Parser, Debug)]
struct HistoryArgs {
    /// Wikipedia language code
    #[arg(
        short,
        long,
        value_parser = parse_lang_code,
        value_name = "LANG",
        default_value = "en"
    )]
    language: String,

    /// Suppress the spinner (useful for scripts)
    #[arg(long)]
    quiet: bool,
}

// ---------- Models ----------------------------------------------------------

#[derive(Deserialize, Debug)]
struct OnThisDayResponse {
    events: Vec<Event>,
}

#[derive(Deserialize, Debug)]
struct Event {
    year: i32,
    text: String,
}

// ---------- Main ------------------------------------------------------------

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let client = Client::builder()
        .user_agent(concat!(
        env!("CARGO_PKG_NAME"),
        '/',
        env!("CARGO_PKG_VERSION")
        ))
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build HTTP client")?;

    match cli.command {
        Command::Time { statistics } => {
            let now = Local::now();
            if statistics {
                show_time_statistics(now);
            } else {
                show_current_time(now);
            }
        }
        Command::History(args) => show_on_this_day(&client, &args).await?,
    }

    Ok(())
}

// ---------- History ---------------------------------------------------------

async fn show_on_this_day(client: &Client, args: &HistoryArgs) -> Result<()> {
    let now = Local::now();
    let (month, day) = (now.month(), now.day());

    let pb = if args.quiet {
        None
    } else {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?
                .tick_strings(&[
                    "⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏",
                ]),
        );
        pb.set_message(format!(
            "Fetching events for {month:02}-{day:02} in '{lang}'...",
            month = month,
            day = day,
            lang = args.language
        ));
        Some(pb)
    };

    let url = format!(
        "https://{lang}.wikipedia.org/api/rest_v1/feed/onthisday/events/{month}/{day}",
        lang = args.language
    );

    let resp = client
        .get(url)
        .send()
        .await
        .context("Network error contacting Wikipedia")?
        .error_for_status()
        .context("Wikipedia returned an error status")?;

    let events: OnThisDayResponse = resp
        .json()
        .await
        .context("Invalid JSON returned by Wikipedia")?;

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    println!(
        "{} {}",
        "--- On This Day:".bold(),
        now.format("%B %e,").to_string().trim()
    );
    let width = termwidth().max(40); // sensible minimum
    for event in events.events.iter().rev() {
        println!(
            "[{}] {}",
            event.year.yellow(),
            fill(&event.text, width)
        );
    }

    Ok(())
}

// ---------- Time display ----------------------------------------------------

fn show_current_time(now: chrono::DateTime<Local>) {
    println!(
        "{}\n{}",
        "The current time is:".bold(),
        now.format("%A, %B %d, %Y %r")
    );
}

// Statistics are separated so they can be unit-tested easily -----------------

#[derive(Debug, Copy, Clone)]
#[must_use]
struct TimeStats {
    day_of_year: u32,
    total_days_in_year: u32,
    day_progress: f64,  // 0–100
    year_progress: f64, // 0–100
    week_of_year: u32,
    is_leap: bool,
    unix_timestamp: i64,
}

fn compute_time_statistics(now: chrono::DateTime<Local>) -> TimeStats {
    let year = now.year();
    let is_leap =
        NaiveDate::from_ymd_opt(year, 12, 31).unwrap().ordinal() == 366;

    let seconds_into_day =
        now.hour() * 3600 + now.minute() * 60 + now.second();
    let seconds_in_day = 86_400;
    let day_progress =
        (seconds_into_day as f64 / seconds_in_day as f64) * 100.0;

    let day_of_year = now.ordinal();
    let total_days_in_year = if is_leap { 366 } else { 365 };
    let year_progress =
        (day_of_year as f64 / total_days_in_year as f64) * 100.0;

    TimeStats {
        day_of_year,
        total_days_in_year,
        day_progress,
        year_progress,
        week_of_year: now.iso_week().week(),
        is_leap,
        unix_timestamp: now.timestamp(),
    }
}

fn show_time_statistics(now: chrono::DateTime<Local>) {
    let stats = compute_time_statistics(now);

    println!("{}\n----------------", "Time Statistics:".bold());
    println!(
        "Day of the year: {}/{}",
        stats.day_of_year, stats.total_days_in_year
    );
    println!("Week of the year: {}", stats.week_of_year);
    println!(
        "Is it a leap year? {}",
        if stats.is_leap { "Yes" } else { "No" }
    );
    println!("Seconds since Unix epoch: {}", stats.unix_timestamp);
    println!("\nProgress:");
    println!("Day is {:.2}% complete", stats.day_progress);
    println!("Year is {:.2}% complete", stats.year_progress);
}

// ---------- Tests -----------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn leap_year_statistics() {
        let dt = Local
            .with_ymd_and_hms(2024, 3, 1, 0, 0, 0)
            .unwrap();
        let stats = compute_time_statistics(dt);
        assert!(stats.is_leap);
        assert_eq!(stats.total_days_in_year, 366);
        // 1 March in a leap year is day 61
        assert_eq!(stats.day_of_year, 61);
    }

    #[test]
    fn non_leap_year() {
        let dt = Local
            .with_ymd_and_hms(2025, 3, 1, 0, 0, 0)
            .unwrap();
        let stats = compute_time_statistics(dt);
        assert!(!stats.is_leap);
        assert_eq!(stats.total_days_in_year, 365);
    }

    #[test]
    fn parse_lang_code_ok() {
        assert_eq!(parse_lang_code("de").unwrap(), "de");
        assert_eq!(parse_lang_code("EN").unwrap(), "en");
    }

    #[test]
    fn parse_lang_code_err() {
        assert!(parse_lang_code("eng").is_err());
        assert!(parse_lang_code("1a").is_err());
    }
}