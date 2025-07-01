use chrono::{Datelike, Local, NaiveDate, Timelike};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Displays the current time and statistics
    Time(TimeArgs),
    /// Fetches "On This Day" historical events from Wikipedia
    History(HistoryArgs),
}

#[derive(Parser, Debug)]
struct TimeArgs {
    /// Show detailed statistics about the current time
    #[arg(short, long)]
    statistics: bool,
}

#[derive(Parser, Debug)]
struct HistoryArgs {
    /// The language for the history events (e.g., "en", "de", "fr")
    #[arg(short, long, default_value_t = String::from("en"))]
    language: String,
}

#[derive(Deserialize, Debug)]
struct OnThisDayResponse {
    events: Vec<Event>,
}

#[derive(Deserialize, Debug)]
struct Event {
    year: i32,
    text: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Time(args) => {
            let now = Local::now();
            if args.statistics {
                show_time_statistics(now);
            } else {
                show_current_time(now);
            }
        }
        Commands::History(args) => {
            show_on_this_day(&args.language).await?;
        }
    }

    Ok(())
}

async fn show_on_this_day(lang: &str) -> Result<(), Box<dyn std::error::Error>> {
    let now = Local::now();
    let month = now.month();
    let day = now.day();

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.blue} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(format!(
        "Fetching events for {:02}-{:02} in '{}'...",
        month, day, lang
    ));

    let url = format!(
        "https://{}.wikipedia.org/api/rest_v1/feed/onthisday/events/{}/{}",
        lang, month, day
    );

    let response = reqwest::get(&url).await?;

    if !response.status().is_success() {
        pb.finish_with_message("❌ Error fetching data.");
        eprintln!(
            "Failed to get a successful response from the API for language '{}'.",
            lang
        );
        eprintln!("Please ensure '{}' is a valid Wikipedia language code (e.g., 'en', 'de').", lang);
        return Ok(());
    }

    let on_this_day: OnThisDayResponse = response.json().await?;
    pb.finish_with_message("✅ Done!");

    println!("\n--- On This Day: {} {} ---", now.format("%B"), day);
    for event in on_this_day.events.iter().rev() {
        println!("[{}] {}", event.year, event.text);
    }

    Ok(())
}

fn show_current_time(now: chrono::DateTime<Local>) {
    let formatted_time = now.format("%A, %B %d, %Y %r").to_string();
    println!("The current time is:");
    println!("{}", formatted_time);
}

fn show_time_statistics(now: chrono::DateTime<Local>) {
    let year = now.year();
    let is_leap = NaiveDate::from_ymd_opt(year, 12, 31).unwrap().ordinal() == 366;
    let seconds_into_day = now.hour() * 3600 + now.minute() * 60 + now.second();
    let seconds_in_day = 24 * 3600;
    let day_progress = (seconds_into_day as f64 / seconds_in_day as f64) * 100.0;
    let day_of_year = now.ordinal();
    let total_days_in_year = if is_leap { 366 } else { 365 };
    let year_progress = (day_of_year as f64 / total_days_in_year as f64) * 100.0;

    println!("Time Statistics:");
    println!("----------------");
    println!("Day of the year: {}/{}", day_of_year, total_days_in_year);
    println!("Week of the year: {}", now.iso_week().week());
    println!("Is it a leap year? {}", if is_leap { "Yes" } else { "No" });
    println!("Seconds since Unix epoch: {}", now.timestamp());
    println!("\nProgress:");
    println!("Day is {:.2}% complete", day_progress);
    println!("Year is {:.2}% complete", year_progress);
}