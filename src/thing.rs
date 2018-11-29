use serde_json;
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
    ///
    /// Returns the state as a JSON map.
    fn as_thing_description(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Return this thing as an Any.
    fn as_any(&self) -> &Any;

    /// Return this thing as a mutable Any.
    fn as_mut_any(&mut self) -> &mut Any;

    /// Get this thing's href.
    fn get_href(&self) -> String;

    /// Get this thing's href prefix, i.e. /0.
    fn get_href_prefix(&self) -> String;

    /// Get the UI href.
    fn get_ui_href(&self) -> Option<String>;

    /// Set the prefix of any hrefs associated with this thing.
    ///
    /// prefix -- the prefix
    fn set_href_prefix(&mut self, prefix: String);

    /// Set the href of this thing's custom UI.
    ///
    /// href -- the href
    fn set_ui_href(&mut self, href: String);

    /// Get the name of the thing.
    ///
    /// Returns the name as a string.
    fn get_name(&self) -> String;

    /// Get the type context of the thing.
    ///
    /// Returns the context as a string.
    fn get_context(&self) -> String;

    /// Get the type(s) of the thing.
    ///
    /// Returns the list of types.
    fn get_type(&self) -> Vec<String>;

    /// Get the description of the thing.
    ///
    /// Returns the description as a string.
    fn get_description(&self) -> String;

    /// Get the thing's properties as a JSON map.
    ///
    /// Returns the properties as a JSON map, i.e. name -> description.
    fn get_property_descriptions(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Get the thing's actions as an array.
    ///
    /// action_name -- Optional action name to get descriptions for
    ///
    /// Returns the action descriptions.
    fn get_action_descriptions(&self, action_name: Option<String>) -> serde_json::Value;

    /// Get the thing's events as an array.
    ///
    /// event_name -- Optional event name to get descriptions for
    ///
    /// Returns the event descriptions.
    fn get_event_descriptions(&self, event_name: Option<String>) -> serde_json::Value;

    /// Add a property to this thing.
    ///
    /// property -- property to add
    fn add_property(&mut self, property: Box<Property>);

    /// Remove a property from this thing.
    ///
    /// property -- property to remove
    fn remove_property(&mut self, property_name: String);

    /// Find a property by name.
    ///
    /// property_name -- the property to find
    ///
    /// Returns a boxed property trait object, if found, else None.
    fn find_property(&mut self, property_name: String) -> Option<&mut Box<Property>>;

    /// Get a property's value.
    ///
    /// property_name -- the property to get the value of
    ///
    /// Returns the properties value, if found, else None.
    fn get_property(&self, property_name: String) -> Option<serde_json::Value>;

    /// Get a mapping of all properties and their values.
    ///
    /// Returns an object of propertyName -> value.
    fn get_properties(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Determine whether or not this thing has a given property.
    ///
    /// property_name -- the property to look for
    ///
    /// Returns a boolean, indicating whether or not the thing has the property.
    fn has_property(&self, property_name: String) -> bool;

    /// Set a property value.
    ///
    /// property_name -- name of the property to set
    /// value -- value to set
    fn set_property(
        &mut self,
        property_name: String,
        value: serde_json::Value,
    ) -> Result<(), &'static str> {
        {
            let prop = self.find_property(property_name.clone());
            if prop.is_none() {
                return Err("Property not found");
            }

            let prop = prop.unwrap();
            match prop.set_value(value.clone()) {
                Ok(_) => (),
                Err(e) => {
                    return Err(e);
                }
            }
        }

        self.property_notify(property_name, value);
        Ok(())
    }

    /// Get an action.
    ///
    /// action_name -- name of the action
    /// action_id -- ID of the action
    ///
    /// Returns the requested action if found, else None.
    fn get_action(
        &self,
        action_name: String,
        action_id: String,
    ) -> Option<Arc<RwLock<Box<Action>>>>;

    /// Add a new event and notify subscribers.
    ///
    /// event -- the event that occurred
    fn add_event(&mut self, event: Box<Event>);

    /// Add an available event.
    ///
    /// name -- name of the event
    /// metadata -- event metadata, i.e. type, description, etc., as a JSON map
    fn add_available_event(
        &mut self,
        name: String,
        metadata: serde_json::Map<String, serde_json::Value>,
    );

    /// Perform an action on the thing.
    ///
    /// action_name -- name of the action
    /// input_ -- any action inputs
    ///
    /// Returns the action that was created.
    fn add_action(
        &mut self,
        action: Arc<RwLock<Box<Action>>>,
        input: Option<&serde_json::Value>,
    ) -> Result<(), &str>;

    /// Remove an existing action.
    ///
    /// action_name -- name of the action
    /// action_id -- ID of the action
    ///
    /// Returns a boolean indicating the presence of the action.
    fn remove_action(&mut self, action_name: String, action_id: String) -> bool;

    /// Add an available action.
    ///
    /// name -- name of the action
    /// metadata -- action metadata, i.e. type, description, etc., as a JSON map
    fn add_available_action(
        &mut self,
        name: String,
        metadata: serde_json::Map<String, serde_json::Value>,
    );

    /// Add a new websocket subscriber.
    ///
    /// ws_id -- ID of the websocket
    fn add_subscriber(&mut self, ws_id: String);

    /// Remove a websocket subscriber.
    ///
    /// ws_id -- ID of the websocket
    fn remove_subscriber(&mut self, ws_id: String);

    /// Add a new websocket subscriber to an event.
    ///
    /// name -- name of the event
    /// ws_id -- ID of the websocket
    fn add_event_subscriber(&mut self, name: String, ws_id: String);

    /// Remove a websocket subscriber from an event.
    ///
    /// name -- name of the event
    /// ws_id -- ID of the websocket
    fn remove_event_subscriber(&mut self, name: String, ws_id: String);

    /// Notify all subscribers of a property change.
    ///
    /// name -- name of the property that changed
    /// value -- new property value
    fn property_notify(&mut self, name: String, value: serde_json::Value);

    /// Notify all subscribers of an action status change.
    ///
    /// action -- JSON description the action whose status changed
    fn action_notify(&mut self, action: serde_json::Map<String, serde_json::Value>);

    /// Notify all subscribers of an event.
    ///
    /// name -- name of the event that occurred
    /// event -- JSON description of the event
    fn event_notify(&mut self, name: String, event: serde_json::Map<String, serde_json::Value>);

    /// Start the specified action.
    ///
    /// name -- name of the action
    /// id -- ID of the action
    fn start_action(&mut self, name: String, id: String);

    /// Cancel the specified action.
    ///
    /// name -- name of the action
    /// id -- ID of the action
    fn cancel_action(&mut self, name: String, id: String);

    /// Finish the specified action.
    ///
    /// name -- name of the action
    /// id -- ID of the action
    fn finish_action(&mut self, name: String, id: String);

    /// Drain any message queues for the specified weboscket ID.
    ///
    /// ws_id -- ID of the websocket
    fn drain_queue(&mut self, ws_id: String) -> Vec<Drain<String>>;
}

/// Basic web thing implementation.
///
/// This can easily be used by other things to handle most of the boring work.
pub struct BaseThing {
    context: String,
    type_: Vec<String>,
    name: String,
    description: String,
    properties: HashMap<String, Box<Property>>,
    available_actions: HashMap<String, AvailableAction>,
    available_events: HashMap<String, AvailableEvent>,
    actions: HashMap<String, Vec<Arc<RwLock<Box<Action>>>>>,
    events: Vec<Box<Event>>,
    subscribers: HashMap<String, Vec<String>>,
    href_prefix: String,
    ui_href: Option<String>,
}

impl BaseThing {
    /// Create a new BaseThing.
    ///
    /// name -- the thing's name
    /// type -- the thing's type(s)
    /// description -- description of the thing
    pub fn new(name: String, type_: Option<Vec<String>>, description: Option<String>) -> BaseThing {
        let _type = match type_ {
            Some(t) => t,
            None => vec![],
        };

        let _description = match description {
            Some(d) => d,
            None => "".to_owned(),
        };

        BaseThing {
            context: "https://iot.mozilla.org/schemas".to_owned(),
            type_: _type,
            name: name,
            description: _description,
            properties: HashMap::new(),
            available_actions: HashMap::new(),
            available_events: HashMap::new(),
            actions: HashMap::new(),
            events: Vec::new(),
            subscribers: HashMap::new(),
            href_prefix: "".to_owned(),
            ui_href: None,
        }
    }
}

impl Thing for BaseThing {
    /// Return the thing state as a Thing Description.
    ///
    /// Returns the state as a JSON map.
    fn as_thing_description(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut description = serde_json::Map::new();

        description.insert("name".to_owned(), json!(self.get_name()));
        description.insert("href".to_owned(), json!(self.get_href()));
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

        let ui_href = self.get_ui_href();
        if ui_href.is_some() {
            let mut ui_link = serde_json::Map::new();
            ui_link.insert("rel".to_owned(), json!("alternate"));
            ui_link.insert("mediaType".to_owned(), json!("text/html"));
            ui_link.insert("href".to_owned(), json!(ui_href.unwrap()));
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
    fn as_any(&self) -> &Any {
        self
    }

    /// Return this thing as a mutable Any.
    fn as_mut_any(&mut self) -> &mut Any {
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
    ///
    /// prefix -- the prefix
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
    ///
    /// href -- the href
    fn set_ui_href(&mut self, href: String) {
        self.ui_href = Some(href);
    }

    /// Get the name of the thing.
    ///
    /// Returns the name as a string.
    fn get_name(&self) -> String {
        self.name.clone()
    }

    /// Get the type context of the thing.
    ///
    /// Returns the context as a string.
    fn get_context(&self) -> String {
        self.context.clone()
    }

    /// Get the type(s) of the thing.
    ///
    /// Returns the list of types.
    fn get_type(&self) -> Vec<String> {
        self.type_.clone()
    }

    /// Get the description of the thing.
    ///
    /// Returns the description as a string.
    fn get_description(&self) -> String {
        self.description.clone()
    }

    /// Get the thing's properties as a JSON map.
    ///
    /// Returns the properties as a JSON map, i.e. name -> description.
    fn get_property_descriptions(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut descriptions: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        for (name, property) in self.properties.iter() {
            descriptions.insert(name.to_string(), json!(property.as_property_description()));
        }

        descriptions
    }

    /// Get the thing's actions as an array.
    ///
    /// action_name -- Optional action name to get descriptions for
    ///
    /// Returns the action descriptions.
    fn get_action_descriptions(&self, action_name: Option<String>) -> serde_json::Value {
        let mut descriptions: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

        match action_name {
            Some(action_name) => {
                let actions = self.actions.get(&action_name);
                if actions.is_some() {
                    let actions = actions.unwrap();
                    for action in actions {
                        descriptions.push(action.read().unwrap().as_action_description());
                    }
                }
            }
            None => {
                for actions in self.actions.values() {
                    for action in actions {
                        descriptions.push(action.read().unwrap().as_action_description());
                    }
                }
            }
        }

        json!(descriptions)
    }

    /// Get the thing's events as an array.
    ///
    /// event_name -- Optional event name to get descriptions for
    ///
    /// Returns the event descriptions.
    fn get_event_descriptions(&self, event_name: Option<String>) -> serde_json::Value {
        let mut descriptions: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

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
    ///
    /// property -- property to add
    fn add_property(&mut self, mut property: Box<Property>) {
        property.set_href_prefix(self.get_href_prefix());
        self.properties.insert(property.get_name(), property);
    }

    /// Remove a property from this thing.
    ///
    /// property -- property to remove
    fn remove_property(&mut self, property_name: String) {
        self.properties.remove(&property_name);
    }

    /// Find a property by name.
    ///
    /// property_name -- the property to find
    ///
    /// Returns a boxed property trait object, if found, else None.
    fn find_property(&mut self, property_name: String) -> Option<&mut Box<Property>> {
        self.properties.get_mut(&property_name)
    }

    /// Get a property's value.
    ///
    /// property_name -- the property to get the value of
    ///
    /// Returns the properties value, if found, else None.
    fn get_property(&self, property_name: String) -> Option<serde_json::Value> {
        if self.has_property(property_name.clone()) {
            Some(self.properties.get(&property_name).unwrap().get_value())
        } else {
            None
        }
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
    ///
    /// property_name -- the property to look for
    ///
    /// Returns a boolean, indicating whether or not the thing has the property.
    fn has_property(&self, property_name: String) -> bool {
        self.properties.contains_key(&property_name)
    }

    /// Get an action.
    ///
    /// action_name -- name of the action
    /// action_id -- ID of the action
    ///
    /// Returns the requested action if found, else None.
    fn get_action(
        &self,
        action_name: String,
        action_id: String,
    ) -> Option<Arc<RwLock<Box<Action>>>> {
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
    ///
    /// event -- the event that occurred
    fn add_event(&mut self, event: Box<Event>) {
        self.event_notify(event.get_name(), event.as_event_description());
        self.events.push(event);
    }

    /// Add an available event.
    ///
    /// name -- name of the event
    /// metadata -- event metadata, i.e. type, description, etc., as a JSON map
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
    /// action_name -- name of the action
    /// input_ -- any action inputs
    ///
    /// Returns the action that was created.
    fn add_action(
        &mut self,
        action: Arc<RwLock<Box<Action>>>,
        input: Option<&serde_json::Value>,
    ) -> Result<(), &str> {
        let action_name = action.read().unwrap().get_name();

        {
            if !self.available_actions.contains_key(&action_name) {
                return Err("Action type not found");
            }

            let action_type = self.available_actions.get(&action_name).unwrap();
            if !action_type.validate_action_input(input) {
                return Err("Action input invalid");
            }
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
    /// action_name -- name of the action
    /// action_id -- ID of the action
    ///
    /// Returns a boolean indicating the presence of the action.
    fn remove_action(&mut self, action_name: String, action_id: String) -> bool {
        let action = self.get_action(action_name.clone(), action_id.clone());
        match action {
            Some(action) => {
                action.write().unwrap().cancel();

                let mut actions = self.actions.get_mut(&action_name).unwrap();
                actions.retain(|ref a| a.read().unwrap().get_id() != action_id);

                true
            }
            None => false,
        }
    }

    /// Add an available action.
    ///
    /// name -- name of the action
    /// metadata -- action metadata, i.e. type, description, etc., as a JSON map
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
    /// ws_id -- ID of the websocket
    fn add_subscriber(&mut self, ws_id: String) {
        self.subscribers.insert(ws_id, Vec::new());
    }

    /// Remove a websocket subscriber.
    ///
    /// ws_id -- ID of the websocket
    fn remove_subscriber(&mut self, ws_id: String) {
        self.subscribers.remove(&ws_id);

        for event in self.available_events.values_mut() {
            event.remove_subscriber(ws_id.clone());
        }
    }

    /// Add a new websocket subscriber to an event.
    ///
    /// name -- name of the event
    /// ws_id -- ID of the websocket
    fn add_event_subscriber(&mut self, name: String, ws_id: String) {
        if self.available_events.contains_key(&name) {
            self.available_events
                .get_mut(&name)
                .unwrap()
                .add_subscriber(ws_id);
        }
    }

    /// Remove a websocket subscriber from an event.
    ///
    /// name -- name of the event
    /// ws_id -- ID of the websocket
    fn remove_event_subscriber(&mut self, name: String, ws_id: String) {
        if self.available_events.contains_key(&name) {
            self.available_events
                .get_mut(&name)
                .unwrap()
                .remove_subscriber(ws_id);
        }
    }

    /// Notify all subscribers of a property change.
    ///
    /// name -- name of the property that changed
    /// value -- new property value
    fn property_notify(&mut self, name: String, value: serde_json::Value) {
        let message = json!({
            "messageType": "propertyStatus",
            "data": {
                name: value
            }
        }).to_string();

        self.subscribers
            .values_mut()
            .for_each(|queue| queue.push(message.clone()));
    }

    /// Notify all subscribers of an action status change.
    ///
    /// action -- JSON description the action whose status changed
    fn action_notify(&mut self, action: serde_json::Map<String, serde_json::Value>) {
        let message = json!({
            "messageType": "actionStatus",
            "data": action
        }).to_string();

        self.subscribers
            .values_mut()
            .for_each(|queue| queue.push(message.clone()));
    }

    /// Notify all subscribers of an event.
    ///
    /// name -- name of the event that occurred
    /// event -- JSON description of the event
    fn event_notify(&mut self, name: String, event: serde_json::Map<String, serde_json::Value>) {
        if !self.available_events.contains_key(&name) {
            return;
        }

        let message = json!({
            "messageType": "event",
            "data": event,
        }).to_string();

        self.available_events
            .get_mut(&name)
            .unwrap()
            .get_subscribers()
            .values_mut()
            .for_each(|queue| queue.push(message.clone()));
    }

    /// Start the specified action.
    ///
    /// name -- name of the action
    /// id -- ID of the action
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
    ///
    /// name -- name of the action
    /// id -- ID of the action
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
    ///
    /// name -- name of the action
    /// id -- ID of the action
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
    /// ws_id -- ID of the websocket
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
    /// metadata -- action metadata
    fn new(metadata: serde_json::Map<String, serde_json::Value>) -> AvailableAction {
        AvailableAction { metadata: metadata }
    }

    /// Get the action metadata.
    fn get_metadata(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.metadata
    }

    /// Validate the input for a new action.
    ///
    /// input -- the input to validate
    ///
    /// Returns a boolean indicating validation success.
    fn validate_action_input(&self, input: Option<&serde_json::Value>) -> bool {
        let mut scope = json_schema::Scope::new();
        let validator = if self.metadata.contains_key("input") {
            let schema = self.metadata.get("input").unwrap();
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
    /// metadata -- event metadata
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
    /// ws_id -- ID of the websocket
    fn add_subscriber(&mut self, ws_id: String) {
        self.subscribers.insert(ws_id, Vec::new());
    }

    /// Remove a websocket subscriber from the event.
    ///
    /// ws_id -- ID of the websocket
    fn remove_subscriber(&mut self, ws_id: String) {
        self.subscribers.remove(&ws_id);
    }

    /// Get the set of subscribers for the event.
    fn get_subscribers(&mut self) -> &mut HashMap<String, Vec<String>> {
        &mut self.subscribers
    }
}
