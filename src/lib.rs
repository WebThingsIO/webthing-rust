#![deny(missing_docs)]

//! Implementation of an HTTP [Web Thing](https://webthings.io/api/).

extern crate std;

/// Action trait and base implementation.
pub mod action;

/// ActionGenerator trait and base implementation.
pub mod action_generator;

/// Event trait and base implementation.
pub mod event;

/// Property trait and base implementation.
pub mod property;

/// WebThingServer implementation.
pub mod server;

/// Thing trait and base implementation.
pub mod thing;

/// Utility functions.
pub mod utils;

pub use action::{Action, BaseAction};
pub use action_generator::BaseActionGenerator;
pub use event::{BaseEvent, Event};
pub use property::{BaseProperty, Property};
pub use server::{ThingsType, WebThingServer};
pub use thing::{BaseThing, Thing};
