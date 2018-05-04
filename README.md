# webthing

Implementation of an HTTP [Web Thing](https://iot.mozilla.org/wot/).

# Using

If you're using `Cargo`, just add `webthing` to your `Cargo.toml`:

```toml
[dependencies]
webthing = "*"
```

# Example

In this example we will set up a dimmable light and a humidity sensor (both using fake data, of course). Both working examples can be found in [here](https://github.com/mozilla-iot/webthing-rust/tree/master/example).

## Dimmable Light

Imagine you have a dimmable Light that you want to expose via the web of things API. The Light can be turned on/off and the brightness can be set from 0% to 100%. Besides the name, description, and type, a `dimmableLight` is required to expose two properties:
* `on`: the state of the light, whether it is turned on or off
    * Setting this property via a `PUT {"on": true/false}` call to the REST API toggles the light.
* `level`: the brightness level of the light from 0-100%
    * Setting this property via a PUT call to the REST API sets the brightness level of this light.

First we create a new Thing:

```rust
let mut light = BaseThing::new(
    "My Lamp".to_owned(),
    Some("dimmableLight".to_owned()),
    Some("A web connected lamp".to_owned()),
);
```

Now we can add the required properties.

The **`on`** property reports and sets the on/off state of the light. For our purposes, we just want to log the new state if the light is switched on/off.

```rust
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
```

The **`level`** property reports the brightness level of the light and sets the level. Like before, instead of actually setting the level of a light, we just log the level to std::out.

```rust
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
```

Now we can add our newly created thing to the server and start it:

```rust
let mut things: Vec<Arc<RwLock<Box<Thing + 'static>>>> = Vec::new();
things.push(Arc::new(RwLock::new(Box::new(light)));

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
```

This will start the server, making the light available via the WoT REST API and announcing it as a discoverable resource on your local network via mDNS.

## Sensor

Let's now also connect a humidity sensor to the server we set up for our light.

A `multiLevelSensor` (a sensor that can also return a level instead of just true/false) has two required properties (besides the name, type, and  optional description): **`on`** and **`level`**. We want to monitor those properties and get notified if the value changes.

First we create a new Thing:

```rust
let mut thing = BaseThing::new(
    "My Humidity Sensor".to_owned(),
    Some("multiLevelSensor".to_owned()),
    Some("A web connected humidity sensor".to_owned()),
);
```

Then we create and add the appropriate properties:
* `on`: tells us whether the sensor is on (i.e. high), or off (i.e. low)

    ```rust
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
    ```

* `level`: tells us what the sensor is actually reading
    * Contrary to the light, the value cannot be set via an API call, as it wouldn't make much sense, to SET what a sensor is reading. Therefore, we are utilizing a *readOnly* value.

    ```rust
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
    ```

Now we have a sensor that constantly reports 0%. To make it usable, we need a thread or some kind of input when the sensor has a new reading available. For this purpose we start a thread that queries the physical sensor every few seconds. For our purposes, it just calls a fake method.

```rust
let sensor = Arc::new(RwLock::new(Box::new(sensor))));
let cloned = sensor.clone();
thread::spawn(move || {
    let mut rng = rand::thread_rng();

    // Mimic an actual sensor updating its reading every couple seconds.
    loop {
        thread::sleep(time::Duration::from_millis(3000));
        let t = cloned.clone();
        let new_value = json!(
            70.0 * rng.gen_range::<f32>(0.0, 1.0) * (-0.5 + rng.gen_range::<f32>(0.0, 1.0))
        );

        {
            let mut t = t.write().unwrap();
            let prop = t.find_property("level".to_owned()).unwrap();
            let _ = prop.set_value(new_value.clone());
        }

        t.write()
            .unwrap()
            .property_notify("level".to_owned(), new_value);
    }
});
```

This will update our property with random sensor readings. The new property value is then sent to all websocket listeners.
