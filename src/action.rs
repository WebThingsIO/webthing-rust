/// High-level Action base class implementation.

use serde_json;
use std::marker::{Send, Sync};
use std::sync::Arc;

use super::utils::timestamp;

pub trait ActionObserver: Send + Sync {
    fn action_notify(&self, action: serde_json::Map<String, serde_json::Value>);
}

pub trait Observable {
    fn register(&mut self, observer: Arc<ActionObserver>);
}

pub trait Action: Send + Sync + Observable {
    /// Initialize the object.
    ///
    /// id -- ID of this action
    /// name -- name of the action
    /// input -- any action inputs
    fn new(
        id: String,
        name: String,
        input: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Self
    where
        Self: Sized;

    /// Get the action description.
    ///
    /// Returns a dictionary describing the action.
    fn as_action_description(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut description = serde_json::Map::new();
        let mut inner = serde_json::Map::new();
        inner.insert("href".to_owned(), json!(self.get_href()));
        inner.insert("timeRequested".to_owned(), json!(self.get_time_requested()));
        inner.insert("status".to_owned(), json!(self.get_status()));

        let input = self.get_input();
        if input.is_some() {
            inner.insert("input".to_owned(), json!(input.unwrap()));
        }

        let time_completed = self.get_time_completed();
        if time_completed.is_some() {
            inner.insert("timeCompleted".to_owned(), json!(time_completed.unwrap()));
        }

        description.insert(self.get_name(), json!(inner));
        description
    }

    /// Set the prefix of any hrefs associated with this action.
    ///
    /// prefix -- the prefix
    fn set_href_prefix(&mut self, prefix: String);

    /// Get this action's ID.
    fn get_id(&self) -> String;

    /// Get this action's name.
    fn get_name(&self) -> String;

    /// Get this action's href.
    fn get_href(&self) -> String;

    /// Get this action's status.
    fn get_status(&self) -> String;

    /// Get the time the action was requested.
    fn get_time_requested(&self) -> String;

    /// Get the time the action was completed.
    fn get_time_completed(&self) -> Option<String>;

    /// Get the inputs for this action.
    fn get_input(&self) -> Option<serde_json::Map<String, serde_json::Value>>;

    /// Start performing the action.
    fn start(&mut self);

    /// Override this with the code necessary to perform the action.
    fn perform_action(&self);

    /// Override this with the code necessary to cancel the action.
    fn cancel(&self);

    /// Finish performing the action.
    fn finish(&mut self);

    /// Notify all observers of a change.
    fn notify_all(&self);
}

pub struct BaseAction {
    id: String,
    name: String,
    input: Option<serde_json::Map<String, serde_json::Value>>,
    href_prefix: String,
    href: String,
    status: String,
    time_requested: String,
    time_completed: Option<String>,
    observers: Vec<Arc<ActionObserver>>,
}

/// An Action represents an individual action on a thing.
impl Action for BaseAction {
    /// Initialize the object.
    ///
    /// id -- ID of this action
    /// name -- name of the action
    /// input -- any action inputs
    fn new(
        id: String,
        name: String,
        input: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> BaseAction {
        let href = format!("/actions/{}/{}", name, id);

        BaseAction {
            id: id,
            name: name,
            input: input,
            href_prefix: "".to_owned(),
            href: href,
            status: "created".to_owned(),
            time_requested: timestamp(),
            time_completed: None,
            observers: Vec::new(),
        }
    }

    /// Set the prefix of any hrefs associated with this action.
    ///
    /// prefix -- the prefix
    fn set_href_prefix(&mut self, prefix: String) {
        self.href_prefix = prefix;
    }

    /// Get this action's ID.
    fn get_id(&self) -> String {
        self.id.clone()
    }

    /// Get this action's name.
    fn get_name(&self) -> String {
        self.name.clone()
    }

    /// Get this action's href.
    fn get_href(&self) -> String {
        format!("{}{}", self.href_prefix, self.href)
    }

    /// Get this action's status.
    fn get_status(&self) -> String {
        self.status.clone()
    }

    /// Get the time the action was requested.
    fn get_time_requested(&self) -> String {
        self.time_requested.clone()
    }

    /// Get the time the action was completed.
    fn get_time_completed(&self) -> Option<String> {
        self.time_completed.clone()
    }

    /// Get the inputs for this action.
    fn get_input(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        self.input.clone()
    }

    /// Start performing the action.
    fn start(&mut self) {
        self.status = "pending".to_owned();
        self.notify_all();
        self.perform_action();
        self.finish();
    }

    /// Override this with the code necessary to perform the action.
    fn perform_action(&self) {}

    /// Override this with the code necessary to cancel the action.
    fn cancel(&self) {}

    /// Finish performing the action.
    fn finish(&mut self) {
        self.status = "completed".to_owned();
        self.time_completed = Some(timestamp());
        self.notify_all();
    }

    /// Notify all observers of a change.
    fn notify_all(&self) {
        for obs in &self.observers {
            obs.action_notify(self.as_action_description());
        }
    }
}

impl Observable for BaseAction {
    fn register(&mut self, observer: Arc<ActionObserver>) {
        self.observers.push(observer);
    }
}
