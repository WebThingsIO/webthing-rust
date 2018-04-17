/// An observable, settable value interface.

use serde_json;

pub trait Observer {
    fn notify(&self, value: serde_json::Value);
}

pub trait ValueForwarder {
    fn handle(&self, value: serde_json::Value) -> Result<(), &'static str>;
}

pub struct ReadOnlyValue;

impl ValueForwarder for ReadOnlyValue {
    fn handle(&self, _value: serde_json::Value) -> Result<(), &'static str> {
        Err("Read-only value")
    }
}

/// A property value.
///
/// This is used for communicating between the Thing representation and the
/// actual physical thing implementation.
///
/// Notifies all observers when the underlying value changes through an
/// external update (command to turn the light off) or if the underlying sensor
/// reports a new value.
pub trait Value {
    /// Set a new value for this thing.
    ///
    /// value -- value to set
    fn set(&mut self, value: serde_json::Value) -> Result<(), &'static str> {
        let res = self.forward_value(value.clone());

        if res.is_err() {
            return res;
        }

        if value != self.get() {
            self.set_cached_value(value.clone());
        }

        self.notify_of_external_update(value);
        Ok(())
    }

    /// Forward the value to the physical (or virtual) device.
    ///
    /// value -- value to forward
    fn forward_value(&self, value: serde_json::Value) -> Result<(), &'static str>;

    /// Set the cached value.
    ///
    /// value -- value to set
    fn set_cached_value(&mut self, value: serde_json::Value);

    /// Return the last known value from the underlying thing.
    fn get(&self) -> serde_json::Value;

    /// Notify observers of a new value.
    ///
    /// value -- new value
    fn notify_of_external_update(&mut self, value: serde_json::Value);
}

pub struct BaseValue<F: ValueForwarder, O: 'static + Observer> {
    last_value: serde_json::Value,
    value_forwarder: F,
    observer: Option<&'static O>,
}

impl<F: ValueForwarder, O: Observer> BaseValue<F, O> {
    /// Initialize the object.
    ///
    /// initial_value -- the initial value
    /// value_forwarder -- the method that updates the actual value on the thing
    /// observer -- observer of value changes
    pub fn new(initial_value: serde_json::Value, value_forwarder: F) -> BaseValue<F, O> {
        BaseValue {
            last_value: initial_value,
            value_forwarder: value_forwarder,
            observer: None,
        }
    }

    /// Set the observer
    pub fn add_observer(&mut self, observer: &'static O) {
        self.observer = Some(observer)
    }
}

impl<F: ValueForwarder, O: Observer> Value for BaseValue<F, O> {
    /// Forward the value to the physical (or virtual) device.
    ///
    /// value -- value to forward
    fn forward_value(&self, value: serde_json::Value) -> Result<(), &'static str> {
        self.value_forwarder.handle(value)
    }

    /// Set the cached value.
    ///
    /// value -- value to set
    fn set_cached_value(&mut self, value: serde_json::Value) {
        self.last_value = value;
    }

    /// Return the last known value from the underlying thing.
    fn get(&self) -> serde_json::Value {
        self.last_value.clone()
    }

    /// Notify observers of a new value.
    ///
    /// value -- new value
    fn notify_of_external_update(&mut self, value: serde_json::Value) {
        if value != self.get() && self.observer.is_some() {
            self.observer.unwrap().notify(value);
        }
    }
}
