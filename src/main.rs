use anyhow::{bail, Context, Result};
use chrono::{Datelike, Local, NaiveDate, Timelike};
use clap::{Parser, Subcommand, ValueEnum};
use comfy_table::{
    presets::UTF8_FULL, Attribute, Cell, Color, ContentArrangement, Row, Table,
};
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;
use textwrap::{fill, termwidth};

/* --------------------------------------------------------------------------
 *                                helpers
 * ---------------------------------------------------------------------- */

/// Validate an ISO-639-1 language code (exactly two ASCII letters).
fn parse_lang_code(s: &str) -> std::result::Result<String, String> {
    if s.len() == 2 && s.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(s.to_ascii_lowercase())
    } else {
        Err(format!(
            "'{s}' is not a valid ISO-639-1 language code \
             (two ASCII letters)",
        ))
    }
}

/* --------------------------------------------------------------------------
 *                                  CLI
 * ---------------------------------------------------------------------- */

#[derive(Parser, Debug)]
#[command(
    name = "time-cli",
    author,
    version,
    about = "Tiny CLI that prints the current time and \
             Wikipedia “On This Day” events",
    propagate_version = true,
    color = clap::ColorChoice::Always,
    after_long_help = "Project home: https://github.com/your/repo",
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Also show progress through the day / year
    #[arg(short, long)]
    statistics: bool,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum EventType {
    Events,
    Births,
    Deaths,
    Holidays,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Fetch “On This Day” events from Wikipedia
    History(HistoryArgs),
}

#[derive(Parser, Debug)]
struct HistoryArgs {
    /// Type of events to show
    #[arg(
        short = 't',
        long,
        value_enum,
        default_value_t = EventType::Events,
        value_name = "TYPE",
    )]
    r#type: EventType,

    /// Wikipedia language code
    #[arg(
        short,
        long,
        value_parser = parse_lang_code,
        value_name = "LANG",
        default_value = "en",
    )]
    language: String,

    /// Suppress the spinner (useful for scripts)
    #[arg(long)]
    quiet: bool,

    /// Override month (1-12). Defaults to the current month.
    #[arg(
        short = 'm',
        long,
        value_name = "MONTH",
        value_parser = clap::value_parser!(u32).range(1..=12),
    )]
    month: Option<u32>,

    /// Override day of the month (1-31). Defaults to the current day.
    #[arg(
        short = 'd',
        long,
        value_name = "DAY",
        value_parser = clap::value_parser!(u32).range(1..=31),
    )]
    day: Option<u32>,
}

/* --------------------------------------------------------------------------
 *                                models
 * ---------------------------------------------------------------------- */

#[derive(Deserialize, Debug)]
struct OnThisDayResponse {
    #[serde(default)]
    events: Vec<Event>,
    #[serde(default)]
    births: Vec<Event>,
    #[serde(default)]
    deaths: Vec<Event>,
    #[serde(default)]
    holidays: Vec<Holiday>,
}

#[derive(Deserialize, Debug)]
struct Event {
    year: i32,
    text: String,
}

#[derive(Deserialize, Debug)]
struct Holiday {
    text: String,
}

/* --------------------------------------------------------------------------
 *                                 main
 * ---------------------------------------------------------------------- */

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
        Some(Command::History(args)) => {
            show_on_this_day(&client, &args).await?
        }
        None => {
            let now = Local::now();
            if cli.statistics {
                show_time_statistics(now);
            } else {
                show_current_time(now);
            }
        }
    }

    Ok(())
}

/* --------------------------------------------------------------------------
 *                              Wikipedia
 * ---------------------------------------------------------------------- */

async fn show_on_this_day(client: &Client, args: &HistoryArgs) -> Result<()> {
    // Determine the requested calendar day
    let today = Local::now();
    let month = args.month.unwrap_or(today.month());
    let day = args.day.unwrap_or(today.day());

    // Validate the month/day combination (use leap year for “Feb-29”)
    if NaiveDate::from_ymd_opt(2024, month, day).is_none() {
        bail!("'{month:02}-{day:02}' is not a valid calendar date");
    }

    // Optional spinner
    let spinner = if args.quiet {
        None
    } else {
        let pb = ProgressBar::new_spinner();
        pb.enable_steady_tick(Duration::from_millis(120));
        pb.set_style(
            ProgressStyle::with_template("{spinner:.blue} {msg}")?.tick_strings(
                &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            ),
        );
        pb.set_message(format!(
            "Fetching {event_type:?} for {month:02}-{day:02} ({lang})",
            event_type = args.r#type,
            month = month,
            day = day,
            lang = args.language,
        ));
        Some(pb)
    };

    // Wikipedia API URL
    let event_type_str = format!("{:?}", args.r#type).to_ascii_lowercase();
    let url = format!(
        "https://{lang}.wikipedia.org/api/rest_v1/feed/onthisday/{event_type}/{month}/{day}",
        lang = args.language,
        event_type = event_type_str,
    );

    // Fetch & parse JSON
    let response: OnThisDayResponse = client
        .get(url)
        .send()
        .await
        .context("Network error contacting Wikipedia")?
        .error_for_status()
        .context("Wikipedia returned an error status")?
        .json()
        .await
        .context("Invalid JSON returned by Wikipedia")?;

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }

    /* ----------- pretty table ----------- */
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    let width = termwidth().max(50); // sensible minimum

    match args.r#type {
        EventType::Holidays => {
            table.set_header(vec![Cell::new("Holidays & Observances")
                .add_attribute(Attribute::Bold)]);
            if response.holidays.is_empty() {
                table.add_row(vec!["No holidays found for this day."]);
            } else {
                for holiday in &response.holidays {
                    table.add_row(vec![Cell::new(fill(
                        &holiday.text,
                        width - 5,
                    ))]);
                }
            }
        }
        _ => {
            let (header1, header2, events) = match args.r#type {
                EventType::Events => ("Year", "Event", &response.events),
                EventType::Births => ("Born", "Person", &response.births),
                EventType::Deaths => ("Died", "Person", &response.deaths),
                EventType::Holidays => unreachable!(),
            };

            table.set_header(vec![
                Cell::new(header1).add_attribute(Attribute::Bold),
                Cell::new(header2).add_attribute(Attribute::Bold),
            ]);

            if events.is_empty() {
                table.add_row(vec![
                    Cell::new("N/A"),
                    Cell::new("No entries of this type found for this day."),
                ]);
            } else {
                for ev in events.iter().rev() {
                    table.add_row(Row::from(vec![
                        Cell::new(ev.year)
                            .fg(Color::Yellow)
                            .add_attribute(Attribute::Bold),
                        Cell::new(fill(&ev.text, width - 15)),
                    ]));
                }
            }
        }
    }

    // Nice human-readable header for the requested day
    let fake_year = 2024; // leap year → Feb-29 always valid
    let header_date = NaiveDate::from_ymd_opt(fake_year, month, day).unwrap();
    println!(
        "{} {}\n",
        "— On This Day:".bold().underline(),
        header_date.format("%B %e").to_string().trim(),
    );
    println!("{table}");

    Ok(())
}

/* --------------------------------------------------------------------------
 *                              time output
 * ---------------------------------------------------------------------- */

fn show_current_time(now: chrono::DateTime<Local>) {
    println!(
        "{}\n{}",
        "The current time is:".bold(),
        now.format("%A, %B %d, %Y %r"),
    );
}

fn ascii_bar(percent: f64, width: usize) -> String {
    let filled = ((percent / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!(
        "{}{}",
        "█".repeat(filled).green(),
        "░".repeat(empty).dimmed(),
    )
}

/* --------------------------------------------------------------------------
 *                            time statistics
 * ---------------------------------------------------------------------- */

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
    let bar_width = 28;

    println!("\n{}\n{}", "Time statistics".bold(), "─".repeat(35));
    println!("Date            : {}", now.format("%A, %B %d %Y"));
    println!("Local time      : {}", now.format("%r"));
    println!("Unix timestamp  : {}", stats.unix_timestamp);

    println!(
        "\nDay   ({}/{}) : {} {:>5.1} %",
        stats.day_of_year,
        stats.total_days_in_year,
        ascii_bar(stats.day_progress, bar_width),
        stats.day_progress,
    );

    println!(
        "Year  (week {}) : {} {:>5.1} %",
        stats.week_of_year,
        ascii_bar(stats.year_progress, bar_width),
        stats.year_progress,
    );

    println!(
        "\nLeap year       : {}",
        if stats.is_leap {
            "Yes".bright_green().to_string()
        } else {
            "No".bright_red().to_string()
        },
    );
}

/* --------------------------------------------------------------------------
 *                                 tests
 * ---------------------------------------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn leap_year_statistics() {
        let dt = Local.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        let stats = compute_time_statistics(dt);
        assert!(stats.is_leap);
        assert_eq!(stats.total_days_in_year, 366);
        // 1 March in a leap year is day 61
        assert_eq!(stats.day_of_year, 61);
    }

    #[test]
    fn non_leap_year() {
        let dt = Local.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
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

    #[test]
    fn custom_date_validation() {
        // Valid
        assert!(NaiveDate::from_ymd_opt(2024, 2, 29).is_some());
        // Invalid
        assert!(NaiveDate::from_ymd_opt(2024, 4, 31).is_none());
    }
}