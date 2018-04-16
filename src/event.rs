/// High-level Event base class implementation.

use serde_json::Value;

use thing::Thing;


/// An Event represents an individual event from a thing.
pub struct Event {
    thing: Thing,
    name: String,
    data: Option<Value>,
    time: String,
}

impl Event {
    /// Get the event description.
    ///
    /// Returns a JSON value describing the event.
    pub fn as_event_description(&self) -> Value {
        match self.data {
            Some(ref v) => {
                json!({
                    "name": json!({
                        "timestamp": self.time,
                        "data": v
                    })
                })
            }
            None => {
                json!({
                    "name": json!({
                        "timestamp": self.time,
                    })
                })
            }
        }
    }

    /// Get the thing associated with this event.
    pub fn get_thing(&self) -> &Thing {
        &self.thing
    }

    /// Get the event's name.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Get the event's data.
    pub fn get_data(&self) -> &Option<Value> {
        &self.data
    }

    /// Get the event's timestamp.
    pub fn get_time(&self) -> &str {
        &self.time
    }
}
