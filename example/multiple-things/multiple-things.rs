extern crate env_logger;
extern crate rand;
#[macro_use]
extern crate serde_json;
extern crate uuid;
extern crate webthing;

use rand::Rng;
use std::{thread, time};
use std::sync::{Arc, RwLock, Weak};
use uuid::Uuid;
use webthing::{Action, BaseAction, BaseEvent, BaseProperty, BaseThing, Event, Property, Thing,
               WebThingServer};
use webthing::server::ActionGenerator;

pub struct OverheatedEvent(BaseEvent);

impl Event for OverheatedEvent {
    fn new(_name: String, data: Option<serde_json::Value>) -> OverheatedEvent {
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

impl Action for FadeAction {
    fn new(
        _id: String,
        _name: String,
        input: Option<serde_json::Map<String, serde_json::Value>>,
        thing: Weak<RwLock<Box<Thing>>>,
    ) -> FadeAction {
        FadeAction(BaseAction::new(
            Uuid::new_v4().to_string(),
            "fade".to_owned(),
            input,
            thing,
        ))
    }

    fn set_href_prefix(&mut self, prefix: String) {
        self.0.set_href_prefix(prefix)
    }

    fn get_id(&self) -> String {
        self.0.get_id()
    }

    fn get_name(&self) -> String {
        self.0.get_name()
    }

    fn get_href(&self) -> String {
        self.0.get_href()
    }

    fn get_status(&self) -> String {
        self.0.get_status()
    }

    fn get_time_requested(&self) -> String {
        self.0.get_time_requested()
    }

    fn get_time_completed(&self) -> Option<String> {
        self.0.get_time_completed()
    }

    fn get_input(&self) -> Option<serde_json::Map<String, serde_json::Value>> {
        self.0.get_input()
    }

    fn get_thing(&self) -> Option<Arc<RwLock<Box<Thing>>>> {
        self.0.get_thing()
    }

    fn set_status(&mut self, status: String) {
        self.0.set_status(status)
    }

    fn start(&mut self) {
        self.0.start()
    }

    fn perform_action(&mut self) {
        let thing = self.get_thing();
        if thing.is_none() {
            return;
        }

        let thing = thing.unwrap();
        let input = self.get_input().unwrap().clone();
        let name = self.get_name();
        let id = self.get_id();

        thread::spawn(move || {
            thread::sleep(time::Duration::from_millis(
                input.get("duration").unwrap().as_u64().unwrap(),
            ));

            let thing = thing.clone();
            let mut thing = thing.write().unwrap();
            let _ = thing.set_property("level".to_owned(), input.get("level").unwrap().clone());
            thing.add_event(Box::new(OverheatedEvent::new(
                "".to_owned(),
                Some(json!(102)),
            )));

            thing.finish_action(name, id);
        });
    }

    fn cancel(&mut self) {
        self.0.cancel()
    }

    fn finish(&mut self) {
        self.0.finish()
    }
}

struct Generator;

impl ActionGenerator for Generator {
    fn generate(
        &self,
        thing: Weak<RwLock<Box<Thing>>>,
        name: String,
        input: Option<&serde_json::Value>,
    ) -> Option<Box<Action>> {
        let input = match input {
            Some(v) => match v.as_object() {
                Some(o) => Some(o.clone()),
                None => None,
            },
            None => None,
        };

        let name: &str = &name;
        match name {
            "fade" => Some(Box::new(FadeAction::new(
                "".to_owned(),
                "".to_owned(),
                input,
                thing,
            ))),
            _ => None,
        }
    }
}

/// A dimmable light that logs received commands to stdout.
fn make_light() -> Arc<RwLock<Box<Thing + 'static>>> {
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
        false,
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
        false,
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

    Arc::new(RwLock::new(Box::new(thing)))
}

/// A humidity sensor which updates its measurement every few seconds.
fn make_sensor() -> Arc<RwLock<Box<Thing + 'static>>> {
    let mut thing = BaseThing::new(
        "My Humidity Sensor".to_owned(),
        Some("multiLevelSensor".to_owned()),
        Some("A web connected humidity sensor".to_owned()),
    );

    let on_description = json!({
        "type": "boolean",
        "description": "Whether the sensor is on"
    });
    let on_description = on_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "on".to_owned(),
        json!(true),
        true,
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
        true,
        Some(level_description),
    )));

    Arc::new(RwLock::new(Box::new(thing)))
}

fn main() {
    env_logger::init();

    let mut things: Vec<Arc<RwLock<Box<Thing + 'static>>>> = Vec::new();

    // Create a thing that represents a dimmable light
    things.push(make_light());

    // Create a thing that represents a humidity sensor
    let sensor = make_sensor();
    things.push(sensor.clone());

    let cloned = sensor.clone();
    thread::spawn(move || {
        let mut rng = rand::thread_rng();

        // Mimic an actual sensor updating its reading every couple seconds.
        loop {
            thread::sleep(time::Duration::from_millis(3000));
            let t = cloned.clone();
            let mut t = t.write().unwrap();
            let prop = t.find_property("level".to_owned()).unwrap();
            let _ = prop.set_value(json!(
                70.0 * rng.gen_range::<f32>(0.0, 1.0) * (-0.5 + rng.gen_range::<f32>(0.0, 1.0))
            ));
        }
    });

    // If adding more than one thing here, be sure to set the `name`
    // parameter to some string, which will be broadcast via mDNS.
    // In the single thing case, the thing's name will be broadcast.
    let server = WebThingServer::new(
        things,
        Some("LightAndTempDevice".to_owned()),
        Some(8888),
        None,
        Box::new(Generator),
    );
    server.start();
}
