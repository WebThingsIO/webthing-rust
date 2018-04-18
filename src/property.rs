/// High-level Property base class implementation.

use serde_json;
use std::marker::Sized;

pub trait PropertyNotifier {
    fn property_notify<'a, P: Property<Self>>(&self, property: &'a P)
    where
        Self: Sized;
}

pub trait Property<P: PropertyNotifier> {
    /// Initialize the object.
    ///
    /// thing -- the Thing this property belongs to
    /// name -- name of the property
    /// value -- Value object to hold the property value
    /// metadata -- property metadata, i.e. type, description, unit, etc., as a Map
    fn new(
        notifier: P,
        name: String,
        initial_value: serde_json::Value,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Self
    where
        Self: Sized;

    /// Get the property description.
    ///
    /// Returns a JSON value describing the property.
    fn as_property_description(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut description = self.get_metadata().clone();
        description.insert("href".to_string(), json!(self.get_href()));
        description
    }

    /// Set the prefix of any hrefs associated with this property.
    ///
    /// prefix -- the prefix
    fn set_href_prefix(&mut self, prefix: String);

    /// Get the href of this property.
    fn get_href(&self) -> String;

    /// Get the current property value.
    fn get_value(&self) -> serde_json::Value;

    /// Set the current value of the property.
    ///
    /// value -- the value to set
    fn set_value(&mut self, value: serde_json::Value) -> Result<(), &'static str>;

    /// Forward the value to the physical (or virtual) device.
    ///
    /// value -- value to forward
    fn forward_value(&self, _value: serde_json::Value) -> Result<(), &'static str>;

    /// Get the name of this property.
    fn get_name(&self) -> String;

    /// Get the metadata associated with this property.
    fn get_metadata(&self) -> serde_json::Map<String, serde_json::Value>;
}

/// A Property represents an individual state value of a thing.
pub struct BaseProperty<P: PropertyNotifier> {
    notifier: P,
    name: String,
    last_value: serde_json::Value,
    href_prefix: String,
    href: String,
    metadata: serde_json::Map<String, serde_json::Value>,
}

impl<P: PropertyNotifier> Property<P> for BaseProperty<P> {
    /// Initialize the object.
    ///
    /// thing -- the Thing this property belongs to
    /// name -- name of the property
    /// value -- Value object to hold the property value
    /// metadata -- property metadata, i.e. type, description, unit, etc., as a Map
    fn new(
        notifier: P,
        name: String,
        initial_value: serde_json::Value,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> BaseProperty<P> {
        let meta = match metadata {
            Some(m) => m,
            None => serde_json::Map::new(),
        };

        let href = format!("/properties/{}", name);

        BaseProperty {
            notifier: notifier,
            name: name,
            last_value: initial_value,
            href_prefix: "".to_owned(),
            href: href,
            metadata: meta,
        }
    }

    /// Set the prefix of any hrefs associated with this property.
    ///
    /// prefix -- the prefix
    fn set_href_prefix(&mut self, prefix: String) {
        self.href_prefix = prefix;
    }

    /// Get the href of this property.
    fn get_href(&self) -> String {
        format!("{}{}", self.href_prefix, self.href).to_string()
    }

    /// Get the current property value.
    fn get_value(&self) -> serde_json::Value {
        self.last_value.clone()
    }

    /// Set the current value of the property.
    ///
    /// value -- the value to set
    fn set_value(&mut self, value: serde_json::Value) -> Result<(), &'static str> {
        let res = self.forward_value(value.clone());
        if res.is_err() {
            return res;
        }

        if value != self.last_value {
            self.last_value = value.clone();
        }

        self.notifier.property_notify(self);
        Ok(())
    }

    /// Forward the value to the physical (or virtual) device.
    ///
    /// value -- value to forward
    fn forward_value(&self, _value: serde_json::Value) -> Result<(), &'static str> {
        Err("Read-only value")
    }

    /// Get the name of this property.
    fn get_name(&self) -> String {
        self.name.clone()
    }

    /// Get the metadata associated with this property.
    fn get_metadata(&self) -> serde_json::Map<String, serde_json::Value> {
        self.metadata.clone()
    }
}
