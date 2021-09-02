use serde_json;
use serde_json::json;
use std::marker::{Send, Sync};
use valico::json_schema;

/// Used to forward a new property value to the physical/virtual device.
pub trait ValueForwarder: Send + Sync {
    /// Set the new value of the property.
    fn set_value(&mut self, value: serde_json::Value) -> Result<serde_json::Value, &'static str>;
}

/// High-level Property trait.
pub trait Property: Send + Sync {
    /// Validate new property value before setting it.
    ///
    /// Returns a result indicating validity.
    fn validate_value(&self, value: &serde_json::Value) -> Result<(), &'static str> {
        let mut description = self.get_metadata();
        description.remove("@type");
        description.remove("unit");
        description.remove("title");

        if let Some(b) = description.get("readOnly").and_then(|b| b.as_bool()) {
            if b {
                return Err("Read-only property");
            }
        }

        let mut scope = json_schema::Scope::new();
        match scope.compile_and_return(json!(description), true) {
            Ok(validator) => {
                if validator.validate(value).is_valid() {
                    Ok(())
                } else {
                    Err("Invalid property value")
                }
            }
            Err(_) => Err("Invalid property schema"),
        }
    }

    /// Get the property description.
    ///
    /// Returns a JSON value describing the property.
    fn as_property_description(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut description = self.get_metadata().clone();
        let link = json!(
            {
                "rel": "property",
                "href": self.get_href(),
            }
        );

        if let Some(links) = description
            .get_mut("links")
            .map(|links| links.as_array_mut().unwrap())
        {
            links.push(link);
        } else {
            description.insert("links".to_string(), json!([link]));
        }
        description
    }

    /// Set the prefix of any hrefs associated with this property.
    fn set_href_prefix(&mut self, prefix: String);

    /// Get the href of this property.
    fn get_href(&self) -> String;

    /// Get the current property value.
    fn get_value(&self) -> serde_json::Value;

    /// Set the current value of the property with the value forwarder.
    fn set_value(&mut self, value: serde_json::Value) -> Result<(), &'static str>;

    /// Set the cached value of the property.
    fn set_cached_value(&mut self, value: serde_json::Value) -> Result<(), &'static str>;

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
    value_forwarder: Option<Box<dyn ValueForwarder>>,
    href_prefix: String,
    href: String,
    metadata: serde_json::Map<String, serde_json::Value>,
}

impl BaseProperty {
    /// Create a new BaseProperty.
    ///
    /// # Arguments
    ///
    /// * `name` - name of the property
    /// * `initial_value` - initial property value
    /// * `value_forwarder` - optional value forwarder; property will be read-only if None
    /// * `metadata` - property metadata, i.e. type, description, unit, etc., as a JSON map
    pub fn new(
        name: String,
        initial_value: serde_json::Value,
        value_forwarder: Option<Box<dyn ValueForwarder>>,
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
    fn set_value(&mut self, value: serde_json::Value) -> Result<(), &'static str> {
        let result = self.validate_value(&value);
        if result.is_err() {
            return result;
        }

        match self.value_forwarder {
            Some(ref mut vf) => match vf.set_value(value) {
                Ok(v) => {
                    self.last_value = v;
                    Ok(())
                }
                Err(e) => Err(e),
            },
            None => {
                self.last_value = value;
                Ok(())
            }
        }
    }

    /// Set the cached value of the property.
    fn set_cached_value(&mut self, value: serde_json::Value) -> Result<(), &'static str> {
        self.last_value = value;
        Ok(())
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
