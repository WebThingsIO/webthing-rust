#![deny(missing_docs)]

//! Implementation of an HTTP [Web Thing](https://iot.mozilla.org/wot/).

extern crate actix;
extern crate actix_web;
extern crate chrono;
extern crate hostname;
extern crate libmdns;
extern crate openssl;
#[macro_use]
extern crate serde_json;
extern crate uuid;
extern crate valico;

/// Action trait and base implementation.
pub mod action;

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
pub use event::{BaseEvent, Event};
pub use property::{BaseProperty, Property};
pub use server::{ThingsType, WebThingServer};
pub use thing::{BaseThing, Thing};
