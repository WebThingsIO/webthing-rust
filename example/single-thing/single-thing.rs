extern crate env_logger;
#[macro_use]
extern crate serde_json;
extern crate uuid;
extern crate webthing;

use std::{thread, time};
use uuid::Uuid;
use webthing::{Action, BaseAction, BaseEvent, BaseProperty, BaseThing, Event, Property, Thing, WebThingServer};


pub struct OverheatedEvent(BaseEvent);

impl Event for OverheatedEvent {
    fn new(name: String, data: Option<serde_json::Value>) -> OverheatedEvent {
        OverheatedEvent(BaseEvent::new("overheated".to_owned(), data))
    }

    fn get_name(&self) -> String {
        self.0.get_name()
    }

    fn get_data(&self) -> Option<serde_json::Value> {
        self.0.get_data()
    }

    fn get_time(&self) -> String {
        self.0.get_time()
    }
}

pub struct FadeAction(BaseAction);

/*
class FadeAction(Action):

    def __init__(self, thing, input_):
        Action.__init__(self, uuid.uuid4().hex, thing, 'fade', input_=input_)

    def perform_action(self):
        time.sleep(self.input['duration'] / 1000)
        self.thing.set_property('level', self.input['level'])
        self.thing.add_event(OverheatedEvent(self.thing, 102))
*/

fn make_thing() -> Box<BaseThing> {
    let mut thing = BaseThing::new("My Lamp".to_owned(), None, Some("A web connected lamp".to_owned()));

    let on_description = json!({
        "type": "boolean",
        "description": "Whether the lamp is turned on"
    });
    let on_description = on_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new("on".to_owned(), json!(true), Some(on_description))));

    let level_description = json!({
        "type": "number",
        "description": "The level of light from 0-100",
        "minimum": 0,
        "maximum": 100
    });
    let level_description = level_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new("level".to_owned(), json!(50), Some(level_description))));

    let fade_metadata = json!({
        "description": "Fade the lamp to a given level",
        "input": {
            "type": "object",
            "required": [
                "level",
                "duration"
            ],
            "properties": {
                "level": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 100
                },
                "duration": {
                    "type": "number",
                    "unit": "milliseconds"
                }
            }
        }
    });
    let fade_metadata = fade_metadata.as_object().unwrap().clone();
    thing.add_available_action("fade".to_owned(), fade_metadata);

    let overheated_metadata = json!({
        "description": "The lamp has exceeded its safe operating temperature",
        "type": "number",
        "unit": "celsius"
    });
    let overheated_metadata = overheated_metadata.as_object().unwrap().clone();
    thing.add_available_event("overheated".to_owned(), overheated_metadata);

    Box::new(thing)
}

fn main() {
    env_logger::init();

    let mut things: Vec<Box<Thing + 'static>> = Vec::new();
    things.push(make_thing());

    // If adding more than one thing here, be sure to set the `name`
    // parameter to some string, which will be broadcast via mDNS.
    // In the single thing case, the thing's name will be broadcast.
    let server = WebThingServer::new(things, None, Some(8888), None);
    server.start();
}
