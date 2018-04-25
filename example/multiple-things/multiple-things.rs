extern crate env_logger;
extern crate rand;
#[macro_use]
extern crate serde_json;
extern crate uuid;
extern crate webthing;

use rand::Rng;
use std::{thread, time};
use std::sync::Arc;
use uuid::Uuid;
use webthing::{Action, BaseAction, BaseEvent, BaseProperty, BaseThing, Event, Property, Thing,
               WebThingServer};

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

/// A dimmable light that logs received commands to stdout.
fn make_light() -> Box<BaseThing> {
    let mut thing = BaseThing::new(
        "My Lamp".to_owned(),
        Some("dimmableLight".to_owned()),
        Some("A web connected lamp".to_owned()),
    );

    let on_description = json!({
        "type": "boolean",
        "description": "Whether the lamp is turned on"
    });
    let on_description = on_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "on".to_owned(),
        json!(true),
        Some(on_description),
    )));

    let level_description = json!({
        "type": "number",
        "description": "The level of light from 0-100",
        "minimum": 0,
        "maximum": 100
    });
    let level_description = level_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "level".to_owned(),
        json!(50),
        Some(level_description),
    )));

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

/// A humidity sensor which updates its measurement every few seconds.
fn make_sensor() -> Box<BaseThing> {
    let mut thing = BaseThing::new(
        "My Humidity Sensor".to_owned(),
        Some("multiLevelSensor".to_owned()),
        Some("A web connected humidity sensor".to_owned()),
    );

    let on_description = json!({
        "type": "boolean",
        "description": "Whether the lamp is turned on"
    });
    let on_description = on_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "on".to_owned(),
        json!(true),
        Some(on_description),
    )));

    let level_description = json!({
        "type": "number",
        "description": "The current humidity in %",
        "unit": "%"
    });
    let level_description = level_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "level".to_owned(),
        json!(0),
        Some(level_description),
    )));

    /*
    let thing = Arc::new(thing);
    let cloned = thing.clone();
    thread::spawn(move || {
        let mut rng = rand::thread_rng();

        // Mimic an actual sensor updating its reading every couple seconds.
        loop {
            let mut t = cloned.clone();
            thread::sleep(time::Duration::from_millis(3000));
            let prop = Arc::get_mut(&mut t).unwrap().find_property("level".to_owned()).unwrap();
            prop.set_value(json!(70.0 * rng.gen_range::<f32>(0.0, 1.0) * (-0.5 + rng.gen_range::<f32>(0.0, 1.0))));
        }
    });
    */

    Box::new(thing)
}

fn main() {
    env_logger::init();

    let mut things: Vec<Box<Thing + 'static>> = Vec::new();

    // Create a thing that represents a dimmable light
    things.push(make_light());

    // Create a thing that represents a humidity sensor
    things.push(make_sensor());

    // If adding more than one thing here, be sure to set the `name`
    // parameter to some string, which will be broadcast via mDNS.
    // In the single thing case, the thing's name will be broadcast.
    let server = WebThingServer::new(
        things,
        Some("LightAndTempDevice".to_owned()),
        Some(8888),
        None,
    );
    server.start();
}
