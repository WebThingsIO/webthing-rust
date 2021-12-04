use std::sync::{RwLock, Weak};

use super::action::Action;
use super::thing::Thing;

/// Generator for new actions, based on name.
pub trait ActionGenerator: Send + Sync {
    /// Generate a new action, if possible.
    ///
    /// # Arguments
    ///
    /// * `thing` - thing associated with this action
    /// * `name` - name of the requested action
    /// * `input` - input for the action
    fn generate(
        &self,
        thing: Weak<RwLock<Box<dyn Thing>>>,
        name: String,
        input: Option<&serde_json::Value>,
    ) -> Option<Box<dyn Action>>;
}

/// Basic action generator implementation.
///
/// This always returns `None` and can be used when no actions are needed.
pub struct BaseActionGenerator;

impl ActionGenerator for BaseActionGenerator {
    fn generate(
        &self,
        _thing: Weak<RwLock<Box<dyn Thing>>>,
        _name: String,
        _input: Option<&serde_json::Value>,
    ) -> Option<Box<dyn Action>> {
        None
    }
}
