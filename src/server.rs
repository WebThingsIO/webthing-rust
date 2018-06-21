/// Rust Web Thing server implementation.
use actix;
use actix::prelude::*;
use actix_web::server::{HttpHandler, HttpServer};
use actix_web::{middleware, pred, server, ws, App, Error, HttpRequest, HttpResponse, Json};
use libmdns;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use serde_json;
use std::marker::{Send, Sync};
use std::sync::{Arc, RwLock, Weak};
use std::time::Duration;
use uuid::Uuid;

use super::action::Action;
use super::thing::Thing;
use super::utils::get_ip;

/// Represents the things managed by the server.
#[derive(Clone)]
pub enum ThingsType {
    /// Set when there are multiple things managed by the server
    Multiple(Vec<Arc<RwLock<Box<Thing>>>>, String),
    /// Set when there is only one thing
    Single(Arc<RwLock<Box<Thing>>>),
}

/// Generator for new actions, based on name.
pub trait ActionGenerator: Send + Sync {
    /// Generate a new action, if possible.
    ///
    /// thing -- thing associated with this action
    /// name -- name of the requested action
    /// input -- input for the action
    fn generate(
        &self,
        thing: Weak<RwLock<Box<Thing>>>,
        name: String,
        input: Option<&serde_json::Value>,
    ) -> Option<Box<Action>>;
}

/// Shared app state, used by server threads.
pub struct AppState {
    things: Arc<ThingsType>,
    action_generator: Arc<Box<ActionGenerator>>,
}

impl AppState {
    /// Get the thing this request is for.
    ///
    /// thing_id -- ID of the thing to get, in string form
    ///
    /// Returns the thing, or None if not found.
    fn get_thing(&self, thing_id: Option<&str>) -> Option<Arc<RwLock<Box<Thing>>>> {
        match self.things.as_ref() {
            ThingsType::Multiple(ref inner_things, _) => {
                if thing_id.is_none() {
                    return None;
                }

                let id = thing_id.unwrap().parse::<usize>();

                if id.is_err() {
                    return None;
                }

                let id = id.unwrap();
                if id >= inner_things.len() {
                    None
                } else {
                    Some(inner_things[id].clone())
                }
            }
            ThingsType::Single(ref thing) => Some(thing.clone()),
        }
    }

    fn get_things(&self) -> Arc<ThingsType> {
        self.things.clone()
    }

    fn get_action_generator(&self) -> Arc<Box<ActionGenerator>> {
        self.action_generator.clone()
    }
}

/// Shared state used by individual websockets.
struct ThingWebSocket {
    id: String,
    thing_id: usize,
    things: Arc<ThingsType>,
    action_generator: Arc<Box<ActionGenerator>>,
}

impl ThingWebSocket {
    /// Get the ID of this websocket.
    fn get_id(&self) -> String {
        self.id.clone()
    }

    /// Get the thing associated with this websocket.
    fn get_thing(&self) -> Arc<RwLock<Box<Thing>>> {
        match self.things.as_ref() {
            ThingsType::Multiple(ref things, _) => things[self.thing_id].clone(),
            ThingsType::Single(ref thing) => thing.clone(),
        }
    }

    /// Drain all message queues associated with this websocket.
    fn drain_queue(&self, ctx: &mut ws::WebsocketContext<Self, AppState>) {
        ctx.run_later(Duration::from_millis(200), |act, ctx| {
            let thing = act.get_thing();
            let mut thing = thing.write().unwrap();

            let drains = thing.drain_queue(act.get_id());
            for iter in drains {
                for message in iter {
                    ctx.text(message);
                }
            }

            act.drain_queue(ctx);
        });
    }
}

impl Actor for ThingWebSocket {
    type Context = ws::WebsocketContext<Self, AppState>;
}

impl StreamHandler<ws::Message, ws::ProtocolError> for ThingWebSocket {
    fn started(&mut self, ctx: &mut Self::Context) {
        self.drain_queue(ctx);
    }

    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Pong(_) => (),
            ws::Message::Text(text) => {
                let message = serde_json::from_str(&text);
                if message.is_err() {
                    return ctx.text(
                        r#"
                        {
                            "messageType": "error",
                            "data": {
                                "status": "400 Bad Request",
                                "message": "Parsing request failed"
                            }
                        }"#,
                    );
                }

                let message: serde_json::Value = message.unwrap();
                if !message.is_object() {
                    return ctx.text(
                        r#"
                        {
                            "messageType": "error",
                            "data": {
                                "status": "400 Bad Request",
                                "message": "Parsing request failed"
                            }
                        }"#,
                    );
                }

                let message = message.as_object().unwrap();

                if !message.contains_key("messageType") || !message.contains_key("data") {
                    return ctx.text(
                        r#"
                        {
                            "messageType": "error",
                            "data": {
                                "status": "400 Bad Request",
                                "message": "Invalid message"
                            }
                        }"#,
                    );
                }

                let msg_type = message.get("messageType").unwrap().as_str();
                let data = message.get("data").unwrap().as_object();
                if msg_type.is_none() || data.is_none() {
                    return ctx.text(
                        r#"
                        {
                            "messageType": "error",
                            "data": {
                                "status": "400 Bad Request",
                                "message": "Invalid message"
                            }
                        }"#,
                    );
                }

                let msg_type = msg_type.unwrap();
                let data = data.unwrap();
                let thing = self.get_thing();

                match msg_type {
                    "setProperty" => for (property_name, property_value) in data.iter() {
                        if thing
                            .write()
                            .unwrap()
                            .set_property(property_name.to_string(), property_value.clone())
                            .is_err()
                        {
                            return ctx.text(
                                r#"
                                    {
                                        "messageType": "error",
                                        "data": {
                                            "status": "403 Forbidden",
                                            "message": "Read-only property"
                                        }
                                    }"#,
                            );
                        }
                    },
                    "requestAction" => for (action_name, action_params) in data.iter() {
                        let input = action_params.get("input");
                        let action = self.action_generator.generate(
                            Arc::downgrade(&self.get_thing()),
                            action_name.to_string(),
                            input,
                        );

                        if action.is_none() {
                            return ctx.text(format!(
                                r#"
                                {{
                                    "messageType": "error",
                                    "data": {{
                                        "status": "400 Bad Request",
                                        "message": "Invalid action request",
                                        "request": {}
                                    }}
                                }}"#,
                                text
                            ));
                        }

                        let action = action.unwrap();
                        let id = action.get_id();
                        let action = Arc::new(RwLock::new(action));

                        {
                            let mut thing = thing.write().unwrap();
                            let result = thing.add_action(action.clone(), input);

                            if result.is_err() {
                                return ctx.text(format!(
                                    r#"
                                    {{
                                        "messageType": "error",
                                        "data": {{
                                            "status": "400 Bad Request",
                                            "message": "Failed to start action: {}"
                                        }}
                                    }}"#,
                                    result.unwrap_err()
                                ));
                            }
                        }

                        thing
                            .write()
                            .unwrap()
                            .start_action(action_name.to_string(), id);
                    },
                    "addEventSubscription" => for event_name in data.keys() {
                        thing
                            .write()
                            .unwrap()
                            .add_event_subscriber(event_name.to_string(), self.get_id());
                    },
                    unknown => {
                        return ctx.text(format!(
                            r#"
                            {{
                                "messageType": "error",
                                "data": {{
                                    "status": "400 Bad Request",
                                    "message": "Unknown messageType: {}"
                                }}
                            }}"#,
                            unknown
                        ));
                    }
                }
            }
            ws::Message::Binary(_) => (),
            ws::Message::Close(_) => {
                let thing = self.get_thing();
                thing.write().unwrap().remove_subscriber(self.get_id());
            }
        }
    }
}

/// Handle a GET request to / when the server manages multiple things.
#[allow(non_snake_case)]
fn things_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let mut response: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    if let ThingsType::Multiple(things, _) = req.state().things.as_ref() {
        for thing in things.iter() {
            response.push(thing.read().unwrap().as_thing_description());
        }
    }
    HttpResponse::Ok().json(response)
}

/// Handle a GET request to /.
#[allow(non_snake_case)]
fn thing_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().as_thing_description()),
    }
}

/// Handle websocket on /.
#[allow(non_snake_case)]
fn thing_handler_WS(req: HttpRequest<AppState>) -> Result<HttpResponse, Error> {
    let thing_id = req.match_info().get("thing_id");

    match req.state().get_thing(thing_id) {
        None => Ok(HttpResponse::NotFound().finish()),
        Some(thing) => {
            let thing_id = match thing_id {
                None => 0,
                Some(id) => id.parse::<usize>().unwrap(),
            };
            let ws = ThingWebSocket {
                id: Uuid::new_v4().to_string(),
                thing_id: thing_id,
                things: req.state().get_things(),
                action_generator: req.state().get_action_generator(),
            };
            thing.write().unwrap().add_subscriber(ws.get_id());
            ws::start(req.clone(), ws)
        }
    }
}

/// Handle a GET request to /properties.
#[allow(non_snake_case)]
fn properties_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        HttpResponse::NotFound().finish()
    } else {
        // TODO: this is not yet defined in the spec
        HttpResponse::Ok().finish()
    }
}

/// Handle a GET request to /properties/<property>.
#[allow(non_snake_case)]
fn property_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    let property_name = req.match_info().get("property_name");
    if property_name.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let property_name = property_name.unwrap();
    let thing = thing.read().unwrap();
    if thing.has_property(property_name.to_string()) {
        HttpResponse::Ok()
            .json(json!({property_name: thing.get_property(property_name.to_string()).unwrap()}))
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a PUT request to /properties/<property>.
#[allow(non_snake_case)]
fn property_handler_PUT(
    req: HttpRequest<AppState>,
    message: Json<serde_json::Value>,
) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    let property_name = req.match_info().get("property_name");
    if property_name.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let property_name = property_name.unwrap();

    if !message.is_object() {
        return HttpResponse::BadRequest().finish();
    }

    let args = message.as_object().unwrap();

    if !args.contains_key(property_name) {
        return HttpResponse::BadRequest().finish();
    }

    let mut thing = thing.write().unwrap();
    if thing.has_property(property_name.to_string()) {
        if thing
            .set_property(
                property_name.to_string(),
                args.get(property_name).unwrap().clone(),
            )
            .is_ok()
        {
            HttpResponse::Ok().json(
                json!({property_name: thing.get_property(property_name.to_string()).unwrap()}),
            )
        } else {
            HttpResponse::Forbidden().finish()
        }
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a GET request to /actions.
#[allow(non_snake_case)]
fn actions_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_action_descriptions()),
    }
}

/// Handle a POST request to /actions.
#[allow(non_snake_case)]
fn actions_handler_POST(
    req: HttpRequest<AppState>,
    message: Json<serde_json::Value>,
) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    if !message.is_object() {
        return HttpResponse::BadRequest().finish();
    }

    let message = message.as_object().unwrap();

    let mut response: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for (action_name, action_params) in message.iter() {
        let input = action_params.get("input");

        let action = req.state().get_action_generator().generate(
            Arc::downgrade(&thing.clone()),
            action_name.to_string(),
            input,
        );

        if action.is_some() {
            let action = action.unwrap();
            let id = action.get_id();
            let action = Arc::new(RwLock::new(action));

            {
                let mut thing = thing.write().unwrap();
                let result = thing.add_action(action.clone(), input);

                if result.is_err() {
                    continue;
                }
            }

            response.insert(
                action_name.to_string(),
                action
                    .read()
                    .unwrap()
                    .as_action_description()
                    .get(action_name)
                    .unwrap()
                    .clone(),
            );

            thing
                .write()
                .unwrap()
                .start_action(action_name.to_string(), id);
        }
    }

    HttpResponse::Created().json(response)
}

/// Handle a GET request to /actions/<action_name>.
#[allow(non_snake_case)]
fn action_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        HttpResponse::NotFound().finish()
    } else {
        // TODO: this is not yet defined in the spec
        HttpResponse::Ok().finish()
    }
}

/// Handle a GET request to /actions/<action_name>/<action_id>.
#[allow(non_snake_case)]
fn action_id_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    let action_name = req.match_info().get("action_name");
    let action_id = req.match_info().get("action_id");
    if action_name.is_none() || action_id.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.read().unwrap();
    let action = thing.get_action(
        action_name.unwrap().to_string(),
        action_id.unwrap().to_string(),
    );
    if action.is_none() {
        HttpResponse::NotFound().finish()
    } else {
        let action = action.unwrap();
        let action = action.read().unwrap();
        HttpResponse::Ok().json(action.as_action_description())
    }
}

/// Handle a PUT request to /actions/<action_name>/<action_id>.
#[allow(non_snake_case)]
fn action_id_handler_PUT(
    req: HttpRequest<AppState>,
    _message: Json<serde_json::Value>,
) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        HttpResponse::NotFound().finish()
    } else {
        // TODO: this is not yet defined in the spec
        HttpResponse::Ok().finish()
    }
}

/// Handle a DELETE request to /actions/<action_name>/<action_id>.
#[allow(non_snake_case)]
fn action_id_handler_DELETE(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    let action_name = req.match_info().get("action_name");
    let action_id = req.match_info().get("action_id");
    if action_name.is_none() || action_id.is_none() {
        return HttpResponse::NotFound().finish();
    }

    if thing.write().unwrap().remove_action(
        action_name.unwrap().to_string(),
        action_id.unwrap().to_string(),
    ) {
        HttpResponse::NoContent().finish()
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a GET request to /events.
#[allow(non_snake_case)]
fn events_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_event_descriptions()),
    }
}

/// Handle a GET request to /events/<event_name>.
#[allow(non_snake_case)]
fn event_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        HttpResponse::NotFound().finish()
    } else {
        // TODO: this is not yet defined in the spec
        HttpResponse::Ok().finish()
    }
}

/// Server to represent a Web Thing over HTTP.
pub struct WebThingServer {
    ip: String,
    port: u16,
    name: String,
    ssl_options: Option<(String, String)>,
    server: HttpServer<Box<HttpHandler>>,
    mdns: Option<libmdns::Service>,
    system: actix::SystemRunner,
}

impl WebThingServer {
    /// Create a new WebThingServer.
    ///
    /// things -- list of Things managed by this server
    /// name -- name of this device -- this is only needed if the server is
    ///         managing multiple things
    /// port -- port to listen on (defaults to 80)
    /// ssl_options -- tuple of SSL options to pass to the actix web server
    /// action_generator -- action generator struct
    pub fn new(
        mut things: ThingsType,
        port: Option<u16>,
        ssl_options: Option<(String, String)>,
        action_generator: Box<ActionGenerator>,
    ) -> WebThingServer {
        let ip = get_ip();

        let port = match port {
            Some(p) => p,
            None => 80,
        };

        let name = match &things {
            ThingsType::Single(thing) => thing.read().unwrap().get_name(),
            ThingsType::Multiple(_, name) => name.to_owned(),
        };

        let ws_protocol = match ssl_options {
            Some(_) => "wss",
            None => "ws",
        };

        match &mut things {
            ThingsType::Multiple(ref mut things, _) => {
                for (idx, thing) in things.iter_mut().enumerate() {
                    let mut thing = thing.write().unwrap();
                    thing.set_href_prefix(format!("/{}", idx));
                    thing.set_ws_href(format!("{}://{}:{}/{}", ws_protocol, ip, port, idx));
                }
            }
            ThingsType::Single(thing) => {
                thing
                    .write()
                    .unwrap()
                    .set_ws_href(format!("{}://{}:{}", ws_protocol, ip, port));
            }
        }

        let generator_arc = Arc::new(action_generator);
        let arc_things = Arc::new(things.clone());
        let arc_things_clone = arc_things.clone();
        let server = match &things {
            ThingsType::Multiple(_, _) => {
                let inner_generator_arc = generator_arc.clone();
                server::new(move || {
                    vec![
                        App::with_state(AppState {
                            things: arc_things_clone.clone(),
                            action_generator: inner_generator_arc.clone(),
                        }).middleware(middleware::Logger::default())
                            .resource("/", |r| r.get().f(things_handler_GET))
                            .scope("/{thing_id}", |scope| {
                                scope
                                    .resource("", |r| {
                                        r.route()
                                            .filter(pred::Get())
                                            .filter(pred::Header("upgrade", "websocket"))
                                            .f(thing_handler_WS);
                                        r.get().f(thing_handler_GET)
                                    })
                                    .resource("/properties", |r| r.get().f(properties_handler_GET))
                                    .resource("/properties/{property_name}", |r| {
                                        r.get().f(property_handler_GET);
                                        r.put().with2(property_handler_PUT);
                                    })
                                    .resource("/actions", |r| {
                                        r.get().f(actions_handler_GET);
                                        r.post().with2(actions_handler_POST);
                                    })
                                    .resource("/actions/{action_name}", |r| {
                                        r.get().f(action_handler_GET)
                                    })
                                    .resource("/actions/{action_name}/{action_id}", |r| {
                                        r.get().f(action_id_handler_GET);
                                        r.delete().f(action_id_handler_DELETE);
                                        r.put().with2(action_id_handler_PUT);
                                    })
                                    .resource("/events", |r| r.get().f(events_handler_GET))
                                    .resource("/events/{event_name}", |r| {
                                        r.get().f(event_handler_GET)
                                    })
                            })
                            .boxed(),
                    ]
                })
            }
            ThingsType::Single(_thing) => {
                let inner_things_arc = arc_things.clone();
                let inner_generator_arc = generator_arc.clone();
                server::new(move || {
                    vec![
                        App::with_state(AppState {
                            things: inner_things_arc.clone(),
                            action_generator: inner_generator_arc.clone(),
                        }).middleware(middleware::Logger::default())
                            .resource("/", |r| {
                                r.route()
                                    .filter(pred::Get())
                                    .filter(pred::Header("upgrade", "websocket"))
                                    .f(thing_handler_WS);
                                r.get().f(thing_handler_GET)
                            })
                            .resource("/properties", |r| r.get().f(properties_handler_GET))
                            .resource("/properties/{property_name}", |r| {
                                r.get().f(property_handler_GET);
                                r.put().with2(property_handler_PUT);
                            })
                            .resource("/actions", |r| {
                                r.get().f(actions_handler_GET);
                                r.post().with2(actions_handler_POST);
                            })
                            .resource("/actions/{action_name}", |r| r.get().f(action_handler_GET))
                            .resource("/actions/{action_name}/{action_id}", |r| {
                                r.get().f(action_id_handler_GET);
                                r.delete().f(action_id_handler_DELETE);
                                r.put().with2(action_id_handler_PUT);
                            })
                            .resource("/events", |r| r.get().f(events_handler_GET))
                            .resource("/events/{event_name}", |r| r.get().f(event_handler_GET))
                            .boxed(),
                    ]
                })
            }
        };

        let sys = actix::System::new("webthing");

        WebThingServer {
            ip: ip,
            port: port,
            name: name,
            ssl_options: ssl_options,
            server: server,
            mdns: None,
            system: sys,
        }
    }

    /// Start listening for incoming connections.
    pub fn start(mut self) {
        let protocol = if self.ssl_options.is_none() {
            "http"
        } else {
            "https"
        };

        let responder = libmdns::Responder::new().unwrap();
        let svc = responder.register(
            "_http._tcp".to_owned(),
            self.name.clone(),
            self.port,
            &[
                &format!("url={}://{}:{}", protocol, self.ip, self.port),
                "webthing=true",
            ],
        );
        self.mdns = Some(svc);

        match self.ssl_options {
            Some(ref o) => {
                let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
                builder
                    .set_private_key_file(o.0.clone(), SslFiletype::PEM)
                    .unwrap();
                builder.set_certificate_chain_file(o.1.clone()).unwrap();
                self.server
                    .bind_ssl(format!("0.0.0.0:{}", self.port), builder)
                    .expect("Failed to bind socket")
                    .start();
            }
            None => {
                self.server
                    .bind(format!("0.0.0.0:{}", self.port))
                    .expect("Failed to bind socket")
                    .start();
            }
        }

        self.system.run();
    }

    /// Stop listening.
    pub fn stop(self) {
        drop(self.mdns.unwrap());
        self.server.system_exit();
    }
}
