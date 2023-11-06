use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use tokio::sync::broadcast;

use tracing_stacks::{fmt::write_entry, RootSpanLayer};

#[tracing::instrument]
fn more(x: u32) {
    tracing::info!(action = "yes", "more!");
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
