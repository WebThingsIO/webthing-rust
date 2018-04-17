/// High-level Event base class implementation.

use serde_json;

use thing::Thing;
use utils::timestamp;

pub trait Event {
    /// Create a new event
    fn new(thing: Thing, name: String, data: Option<serde_json::Value>) -> Self;

    /// Get the event description.
    ///
    /// Returns a JSON value describing the event.
    fn as_event_description(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut description = serde_json::Map::new();
        let mut inner = serde_json::Map::new();
        inner.insert("timestamp".to_string(), json!(self.get_time()));

        let data = self.get_data();
        if data.is_some() {
            inner.insert("data".to_string(), json!(data));
        }

        description.insert("name".to_string(), json!(inner));
        description
    }

    /// Get the thing associated with this event.
    fn get_thing(&self) -> &Thing;

    /// Get the event's name.
    fn get_name(&self) -> String;

    /// Get the event's data.
    fn get_data(&self) -> Option<serde_json::Value>;

    /// Get the event's timestamp.
    fn get_time(&self) -> String;
}

/// An Event represents an individual event from a thing.
pub struct BaseEvent {
    thing: Thing,
    name: String,
    data: Option<serde_json::Value>,
    time: String,
}

impl Event for BaseEvent {
    /// Create a new event
    fn new(thing: Thing, name: String, data: Option<serde_json::Value>) -> BaseEvent {
        BaseEvent {
            thing: thing,
            name: name,
            data: data,
            time: timestamp(),
        }
    }

    /// Get the thing associated with this event.
    fn get_thing(&self) -> &Thing {
        &self.thing
    }

    /// Get the event's name.
    fn get_name(&self) -> String {
        self.name.clone()
    }

    /// Get the event's data.
    fn get_data(&self) -> Option<serde_json::Value> {
        self.data.clone()
    }

    /// Get the event's timestamp.
    fn get_time(&self) -> String {
        self.time.clone()
    }
}
