/// High-level Thing base class implementation.
use serde_json;
use std::collections::HashMap;
use std::marker::{Send, Sync};
use std::sync::{Arc, Weak};
use valico::json_schema;

use super::action::{Action, ActionObserver};
use super::event::Event;
use super::property::{Property, PropertyObserver};
use super::server::ThingWebSocket;

pub trait Thing: Send + Sync {
    /// Initialize the object.
    ///
    /// name -- the thing's name
    /// type -- the thing's type
    /// description -- description of the thing
    fn new(name: String, type_: Option<String>, description: Option<String>) -> Self
    where
        Self: Sized;

    /// Return the thing state as a Thing Description.
    ///
    /// Returns the state as a dictionary.
    fn as_thing_description(&self) -> serde_json::Map<String, serde_json::Value>;

    fn get_href(&self) -> String;

    fn get_href_prefix(&self) -> String;

    fn get_ws_href(&self) -> Option<String>;

    fn get_ui_href(&self) -> Option<String>;

    /// Set the prefix of any hrefs associated with this thing.
    ///
    /// prefix -- the prefix
    fn set_href_prefix(&mut self, prefix: String);

    /// Set the href of this thing's websocket.
    ///
    /// href -- the href
    fn set_ws_href(&mut self, href: String);

    /// Set the href of this thing's custom UI.
    ///
    /// href -- the href
    fn set_ui_href(&mut self, href: String);

    /// Get the name of the thing.
    ///
    /// Returns the name as a string.
    fn get_name(&self) -> String;

    /// Get the type of the thing.
    ///
    /// Returns the type as a string.
    fn get_type(&self) -> String;

    /// Get the description of the thing.
    ///
    /// Returns the description as a string.
    fn get_description(&self) -> String;

    /// Get the thing's properties as a dictionary.
    ///
    /// Returns the properties as a dictionary, i.e. name -> description.
    fn get_property_descriptions(&self) -> serde_json::Map<String, serde_json::Value>;

    /// Get the thing's actions as an array.
    ///
    /// Returns the action descriptions.
    fn get_action_descriptions(&self) -> serde_json::Value;

    /// Get the thing's events as an array.
    ///
    /// Returns the event descriptions.
    fn get_event_descriptions(&self) -> serde_json::Value;

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
    /// Returns a Property object, if found, else None.
    fn find_property(&mut self, property_name: String) -> Option<&mut Box<Property>>;

    /// Get a property's value.
    ///
    /// property_name -- the property to get the value of
    ///
    /// Returns the properties value, if found, else None.
    fn get_property(&self, property_name: String) -> Option<serde_json::Value>;

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
        match self.find_property(property_name) {
            Some(p) => p.set_value(value),
            None => Err("Property not found"),
        }
    }

    /// Get an action.
    ///
    /// action_name -- name of the action
    /// action_id -- ID of the action
    ///
    /// Returns the requested action if found, else None.
    fn get_action(&self, action_name: String, action_id: String) -> Option<&Box<Action>>;

    /// Add a new event and notify subscribers.
    ///
    /// event -- the event that occurred
    fn add_event(&mut self, event: Box<Event>);

    /// Add an available event.
    ///
    /// name -- name of the event
    /// metadata -- event metadata, i.e. type, description, etc., as a dict
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
    fn perform_action(
        &self,
        action_name: String,
        input: Option<&serde_json::Value>,
    ) -> Option<Box<Action>>;

    /// Remove an existing action.
    ///
    /// action_name -- name of the action
    /// action_id -- ID of the action
    ///
    /// Returns a boolean indicating the presence of the action.
    fn remove_action(&self, action_name: String, action_id: String) -> bool;

    /// Add an available action.
    ///
    /// name -- name of the action
    /// metadata -- action metadata, i.e. type, description, etc., as a dict
    /// cls -- class to instantiate for this action
    fn add_available_action(
        &mut self,
        name: String,
        metadata: serde_json::Map<String, serde_json::Value>,
        //, cls) {
    );

    /// Add a new websocket subscriber.
    ///
    /// ws -- the websocket
    fn add_subscriber(&mut self, ws: Weak<ThingWebSocket>);

    /// Remove a websocket subscriber.
    ///
    /// ws -- the websocket
    fn remove_subscriber(&mut self, ws_id: String);

    /// Add a new websocket subscriber to an event.
    ///
    /// name -- name of the event
    /// ws -- the websocket
    fn add_event_subscriber(&mut self, name: String, ws: Weak<ThingWebSocket>);

    /// Remove a websocket subscriber from an event.
    ///
    /// name -- name of the event
    /// ws -- the websocket
    fn remove_event_subscriber(&mut self, name: String, ws_id: String);

    /// Notify all subscribers of an event.
    ///
    /// event -- the event that occurred
    fn event_notify(&self, event: &Box<Event>);
}

/// A Web Thing.
pub struct BaseThing {
    type_: String,
    name: String,
    description: String,
    properties: HashMap<String, Box<Property>>,
    available_actions: HashMap<String, AvailableAction>,
    available_events: HashMap<String, AvailableEvent>,
    actions: HashMap<String, Vec<Box<Action>>>,
    events: Vec<Box<Event>>,
    subscribers: HashMap<String, Weak<ThingWebSocket>>,
    href_prefix: String,
    ws_href: Option<String>,
    ui_href: Option<String>,
}

impl Thing for BaseThing {
    /// Initialize the object.
    ///
    /// name -- the thing's name
    /// type -- the thing's type
    /// description -- description of the thing
    fn new(name: String, type_: Option<String>, description: Option<String>) -> BaseThing {
        let _type = match type_ {
            Some(t) => t,
            None => "thing".to_owned(),
        };

        let _description = match description {
            Some(d) => d,
            None => "".to_owned(),
        };

        BaseThing {
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
            ws_href: None,
            ui_href: None,
        }
    }

    /// Return the thing state as a Thing Description.
    ///
    /// Returns the state as a dictionary.
    fn as_thing_description(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut description = serde_json::Map::new();

        description.insert("name".to_owned(), json!(self.get_name()));
        description.insert("href".to_owned(), json!(self.get_href()));
        description.insert("type".to_owned(), json!(self.get_type()));
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

        let ws_href = self.get_ws_href();
        if ws_href.is_some() {
            let mut ws_link = serde_json::Map::new();
            ws_link.insert("rel".to_owned(), json!("alternate"));
            ws_link.insert("href".to_owned(), json!(ws_href.unwrap()));
            links.push(ws_link);
        }

        let ui_href = self.get_ui_href();
        if ui_href.is_some() {
            let mut ui_link = serde_json::Map::new();
            ui_link.insert("rel".to_owned(), json!("alternate"));
            ui_link.insert("mediaType".to_owned(), json!("text/html"));
            ui_link.insert("href".to_owned(), json!(ui_href.unwrap()));
            links.push(ui_link);
        }

        let mut actions = serde_json::Map::new();
        for (name, action) in self.available_actions.iter() {
            actions.insert(name.to_string(), json!(action.get_metadata()));
        }

        description.insert("actions".to_owned(), json!(actions));

        let mut events = serde_json::Map::new();
        for (name, event) in self.available_events.iter() {
            events.insert(name.to_string(), json!(event.get_metadata()));
        }

        description.insert("events".to_owned(), json!(events));

        if self.description.len() > 0 {
            description.insert("description".to_owned(), json!(self.description));
        }

        description
    }

    fn get_href(&self) -> String {
        if self.href_prefix == "" {
            "/".to_owned()
        } else {
            self.href_prefix.clone()
        }
    }

    fn get_href_prefix(&self) -> String {
        self.href_prefix.clone()
    }

    fn get_ws_href(&self) -> Option<String> {
        self.ws_href.clone()
    }

    fn get_ui_href(&self) -> Option<String> {
        self.ui_href.clone()
    }

    /// Set the prefix of any hrefs associated with this thing.
    ///
    /// prefix -- the prefix
    fn set_href_prefix(&mut self, prefix: String) {
        self.href_prefix = prefix.clone();

        for action in self.available_actions.values_mut() {
            action.set_href_prefix(prefix.clone());
        }

        for event in self.available_events.values_mut() {
            event.set_href_prefix(prefix.clone());
        }

        for property in self.properties.values_mut() {
            property.set_href_prefix(prefix.clone());
        }

        for actions in self.actions.values_mut() {
            for action in actions {
                action.set_href_prefix(prefix.clone());
            }
        }
    }

    /// Set the href of this thing's websocket.
    ///
    /// href -- the href
    fn set_ws_href(&mut self, href: String) {
        self.ws_href = Some(href);
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

    /// Get the type of the thing.
    ///
    /// Returns the type as a string.
    fn get_type(&self) -> String {
        self.type_.clone()
    }

    /// Get the description of the thing.
    ///
    /// Returns the description as a string.
    fn get_description(&self) -> String {
        self.description.clone()
    }

    /// Get the thing's properties as a dictionary.
    ///
    /// Returns the properties as a dictionary, i.e. name -> description.
    fn get_property_descriptions(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut descriptions: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        for (name, property) in self.properties.iter() {
            descriptions.insert(name.to_string(), json!(property.as_property_description()));
        }

        descriptions
    }

    /// Get the thing's actions as an array.
    ///
    /// Returns the action descriptions.
    fn get_action_descriptions(&self) -> serde_json::Value {
        let mut descriptions: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

        for actions in self.actions.values() {
            for action in actions {
                descriptions.push(action.as_action_description());
            }
        }

        json!(descriptions)
    }

    /// Get the thing's events as an array.
    ///
    /// Returns the event descriptions.
    fn get_event_descriptions(&self) -> serde_json::Value {
        let mut descriptions: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

        for event in &self.events {
            descriptions.push(event.as_event_description());
        }

        json!(descriptions)
    }

    /// Add a property to this thing.
    ///
    /// property -- property to add
    fn add_property(&mut self, mut property: Box<Property>) {
        property.set_href_prefix(self.get_href_prefix());

        unsafe {
            property.register(Arc::from_raw(self));
        }

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
    /// Returns a Property object, if found, else None.
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
    fn get_action(&self, action_name: String, action_id: String) -> Option<&Box<Action>> {
        match self.actions.get(&action_name) {
            Some(entry) => {
                for action in entry {
                    if action.get_id() == action_id {
                        return Some(action);
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
        self.event_notify(&event);
        self.events.push(event);
    }

    /// Add an available event.
    ///
    /// name -- name of the event
    /// metadata -- event metadata, i.e. type, description, etc., as a dict
    fn add_available_event(
        &mut self,
        name: String,
        mut metadata: serde_json::Map<String, serde_json::Value>,
    ) {
        metadata.insert("href".to_owned(), json!(format!("/events/{}", name)));

        let event = AvailableEvent::new(metadata);
        self.available_events.insert(name, event);
    }

    /// Perform an action on the thing.
    ///
    /// action_name -- name of the action
    /// input_ -- any action inputs
    ///
    /// Returns the action that was created.
    fn perform_action(
        &self,
        action_name: String,
        input: Option<&serde_json::Value>,
    ) -> Option<Box<Action>> {
        if !self.available_actions.contains_key(&action_name) {
            return None;
        }

        let action_type = self.available_actions.get(&action_name).unwrap();
        if !action_type.validate_action_input(input) {
            return None;
        }

        None
        // TODO
        //        action = action_type['class'](self, input_=input_)
        //unsafe {
        //    property.register(Arc::from_raw(self));
        //}
        //        action.set_href_prefix(self.href_prefix)
        //        self.action_notify(action)
        //        self.actions[action_name].append(action)
        //        return action
    }

    /// Remove an existing action.
    ///
    /// action_name -- name of the action
    /// action_id -- ID of the action
    ///
    /// Returns a boolean indicating the presence of the action.
    fn remove_action(&self, action_name: String, action_id: String) -> bool {
        let action = self.get_action(action_name, action_id);
        match action {
            Some(a) => {
                a.cancel();
                //self.actions[action_name].remove(action)
                true
            }
            None => false,
        }
    }

    /// Add an available action.
    ///
    /// name -- name of the action
    /// metadata -- action metadata, i.e. type, description, etc., as a dict
    /// cls -- class to instantiate for this action
    fn add_available_action(
        &mut self,
        name: String,
        mut metadata: serde_json::Map<String, serde_json::Value>,
        //, cls) {
    ) {
        metadata.insert("href".to_owned(), json!(format!("/actions/{}", name)));

        let action = AvailableAction::new(metadata);
        self.available_actions.insert(name.clone(), action);
        self.actions.insert(name, Vec::new());
    }

    /// Add a new websocket subscriber.
    ///
    /// ws -- the websocket
    fn add_subscriber(&mut self, ws: Weak<ThingWebSocket>) {
        match ws.upgrade() {
            Some(ws) => {
                self.subscribers.insert(ws.get_id(), Arc::downgrade(&ws));
            }
            None => (),
        }
    }

    /// Remove a websocket subscriber.
    ///
    /// ws -- the websocket
    fn remove_subscriber(&mut self, ws_id: String) {
        self.subscribers.remove(&ws_id);

        for event in self.available_events.values_mut() {
            event.remove_subscriber(ws_id.clone());
        }
    }

    /// Add a new websocket subscriber to an event.
    ///
    /// name -- name of the event
    /// ws -- the websocket
    fn add_event_subscriber(&mut self, name: String, ws: Weak<ThingWebSocket>) {
        if self.available_events.contains_key(&name) {
            self.available_events
                .get_mut(&name)
                .unwrap()
                .add_subscriber(ws);
        }
    }

    /// Remove a websocket subscriber from an event.
    ///
    /// name -- name of the event
    /// ws -- the websocket
    fn remove_event_subscriber(&mut self, name: String, ws_id: String) {
        if self.available_events.contains_key(&name) {
            self.available_events
                .get_mut(&name)
                .unwrap()
                .remove_subscriber(ws_id);
        }
    }

    /// Notify all subscribers of an event.
    ///
    /// event -- the event that occurred
    fn event_notify(&self, event: &Box<Event>) {
        if !self.available_events.contains_key(&event.get_name()) {
            return;
        }

        for subscriber in self.available_events
            .get(&event.get_name())
            .unwrap()
            .get_subscribers()
        {
            // TODO
            //            try:
            //                subscriber.write_message(json.dumps({
            //                    'messageType': 'event',
            //                    'data': event.as_event_description(),
            //                }))
            //            except tornado.websocket.WebSocketClosedError:
            //                pass
        }
    }
}

impl PropertyObserver for BaseThing {
    /// Notify all subscribers of a property change.
    ///
    /// property -- the property that changed
    fn property_notify(&self, name: String, value: serde_json::Value) {
        for subscriber in &self.subscribers {
            // TODO
            //            try:
            //                subscriber.write_message(json.dumps({
            //                    'messageType': 'propertyStatus',
            //                    'data': {
            //                        property_.name: property_.get_value(),
            //                    }
            //                }))
            //            except tornado.websocket.WebSocketClosedError:
            //                pass
        }
    }
}

impl ActionObserver for BaseThing {
    /// Notify all subscribers of an action status change.
    ///
    /// action -- the action whose status changed
    fn action_notify(&self, action: serde_json::Map<String, serde_json::Value>) {
        for subscriber in &self.subscribers {
            // TODO
            //            try:
            //                subscriber.write_message(json.dumps({
            //                    'messageType': 'actionStatus',
            //                    'data': action.as_action_description(),
            //                }))
            //            except tornado.websocket.WebSocketClosedError:
            //                pass
        }
    }
}

pub struct AvailableAction {
    metadata: serde_json::Map<String, serde_json::Value>,
    // TODO: class
}

impl AvailableAction {
    pub fn new(metadata: serde_json::Map<String, serde_json::Value>) -> AvailableAction {
        AvailableAction {
            metadata: metadata,
            //  TODO: class: cls,
        }
    }

    pub fn set_href_prefix(&mut self, prefix: String) {
        let href = format!(
            "{}{}",
            prefix,
            self.metadata.get("href").unwrap().as_str().unwrap()
        );
        self.metadata.insert("href".to_owned(), json!(href));
    }

    pub fn get_metadata(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.metadata
    }

    pub fn get_cls(&self) {
        // TODO
    }

    pub fn validate_action_input(&self, input: Option<&serde_json::Value>) -> bool {
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

pub struct AvailableEvent {
    metadata: serde_json::Map<String, serde_json::Value>,
    subscribers: HashMap<String, Weak<ThingWebSocket>>,
}

impl AvailableEvent {
    pub fn new(metadata: serde_json::Map<String, serde_json::Value>) -> AvailableEvent {
        AvailableEvent {
            metadata: metadata,
            subscribers: HashMap::new(),
        }
    }

    pub fn set_href_prefix(&mut self, prefix: String) {
        let href = format!(
            "{}{}",
            prefix,
            self.metadata.get("href").unwrap().as_str().unwrap()
        );
        self.metadata.insert("href".to_owned(), json!(href));
    }

    pub fn get_metadata(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.metadata
    }

    pub fn add_subscriber(&mut self, ws: Weak<ThingWebSocket>) {
        match ws.upgrade() {
            Some(ws) => {
                self.subscribers.insert(ws.get_id(), Arc::downgrade(&ws));
            }
            None => (),
        }
    }

    pub fn remove_subscriber(&mut self, ws_id: String) {
        self.subscribers.remove(&ws_id);
    }

    pub fn get_subscribers(&self) -> &HashMap<String, Weak<ThingWebSocket>> {
        &self.subscribers
    }
}
