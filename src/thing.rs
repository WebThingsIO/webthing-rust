use std::marker::Sized;

use action::Action;
use event::Event;
use property::Property;

pub trait Thing: Sized {
    fn property_notify<'a, 'b, P: Property<'a, Self>>(&self, property: &'b P);
    fn action_notify<'a, 'b, A: Action<'a, Self>>(&self, action: &'b A);
    fn event_notify<'a, 'b, E: Event<'a, Self>>(&self, event: &'b E);
}

pub struct BaseThing;

impl Thing for BaseThing {
    fn property_notify<'a, 'b, P: Property<'a, Self>>(&self, property: &'b P) {
    }

    fn action_notify<'a, 'b, A: Action<'a, Self>>(&self, action: &'b A) {
    }

    fn event_notify<'a, 'b, E: Event<'a, Self>>(&self, event: &'b E) {
    }
}
