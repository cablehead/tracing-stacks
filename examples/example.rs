use std::io::{self, Write};
use std::time::{Duration, UNIX_EPOCH};

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use tokio::sync::broadcast;

use chrono::{DateTime, Utc};

use console::style;

use tracing_layer_lib::{Entry, RootSpanLayer};

#[tracing::instrument]
fn more(x: u32) {
    tracing::info!(info = "yes", "more!");
}

#[tracing::instrument]
fn foobar() {
    more(3);
    more(5);
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = broadcast::channel(16);

    // Spawn a new async task to receive and write messages to stdout
    let logger = tokio::spawn(async move {
        let mut stdout = std::io::stdout();
        while let Ok(entry) = rx.recv().await {
            write_entry(&mut stdout, &entry, 0).unwrap();
        }
    });

    {
        let _subscriber = tracing_subscriber::Registry::default()
            .with(RootSpanLayer::new(tx, None))
            .set_default();

        tracing::info!("let's go!");
        foobar();
    }

    let _ = logger.await;
}

fn write_entry<W: Write>(writer: &mut W, entry: &Entry, depth: usize) -> io::Result<()> {
    let datetime = UNIX_EPOCH + Duration::from_micros(entry.stamp);
    let datetime: DateTime<Utc> = DateTime::from(datetime);
    let formatted_time = datetime.format("%H:%M:%S%.3f");

    let prefix = match depth {
        0 => "".to_string(),
        _ => format!(
            "{}{} ",
            "    ".repeat(depth - 1),
            if entry.children.is_empty() {
                "└─"
            } else {
                "├"
            }
        ),
    };

    let loc = format!(
        "{}:{}",
        entry.file.as_ref().map_or("", |f| f.as_str()),
        entry
            .line
            .map_or_else(|| "".to_string(), |num| num.to_string())
    );

    let message = format!(
        "{} {:>5} {}{}",
        formatted_time,
        entry.level,
        prefix,
        format_entry_message(entry),
    );

    let content_width =
        console::measure_text_width(message.as_str()) + console::measure_text_width(loc.as_str());
    let (_, terminal_width) = console::Term::stdout().size();

    let pad = " ".repeat((terminal_width as usize).saturating_sub(content_width));
    writeln!(writer, "{}{}{}", message, pad, loc)?;

    for child in &entry.children {
        write_entry(writer, child, depth + 1)?;
    }

    Ok(())
}

fn format_entry_message(entry: &Entry) -> String {
    let mut parts = vec![];

    if entry.took > 0 {
        parts.push(format!("{} ({}us)", style(&entry.name).cyan(), entry.took));
    }

    let mut fields = entry.fields.clone();

    let message = fields.remove("message");

    if !fields.is_empty() {
        parts.push(format!(
            "[{}]",
            fields
                .iter()
                .map(|(key, value)| format!("{}:{}", key, value))
                .collect::<Vec<String>>()
                .join(" ")
        ));
    }

    if let Some(message) = message {
        parts.push(style(message).italic().to_string());
    }

    parts.join(" ")
}
