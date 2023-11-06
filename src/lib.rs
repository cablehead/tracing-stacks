use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use tokio::sync::broadcast;

use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::Subscriber;
use tracing::{Event, Level};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

pub struct Monitor {
    pub span_count: usize,
}

impl Monitor {
    pub fn notify(&mut self, spans: &HashMap<Id, Scope>) {
        self.span_count = spans.len();
    }
}

#[derive(Debug, Clone)]
enum Child {
    Event(Scope),
    Span(Id),
}

#[derive(Debug, Clone)]
pub struct Scope {
    stamp: SystemTime,
    level: Level,
    name: String,
    parent_id: Option<Id>,
    children: Vec<Child>,
    file: Option<String>,
    line: Option<u32>,
    start_time: Option<Instant>,
    took: u128, // Stores duration in microseconds
    fields: HashMap<String, String>,
}

impl Visit for Scope {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_string(), format!("{:?}", value));
    }
}

impl Scope {
    fn new(
        level: Level,
        name: String,
        parent_id: Option<Id>,
        file: Option<String>,
        line: Option<u32>,
    ) -> Self {
        Self {
            stamp: SystemTime::now(),
            level,
            name,
            parent_id,
            children: Vec::new(),
            file,
            line,
            start_time: None,
            took: 0,
            fields: HashMap::new(),
        }
    }

    fn to_entry(&self) -> Entry {
        Entry {
            stamp: self
                .stamp
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_micros() as u64,
            level: self.level.to_string(),
            name: self.name.clone(),
            file: self.file.clone(),
            line: self.line,
            took: self.took,
            fields: self.fields.clone(),
            children: Vec::new(), // This method does not handle children; they must be added separately if needed
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entry {
    pub stamp: u64,
    pub level: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "is_zero")]
    pub took: u128,
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub fields: HashMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<Entry>,
}

fn is_zero(num: &u128) -> bool {
    *num == 0
}

fn extract_span_root(root_id: Id, spans: &mut HashMap<Id, Scope>) -> Entry {
    let mut root_scope = spans.remove(&root_id).unwrap();
    let mut entry = root_scope.to_entry();
    entry.children = root_scope
        .children
        .drain(..)
        .map(|child| match child {
            Child::Span(child_id) => extract_span_root(child_id, spans),
            Child::Event(event_scope) => event_scope.to_entry(),
        })
        .collect::<Vec<_>>();
    entry
}

pub struct RootSpanLayer {
    spans: Mutex<HashMap<Id, Scope>>,
    sender: broadcast::Sender<Entry>,
    monitor: Option<Arc<Mutex<Monitor>>>,
}

impl RootSpanLayer {
    pub fn new(sender: broadcast::Sender<Entry>, monitor: Option<Arc<Mutex<Monitor>>>) -> Self {
        RootSpanLayer {
            spans: Mutex::new(HashMap::new()),
            sender,
            monitor,
        }
    }
}

impl<S> Layer<S> for RootSpanLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let metadata = attrs.metadata();

        let curr = ctx.current_span();
        let parent_id = curr.id();

        let mut scope = Scope::new(
            *metadata.level(),
            metadata.name().to_string(),
            parent_id.cloned(),
            metadata.file().map(ToString::to_string),
            metadata.line(),
        );
        attrs.record(&mut scope);

        let mut spans = self.spans.lock().unwrap();

        // If the span has a parent, add it to the parent's children
        if let Some(parent_id) = &parent_id {
            if let Some(parent_scope) = spans.get_mut(parent_id) {
                parent_scope.children.push(Child::Span(id.clone()));
            }
        }

        spans.insert(id.clone(), scope);
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let mut scope = Scope::new(
            *metadata.level(),
            metadata.name().to_string(),
            ctx.current_span().id().cloned(),
            metadata.file().map(ToString::to_string),
            metadata.line(),
        );
        event.record(&mut scope);

        if let Some(parent_span_id) = ctx.current_span().id() {
            if let Ok(mut spans) = self.spans.lock() {
                if let Some(parent_scope) = spans.get_mut(parent_span_id) {
                    parent_scope.children.push(Child::Event(scope));
                }
            }
        } else {
            // If there's no parent, we send the Event immediately as an Entry
            let entry = scope.to_entry();
            self.sender.send(entry).unwrap();
        }
    }

    fn on_enter(&self, id: &Id, _ctx: Context<'_, S>) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(scope) = spans.get_mut(id) {
            scope.start_time = Some(Instant::now());
        }
    }

    fn on_exit(&self, id: &Id, _ctx: Context<'_, S>) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(scope) = spans.get_mut(id) {
            if let Some(start_time) = scope.start_time {
                let elapsed = start_time.elapsed().as_micros();
                scope.took += elapsed;
                scope.start_time = None; // Reset the start time
            }
        }
    }

    fn on_close(&self, id: Id, _ctx: Context<'_, S>) {
        let mut spans = self.spans.lock().unwrap();
        if let Some(scope) = spans.get(&id) {
            if scope.parent_id.is_none() {
                let inlined_scope = extract_span_root(id.clone(), &mut spans);
                self.sender.send(inlined_scope).unwrap();
            }
            if let Some(monitor) = &self.monitor {
                monitor.lock().unwrap().notify(&spans);
            }
        }
    }
}
