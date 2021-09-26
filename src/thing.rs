use serde_json;
use serde_json::json;
use std::any::Any;
use std::collections::HashMap;
use std::marker::{Send, Sync};
use std::sync::{Arc, RwLock};
use std::vec::Drain;
use valico::json_schema;

use super::action::Action;
use super::event::Event;
use super::property::Property;

/// High-level Thing trait.
pub trait Thing: Send + Sync {
    /// Return the thing state as a Thing Description.
    fn as_thing_description(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Return this thing as an Any.
    fn as_any(&self) -> &dyn Any;

    /// Return this thing as a mutable Any.
    fn as_mut_any(&mut self) -> &mut dyn Any;

    /// Get this thing's href.
    fn get_href(&self) -> String;

    /// Get this thing's href prefix, i.e. /0.
    fn get_href_prefix(&self) -> String;

    /// Get the UI href.
    fn get_ui_href(&self) -> Option<String>;

    /// Set the prefix of any hrefs associated with this thing.
    fn set_href_prefix(&mut self, prefix: String);

    /// Set the href of this thing's custom UI.
    fn set_ui_href(&mut self, href: String);

    /// Get the ID of the thing.
    fn get_id(&self) -> String;

    /// Get the title of the thing.
    fn get_title(&self) -> String;

    /// Get the type context of the thing.
    fn get_context(&self) -> String;

    /// Get the type(s) of the thing.
    fn get_type(&self) -> Vec<String>;

    /// Get the description of the thing.
    fn get_description(&self) -> String;

    /// Get the thing's properties as a JSON map.
    ///
    /// Returns the properties as a JSON map, i.e. name -> description.
    fn get_property_descriptions(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Get the thing's actions as an array.
    fn get_action_descriptions(&self, action_name: Option<String>) -> serde_json::Value;

    /// Get the thing's events as an array.
    fn get_event_descriptions(&self, event_name: Option<String>) -> serde_json::Value;

    /// Add a property to this thing.
    fn add_property(&mut self, property: Box<dyn Property>);

    /// Remove a property from this thing.
    fn remove_property(&mut self, property_name: String);

    /// Find a property by name.
    fn find_property(&mut self, property_name: &String) -> Option<&mut Box<dyn Property>>;

    /// Get a property's value.
    fn get_property(&self, property_name: &String) -> Option<serde_json::Value>;

    /// Get a mapping of all properties and their values.
    ///
    /// Returns an object of propertyName -> value.
    fn get_properties(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Determine whether or not this thing has a given property.
    fn has_property(&self, property_name: &String) -> bool;

    /// Set a property value.
    fn set_property(
        &mut self,
        property_name: String,
        value: serde_json::Value,
    ) -> Result<(), &'static str> {
        let property = self
            .find_property(&property_name)
            .ok_or_else(|| "Property not found")?;

        property.set_value(value.clone())?;
        self.property_notify(property_name, value);

        Ok(())
    }

    /// Get an action.
    fn get_action(
        &self,
        action_name: String,
        action_id: String,
    ) -> Option<Arc<RwLock<Box<dyn Action>>>>;

    /// Add a new event and notify subscribers.
    fn add_event(&mut self, event: Box<dyn Event>);

    /// Add an available event.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the event
    /// * `metadata` - event metadata, i.e. type, description, etc., as a JSON map
    fn add_available_event(
        &mut self,
        name: String,
        metadata: serde_json::Map<String, serde_json::Value>,
    );

    /// Perform an action on the thing.
    ///
    /// Returns the action that was created.
    fn add_action(
        &mut self,
        action: Arc<RwLock<Box<dyn Action>>>,
        input: Option<&serde_json::Value>,
    ) -> Result<(), &str>;

    /// Remove an existing action.
    ///
    /// Returns a boolean indicating the presence of the action.
    fn remove_action(&mut self, action_name: String, action_id: String) -> bool;

    /// Add an available action.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the action
    /// * `metadata` - action metadata, i.e. type, description, etc., as a JSON map
    fn add_available_action(
        &mut self,
        name: String,
        metadata: serde_json::Map<String, serde_json::Value>,
    );

    /// Add a new websocket subscriber.
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn add_subscriber(&mut self, ws_id: String);

    /// Remove a websocket subscriber.
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn remove_subscriber(&mut self, ws_id: String);

    /// Add a new websocket subscriber to an event.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the event
    /// * `ws_id` - ID of the websocket
    fn add_event_subscriber(&mut self, name: String, ws_id: String);

    /// Remove a websocket subscriber from an event.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the event
    /// * `ws_id` - ID of the websocket
    fn remove_event_subscriber(&mut self, name: String, ws_id: String);

    /// Notify all subscribers of a property change.
    fn property_notify(&mut self, name: String, value: serde_json::Value);

    /// Notify all subscribers of an action status change.
    fn action_notify(&mut self, action: serde_json::Map<String, serde_json::Value>);

    /// Notify all subscribers of an event.
    fn event_notify(&mut self, name: String, event: serde_json::Map<String, serde_json::Value>);

    /// Start the specified action.
    fn start_action(&mut self, name: String, id: String);

    /// Cancel the specified action.
    fn cancel_action(&mut self, name: String, id: String);

    /// Finish the specified action.
    fn finish_action(&mut self, name: String, id: String);

    /// Drain any message queues for the specified weboscket ID.
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn drain_queue(&mut self, ws_id: String) -> Vec<Drain<String>>;
}

/// Basic web thing implementation.
///
/// This can easily be used by other things to handle most of the boring work.
#[derive(Default)]
pub struct BaseThing {
    id: String,
    context: String,
    type_: Vec<String>,
    title: String,
    description: String,
    properties: HashMap<String, Box<dyn Property>>,
    available_actions: HashMap<String, AvailableAction>,
    available_events: HashMap<String, AvailableEvent>,
    actions: HashMap<String, Vec<Arc<RwLock<Box<dyn Action>>>>>,
    events: Vec<Box<dyn Event>>,
    subscribers: HashMap<String, Vec<String>>,
    href_prefix: String,
    ui_href: Option<String>,
}

impl BaseThing {
    /// Create a new BaseThing.
    ///
    /// # Arguments
    ///
    /// * `id` - the thing's unique ID - must be a URI
    /// * `title` - the thing's title
    /// * `type_` - the thing's type(s)
    /// * `description` - description of the thing
    pub fn new(
        id: String,
        title: String,
        type_: Option<Vec<String>>,
        description: Option<String>,
    ) -> Self {
        Self {
            id,
            context: "https://webthings.io/schemas".to_owned(),
            type_: type_.unwrap_or_else(|| vec![]),
            title,
            description: description.unwrap_or_else(|| "".to_string()),
            ..Default::default()
        }
    }
}

impl Thing for BaseThing {
    /// Return the thing state as a Thing Description.
    fn as_thing_description(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut description = serde_json::Map::new();

        description.insert("id".to_owned(), json!(self.get_id()));
        description.insert("title".to_owned(), json!(self.get_title()));
        description.insert("@context".to_owned(), json!(self.get_context()));
        description.insert("@type".to_owned(), json!(self.get_type()));
        description.insert(
            "properties".to_owned(),
            json!(self.get_property_descriptions()),
        );

        let mut links: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

        let mut properties_link = serde_json::Map::new();
        properties_link.insert("rel".to_owned(), json!("properties"));
        properties_link.insert(
            "href".to_owned(),
            json!(format!("{}/properties", self.get_href_prefix())),
        );
        links.push(properties_link);

        let mut actions_link = serde_json::Map::new();
        actions_link.insert("rel".to_owned(), json!("actions"));
        actions_link.insert(
            "href".to_owned(),
            json!(format!("{}/actions", self.get_href_prefix())),
        );
        links.push(actions_link);

        let mut events_link = serde_json::Map::new();
        events_link.insert("rel".to_owned(), json!("events"));
        events_link.insert(
            "href".to_owned(),
            json!(format!("{}/events", self.get_href_prefix())),
        );
        links.push(events_link);

        if let Some(ui_href) = self.get_ui_href() {
            let mut ui_link = serde_json::Map::new();
            ui_link.insert("rel".to_owned(), json!("alternate"));
            ui_link.insert("mediaType".to_owned(), json!("text/html"));
            ui_link.insert("href".to_owned(), json!(ui_href));
            links.push(ui_link);
        }

        description.insert("links".to_owned(), json!(links));

        let mut actions = serde_json::Map::new();
        for (name, action) in self.available_actions.iter() {
            let mut metadata = action.get_metadata().clone();
            metadata.insert(
                "links".to_string(),
                json!([
                    {
                        "rel": "action",
                        "href": format!("{}/actions/{}", self.get_href_prefix(), name),
                    },
                ]),
            );
            actions.insert(name.to_string(), json!(metadata));
        }

        description.insert("actions".to_owned(), json!(actions));

        let mut events = serde_json::Map::new();
        for (name, event) in self.available_events.iter() {
            let mut metadata = event.get_metadata().clone();
            metadata.insert(
                "links".to_string(),
                json!([
                    {
                        "rel": "event",
                        "href": format!("{}/events/{}", self.get_href_prefix(), name),
                    },
                ]),
            );
            events.insert(name.to_string(), json!(metadata));
        }

        description.insert("events".to_owned(), json!(events));

        if self.description.len() > 0 {
            description.insert("description".to_owned(), json!(self.description));
        }

        description
    }

    /// Return this thing as an Any.
    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Return this thing as a mutable Any.
    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }

    /// Get this thing's href.
    fn get_href(&self) -> String {
        if self.href_prefix == "" {
            "/".to_owned()
        } else {
            self.href_prefix.clone()
        }
    }

    /// Get this thing's href prefix, i.e. /0.
    fn get_href_prefix(&self) -> String {
        self.href_prefix.clone()
    }

    /// Get the UI href.
    fn get_ui_href(&self) -> Option<String> {
        self.ui_href.clone()
    }

    /// Set the prefix of any hrefs associated with this thing.
    fn set_href_prefix(&mut self, prefix: String) {
        self.href_prefix = prefix.clone();

        for property in self.properties.values_mut() {
            property.set_href_prefix(prefix.clone());
        }

        for actions in self.actions.values_mut() {
            for action in actions {
                action.write().unwrap().set_href_prefix(prefix.clone());
            }
        }
    }

    /// Set the href of this thing's custom UI.
    fn set_ui_href(&mut self, href: String) {
        self.ui_href = Some(href);
    }

    /// Get the ID of the thing.
    fn get_id(&self) -> String {
        self.id.clone()
    }

    /// Get the title of the thing.
    fn get_title(&self) -> String {
        self.title.clone()
    }

    /// Get the type context of the thing.
    fn get_context(&self) -> String {
        self.context.clone()
    }

    /// Get the type(s) of the thing.
    fn get_type(&self) -> Vec<String> {
        self.type_.clone()
    }

    /// Get the description of the thing.
    fn get_description(&self) -> String {
        self.description.clone()
    }

    /// Get the thing's properties as a JSON map.
    ///
    /// Returns the properties as a JSON map, i.e. name -> description.
    fn get_property_descriptions(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut descriptions = serde_json::Map::new();

        for (name, property) in self.properties.iter() {
            descriptions.insert(name.to_string(), json!(property.as_property_description()));
        }

        descriptions
    }

    /// Get the thing's actions as an array.
    fn get_action_descriptions(&self, action_name: Option<String>) -> serde_json::Value {
        let mut descriptions = Vec::new();

        match action_name {
            Some(action_name) => {
                if let Some(actions) = self.actions.get(&action_name) {
                    for action in actions {
                        descriptions.push(action.read().unwrap().as_action_description());
                    }
                }
            }
            None => {
                for action in self.actions.values().flatten() {
                    descriptions.push(action.read().unwrap().as_action_description());
                }
            }
        }

        json!(descriptions)
    }

    /// Get the thing's events as an array.
    fn get_event_descriptions(&self, event_name: Option<String>) -> serde_json::Value {
        let mut descriptions = Vec::new();

        match event_name {
            Some(event_name) => {
                for event in &self.events {
                    if event.get_name() == event_name {
                        descriptions.push(event.as_event_description());
                    }
                }
            }
            None => {
                for event in &self.events {
                    descriptions.push(event.as_event_description());
                }
            }
        }

        json!(descriptions)
    }

    /// Add a property to this thing.
    fn add_property(&mut self, mut property: Box<dyn Property>) {
        property.set_href_prefix(self.get_href_prefix());
        self.properties.insert(property.get_name(), property);
    }

    /// Remove a property from this thing.
    fn remove_property(&mut self, property_name: String) {
        self.properties.remove(&property_name);
    }

    /// Find a property by name.
    fn find_property(&mut self, property_name: &String) -> Option<&mut Box<dyn Property>> {
        self.properties.get_mut(property_name)
    }

    /// Get a property's value.
    fn get_property(&self, property_name: &String) -> Option<serde_json::Value> {
        self.properties.get(property_name).map(|p| p.get_value())
    }

    /// Get a mapping of all properties and their values.
    ///
    /// Returns an object of propertyName -> value.
    fn get_properties(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut properties = serde_json::Map::new();
        for (name, property) in self.properties.iter() {
            properties.insert(name.to_string(), json!(property.get_value()));
        }
        properties
    }

    /// Determine whether or not this thing has a given property.
    fn has_property(&self, property_name: &String) -> bool {
        self.properties.contains_key(property_name)
    }

    /// Get an action.
    fn get_action(
        &self,
        action_name: String,
        action_id: String,
    ) -> Option<Arc<RwLock<Box<dyn Action>>>> {
        match self.actions.get(&action_name) {
            Some(entry) => {
                for action in entry {
                    if action.read().unwrap().get_id() == action_id {
                        return Some(action.clone());
                    }
                }

                None
            }
            None => None,
        }
    }

    /// Add a new event and notify subscribers.
    fn add_event(&mut self, event: Box<dyn Event>) {
        self.event_notify(event.get_name(), event.as_event_description());
        self.events.push(event);
    }

    /// Add an available event.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the event
    /// * `metadata` - event metadata, i.e. type, description, etc., as a JSON map
    fn add_available_event(
        &mut self,
        name: String,
        metadata: serde_json::Map<String, serde_json::Value>,
    ) {
        let event = AvailableEvent::new(metadata);
        self.available_events.insert(name, event);
    }

    /// Perform an action on the thing.
    ///
    /// Returns the action that was created.
    fn add_action(
        &mut self,
        action: Arc<RwLock<Box<dyn Action>>>,
        input: Option<&serde_json::Value>,
    ) -> Result<(), &str> {
        let action_name = action.read().unwrap().get_name();

        if let Some(action_type) = self.available_actions.get(&action_name) {
            if !action_type.validate_action_input(input) {
                return Err("Action input invalid");
            }
        } else {
            return Err("Action type not found");
        }

        action
            .write()
            .unwrap()
            .set_href_prefix(self.get_href_prefix());
        self.action_notify(action.read().unwrap().as_action_description());
        self.actions.get_mut(&action_name).unwrap().push(action);

        Ok(())
    }

    /// Remove an existing action.
    ///
    /// Returns a boolean indicating the presence of the action.
    fn remove_action(&mut self, action_name: String, action_id: String) -> bool {
        let action = self.get_action(action_name.clone(), action_id.clone());
        match action {
            Some(action) => {
                action.write().unwrap().cancel();

                let actions = self.actions.get_mut(&action_name).unwrap();
                actions.retain(|ref a| a.read().unwrap().get_id() != action_id);

                true
            }
            None => false,
        }
    }

    /// Add an available action.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the action
    /// * `metadata` - action metadata, i.e. type, description, etc., as a JSON map
    fn add_available_action(
        &mut self,
        name: String,
        metadata: serde_json::Map<String, serde_json::Value>,
    ) {
        let action = AvailableAction::new(metadata);
        self.available_actions.insert(name.clone(), action);
        self.actions.insert(name, Vec::new());
    }

    /// Add a new websocket subscriber.
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn add_subscriber(&mut self, ws_id: String) {
        self.subscribers.insert(ws_id, Vec::new());
    }

    /// Remove a websocket subscriber.
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn remove_subscriber(&mut self, ws_id: String) {
        self.subscribers.remove(&ws_id);

        for event in self.available_events.values_mut() {
            event.remove_subscriber(ws_id.clone());
        }
    }

    /// Add a new websocket subscriber to an event.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the event
    /// * `ws_id` - ID of the websocket
    fn add_event_subscriber(&mut self, name: String, ws_id: String) {
        if let Some(event) = self.available_events.get_mut(&name) {
            event.add_subscriber(ws_id);
        }
    }

    /// Remove a websocket subscriber from an event.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the event
    /// * `ws_id` - ID of the websocket
    fn remove_event_subscriber(&mut self, name: String, ws_id: String) {
        if let Some(event) = self.available_events.get_mut(&name) {
            event.remove_subscriber(ws_id);
        }
    }

    /// Notify all subscribers of a property change.
    fn property_notify(&mut self, name: String, value: serde_json::Value) {
        let message = json!({
            "messageType": "propertyStatus",
            "data": {
                name: value
            }
        })
        .to_string();

        self.subscribers
            .values_mut()
            .for_each(|queue| queue.push(message.clone()));
    }

    /// Notify all subscribers of an action status change.
    fn action_notify(&mut self, action: serde_json::Map<String, serde_json::Value>) {
        let message = json!({
            "messageType": "actionStatus",
            "data": action
        })
        .to_string();

        self.subscribers
            .values_mut()
            .for_each(|queue| queue.push(message.clone()));
    }

    /// Notify all subscribers of an event.
    fn event_notify(&mut self, name: String, event: serde_json::Map<String, serde_json::Value>) {
        if !self.available_events.contains_key(&name) {
            return;
        }

        let message = json!({
            "messageType": "event",
            "data": event,
        })
        .to_string();

        self.available_events
            .get_mut(&name)
            .unwrap()
            .get_subscribers()
            .values_mut()
            .for_each(|queue| queue.push(message.clone()));
    }

    /// Start the specified action.
    fn start_action(&mut self, name: String, id: String) {
        match self.get_action(name, id) {
            Some(action) => {
                let mut a = action.write().unwrap();
                a.start();
                self.action_notify(a.as_action_description());
                a.perform_action();
            }
            None => (),
        }
    }

    /// Cancel the specified action.
    fn cancel_action(&mut self, name: String, id: String) {
        match self.get_action(name, id) {
            Some(action) => {
                let mut a = action.write().unwrap();
                a.cancel();
            }
            None => (),
        }
    }

    /// Finish the specified action.
    fn finish_action(&mut self, name: String, id: String) {
        match self.get_action(name, id) {
            Some(action) => {
                let mut a = action.write().unwrap();
                a.finish();
                self.action_notify(a.as_action_description());
            }
            None => (),
        }
    }

    /// Drain any message queues for the specified weboscket ID.
    ///
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn drain_queue(&mut self, ws_id: String) -> Vec<Drain<String>> {
        let mut drains: Vec<Drain<String>> = Vec::new();
        match self.subscribers.get_mut(&ws_id) {
            Some(v) => {
                drains.push(v.drain(..));
            }
            None => (),
        }

        self.available_events.values_mut().for_each(|evt| {
            match evt.get_subscribers().get_mut(&ws_id) {
                Some(v) => {
                    drains.push(v.drain(..));
                }
                None => (),
            }
        });

        drains
    }
}

/// Struct to describe an action available to be taken.
struct AvailableAction {
    metadata: serde_json::Map<String, serde_json::Value>,
}

impl AvailableAction {
    /// Create a new AvailableAction.
    ///
    /// # Arguments
    ///
    /// * `metadata` - action metadata
    fn new(metadata: serde_json::Map<String, serde_json::Value>) -> AvailableAction {
        AvailableAction { metadata: metadata }
    }

    /// Get the action metadata.
    fn get_metadata(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.metadata
    }

    /// Validate the input for a new action.
    ///
    /// Returns a boolean indicating validation success.
    fn validate_action_input(&self, input: Option<&serde_json::Value>) -> bool {
        let mut scope = json_schema::Scope::new();
        let validator = if let Some(input) = self.metadata.get("input") {
            let mut schema = input.as_object().unwrap().clone();
            if let Some(properties) = schema.get_mut("properties") {
                let properties = properties.as_object_mut().unwrap();
                for value in properties.values_mut() {
                    let value = value.as_object_mut().unwrap();
                    value.remove("@type");
                    value.remove("unit");
                    value.remove("title");
                }
            }

            match scope.compile_and_return(json!(schema), true) {
                Ok(s) => Some(s),
                Err(_) => None,
            }
        } else {
            None
        };

        match validator {
            Some(ref v) => match input {
                Some(i) => v.validate(&i).is_valid(),
                None => v.validate(&serde_json::Value::Null).is_valid(),
            },
            None => true,
        }
    }
}

/// Struct to describe an event available for subscription.
struct AvailableEvent {
    metadata: serde_json::Map<String, serde_json::Value>,
    subscribers: HashMap<String, Vec<String>>,
}

impl AvailableEvent {
    /// Create a new AvailableEvent.
    ///
    /// # Arguments
    ///
    /// * `metadata` - event metadata
    fn new(metadata: serde_json::Map<String, serde_json::Value>) -> AvailableEvent {
        AvailableEvent {
            metadata: metadata,
            subscribers: HashMap::new(),
        }
    }

    /// Get the event metadata.
    fn get_metadata(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.metadata
    }

    /// Add a websocket subscriber to the event.
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn add_subscriber(&mut self, ws_id: String) {
        self.subscribers.insert(ws_id, Vec::new());
    }

    /// Remove a websocket subscriber from the event.
    ///
    /// # Arguments
    ///
    /// * `ws_id` - ID of the websocket
    fn remove_subscriber(&mut self, ws_id: String) {
        self.subscribers.remove(&ws_id);
    }

    /// Get the set of subscribers for the event.
    fn get_subscribers(&mut self) -> &mut HashMap<String, Vec<String>> {
        &mut self.subscribers
    }
}
