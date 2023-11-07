use std::io::{self, Write};
use std::time::{Duration, UNIX_EPOCH};

use crate::Entry;
use chrono::{DateTime, Local, Utc};
use console::style;

pub fn write_entry<W: Write>(writer: &mut W, entry: &Entry, depth: usize) -> io::Result<()> {
    let datetime = UNIX_EPOCH + Duration::from_micros(entry.stamp);
    let local_time = DateTime::<Utc>::from(datetime).with_timezone(&Local);
    let formatted_time = local_time.format("%H:%M:%S%.3f");

    let prefix = "    ".repeat(depth.saturating_sub(1))
        + if depth > 0 {
            if entry.children.is_empty() {
                "└─ "
            } else {
                "├ "
            }
        } else {
            ""
        };

    let loc = entry.file.as_deref().unwrap_or_default().to_owned()
        + &entry
            .line
            .map_or_else(String::new, |num| format!(":{}", num));

    let message = format!(
        "{} {:>5} {}{}",
        formatted_time,
        entry.level,
        prefix,
        format_entry_message(entry)
    );

    let content_width = console::measure_text_width(&message) + console::measure_text_width(&loc);
    let terminal_width = console::Term::stdout().size().1 as usize;

    writeln!(
        writer,
        "{}{}{}",
        message,
        " ".repeat(terminal_width.saturating_sub(content_width)),
        loc
    )?;

    for child in &entry.children {
        write_entry(writer, child, depth + 1)?;
    }

    Ok(())
}

fn format_entry_message(entry: &Entry) -> String {
    let mut parts = Vec::new();

    if let Some(took) = entry.took {
        parts.push(style(&entry.name).cyan().to_string());
        let ms = took / 1000;
        if ms > 0 {
            parts.push(format!("[{}ms]", ms));
        }
    }

    let fields = entry
        .fields
        .iter()
        .filter(|&(k, _)| k != "message")
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect::<Vec<_>>()
        .join(" ");

    if !fields.is_empty() {
        parts.push(format!("[{}]", fields));
    }

    if let Some(m) = entry.fields.get("message") {
        parts.push(style(m).italic().to_string())
    }

    parts.join(" ")
}
