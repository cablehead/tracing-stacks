#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use tokio::sync::broadcast;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    use crate::{Monitor, RootSpanLayer};

    #[tracing::instrument]
    fn more(x: u32) {
        tracing::info!(info = "yes", "more!");
    }

    #[tracing::instrument]
    fn foobar() {
        more(3);
    }

    #[tokio::test]
    async fn test_layer() {
        let (tx, mut rx) = broadcast::channel(16);

        let monitor = Arc::new(Mutex::new(Monitor { span_count: 0 }));

        {
            let _subscriber = tracing_subscriber::Registry::default()
                .with(RootSpanLayer::new(tx, Some(monitor.clone())))
                .set_default();

            foobar();
        }

        assert_eq!(monitor.lock().unwrap().span_count, 0);

        let entry = rx.recv().await.unwrap();
        assert_eq!(entry.name, "foobar");
    }
}
