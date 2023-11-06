use tracing_layer_lib::{Entry, RootSpanLayer};

use tracing_subscriber::util::SubscriberInitExt;

use chrono::{DateTime, Utc};
use std::io::{self, Write};
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::broadcast;
use tracing_subscriber::layer::SubscriberExt;

#[tracing::instrument]
fn more(x: u32) {
    tracing::info!(info = "yes", "more!");
}

#[tracing::instrument]
fn foobar() {
    more(3);
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = broadcast::channel(16);

    tracing_subscriber::Registry::default()
        .with(RootSpanLayer::new(tx.clone(), None))
        .init();

    // Spawn a new async task to receive and write messages to stdout
    tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        while let Ok(entry) = rx.recv().await {
            write_entry(&mut stdout, &entry, 0).unwrap();
        }
    });

    tracing::info!("let's go!");

    let handle = std::thread::spawn(|| {
        foobar();
    });
    foobar();
    handle.join().unwrap();
}

fn write_entry<W: Write>(writer: &mut W, entry: &Entry, depth: usize) -> io::Result<()> {
    let datetime = UNIX_EPOCH + Duration::from_micros(entry.stamp);
    let datetime: DateTime<Utc> = DateTime::from(datetime);
    let formatted_time = datetime.format("%Y-%m-%dT%H:%M:%S.%f");

    writeln!(
        writer,
        "{} {}:{} {}{} {}",
        formatted_time,
        entry.file.as_ref().map_or("", |f| f.as_str()),
        entry.line.unwrap_or(0),
        entry.level,
        "    ".repeat(depth), // Indentation
        format_entry_message(entry)
    )?;

    for child in &entry.children {
        write_entry(writer, child, depth + 1)?;
    }

    Ok(())
}

fn format_entry_message(entry: &Entry) -> String {
    let mut parts = vec![];

    if entry.took > 0 {
        parts.push(format!("[{} {}us]", entry.name, entry.took));
    }

    for (key, value) in &entry.fields {
        if key != "message" {
            parts.push(format!("{}={}", key, value));
        }
    }

    if let Some(message) = entry.fields.get("message") {
        parts.push(format!(":: {}", message));
    }

    parts.join(" ")
}
