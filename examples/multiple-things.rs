extern crate env_logger;
extern crate rand;
#[macro_use]
extern crate serde_json;
extern crate uuid;
extern crate webthing;

use rand::Rng;
use std::sync::{Arc, RwLock, Weak};
use std::{thread, time};
use uuid::Uuid;
use webthing::property::ValueForwarder;
use webthing::server::ActionGenerator;
use webthing::{
    Action, BaseAction, BaseEvent, BaseProperty, BaseThing, Event, Thing, ThingsType,
    WebThingServer,
};

pub struct OverheatedEvent(BaseEvent);

impl OverheatedEvent {
    fn new(data: Option<serde_json::Value>) -> OverheatedEvent {
        OverheatedEvent(BaseEvent::new("overheated".to_owned(), data))
    }
}

impl Event for OverheatedEvent {
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

impl FadeAction {
    fn new(
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
}

impl Action for FadeAction {
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
            let _ = thing.set_property(
                "brightness".to_owned(),
                input.get("brightness").unwrap().clone(),
            );
            thing.add_event(Box::new(OverheatedEvent::new(Some(json!(102)))));

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
            "fade" => Some(Box::new(FadeAction::new(input, thing))),
            _ => None,
        }
    }
}

struct OnValueForwarder;

impl ValueForwarder for OnValueForwarder {
    fn set_value(&mut self, value: serde_json::Value) -> Result<serde_json::Value, &'static str> {
        println!("On-State is now {}", value);
        Ok(value)
    }
}

struct BrightnessValueForwarder;

impl ValueForwarder for BrightnessValueForwarder {
    fn set_value(&mut self, value: serde_json::Value) -> Result<serde_json::Value, &'static str> {
        println!("Brightness is now {}", value);
        Ok(value)
    }
}

/// A dimmable light that logs received commands to stdout.
fn make_light() -> Arc<RwLock<Box<Thing + 'static>>> {
    let mut thing = BaseThing::new(
        "My Lamp".to_owned(),
        Some(vec!["OnOffSwitch".to_owned(), "Light".to_owned()]),
        Some("A web connected lamp".to_owned()),
    );

    let on_description = json!({
        "@type": "OnOffProperty",
        "title": "On/Off",
        "type": "boolean",
        "description": "Whether the lamp is turned on"
    });
    let on_description = on_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "on".to_owned(),
        json!(true),
        Some(Box::new(OnValueForwarder)),
        Some(on_description),
    )));

    let brightness_description = json!({
        "@type": "BrightnessProperty",
        "title": "Brightness",
        "type": "integer",
        "description": "The level of light from 0-100",
        "minimum": 0,
        "maximum": 100,
        "unit": "percent"
    });
    let brightness_description = brightness_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "brightness".to_owned(),
        json!(50),
        Some(Box::new(BrightnessValueForwarder)),
        Some(brightness_description),
    )));

    let fade_metadata = json!({
        "title": "Fade",
        "description": "Fade the lamp to a given level",
        "input": {
            "type": "object",
            "required": [
                "brightness",
                "duration"
            ],
            "properties": {
                "brightness": {
                    "type": "integer",
                    "minimum": 0,
                    "maximum": 100,
                    "unit": "percent"
                },
                "duration": {
                    "type": "integer",
                    "minimum": 1,
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
        "unit": "degree celsius"
    });
    let overheated_metadata = overheated_metadata.as_object().unwrap().clone();
    thing.add_available_event("overheated".to_owned(), overheated_metadata);

    Arc::new(RwLock::new(Box::new(thing)))
}

/// A humidity sensor which updates its measurement every few seconds.
fn make_sensor() -> Arc<RwLock<Box<Thing + 'static>>> {
    let mut thing = BaseThing::new(
        "My Humidity Sensor".to_owned(),
        Some(vec!["MultiLevelSensor".to_owned()]),
        Some("A web connected humidity sensor".to_owned()),
    );

    let level_description = json!({
        "@type": "LevelProperty",
        "title": "Humidity",
        "type": "number",
        "description": "The current humidity in %",
        "minimum": 0,
        "maximum": 100,
        "unit": "percent",
        "readOnly": true
    });
    let level_description = level_description.as_object().unwrap().clone();
    thing.add_property(Box::new(BaseProperty::new(
        "level".to_owned(),
        json!(0),
        None,
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
            let new_value = 70.0
                * rng.gen_range::<f32, f32, f32>(0.0, 1.0)
                * (-0.5 + rng.gen_range::<f32, f32, f32>(0.0, 1.0));
            let new_value = json!(new_value.abs());

            println!("setting new humidity level: {}", new_value);

            {
                let mut t = t.write().unwrap();
                let prop = t.find_property("level".to_owned()).unwrap();
                let _ = prop.set_cached_value(new_value.clone());
            }

            t.write()
                .unwrap()
                .property_notify("level".to_owned(), new_value);
        }
    });

    // If adding more than one thing, use ThingsType::Multiple() with a name.
    // In the single thing case, the thing's name will be broadcast.
    let mut server = WebThingServer::new(
        ThingsType::Multiple(things, "LightAndTempDevice".to_owned()),
        Some(8888),
        None,
        None,
        Box::new(Generator),
        None,
    );
    server.create();
    server.start();
}
