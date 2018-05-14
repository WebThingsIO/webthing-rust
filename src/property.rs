use serde_json;
use std::marker::{Send, Sync};

/// Used to forward a new property value to the physical/virtual device.
pub trait ValueForwarder: Send + Sync {
    /// Set the new value of the property.
    fn set_value(&mut self, serde_json::Value) -> Result<serde_json::Value, &'static str>;
}

/// A basic value forwarder that does nothing, but allows the property to be writable.
pub struct EmptyValueForwarder;

impl ValueForwarder for EmptyValueForwarder {
    /// Set the new value of the property.
    fn set_value(&mut self, value: serde_json::Value) -> Result<serde_json::Value, &'static str> {
        Ok(value)
    }
}

/// High-level Property trait.
pub trait Property: Send + Sync {
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

    /// Get the name of this property.
    fn get_name(&self) -> String;

    /// Get the metadata associated with this property.
    fn get_metadata(&self) -> serde_json::Map<String, serde_json::Value>;
}

/// Basic property implementation.
///
/// A Property represents an individual state value of a thing.
///
/// This can easily be used by other properties to handle most of the boring work.
pub struct BaseProperty {
    name: String,
    last_value: serde_json::Value,
    value_forwarder: Option<Box<ValueForwarder>>,
    href_prefix: String,
    href: String,
    metadata: serde_json::Map<String, serde_json::Value>,
}

impl BaseProperty {
    /// Create a new BaseProperty.
    ///
    /// name -- name of the property
    /// initial_value -- initial property value
    /// value_forwarder -- optional value forwarder; property will be read-only if None
    /// metadata -- property metadata, i.e. type, description, unit, etc., as a JSON map
    pub fn new(
        name: String,
        initial_value: serde_json::Value,
        value_forwarder: Option<Box<ValueForwarder>>,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> BaseProperty {
        let meta = match metadata {
            Some(m) => m,
            None => serde_json::Map::new(),
        };

        let href = format!("/properties/{}", name);

        BaseProperty {
            name: name,
            last_value: initial_value,
            value_forwarder: value_forwarder,
            href_prefix: "".to_owned(),
            href: href,
            metadata: meta,
        }
    }
}

impl Property for BaseProperty {
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
        match self.value_forwarder {
            Some(ref mut vf) => match vf.set_value(value) {
                Ok(v) => {
                    self.last_value = v;
                    Ok(())
                }
                Err(e) => Err(e),
            },
            None => Err("Read-only value"),
        }
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
