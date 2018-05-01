/// Rust Web Thing server implementation.
use actix;
use actix::prelude::*;
use actix_web::server::{HttpHandler, HttpServer};
use actix_web::{middleware, pred, server, ws, App, Error, HttpRequest, HttpResponse, Json};
use mdns;
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use serde_json;
use std::sync::{Arc, RwLock};

use super::thing::Thing;
use super::utils::get_ip;

struct AppState {
    things: Arc<Vec<RwLock<Box<Thing>>>>,
}

impl AppState {
    /// Get the thing this request is for.
    ///
    /// thing_id -- ID of the thing to get, in string form
    ///
    /// Returns the thing, or None if not found.
    fn get_thing(&self, thing_id: Option<&str>) -> Option<&RwLock<Box<Thing>>> {
        if self.things.len() > 1 {
            if thing_id.is_none() {
                return None;
            }

            let id = thing_id.unwrap().parse::<usize>();

            if id.is_err() {
                return None;
            }

            let id = id.unwrap();
            if id >= self.things.len() {
                None
            } else {
                Some(&self.things[id])
            }
        } else {
            Some(&self.things[0])
        }
    }
}

struct ThingWebSocket;

impl Actor for ThingWebSocket {
    type Context = ws::WebsocketContext<Self, AppState>;
}

impl StreamHandler<ws::Message, ws::ProtocolError> for ThingWebSocket {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Pong(_) => (),
            ws::Message::Text(text) => {
                /* TODO
                try:
                    message = json.loads(message)
                except ValueError:
                    try:
                        self.write_message(json.dumps({
                            'messageType': 'error',
                            'data': {
                                'status': '400 Bad Request',
                                'message': 'Parsing request failed',
                            },
                        }))
                    except tornado.websocket.WebSocketClosedError:
                        pass

                    return

                if 'messageType' not in message or 'data' not in message:
                    try:
                        self.write_message(json.dumps({
                            'messageType': 'error',
                            'data': {
                                'status': '400 Bad Request',
                                'message': 'Invalid message',
                            },
                        }))
                    except tornado.websocket.WebSocketClosedError:
                        pass

                    return

                msg_type = message['messageType']
                if msg_type == 'setProperty':
                    for property_name, property_value in message['data'].items():
                        try:
                            self.thing.set_property(property_name, property_value)
                        except AttributeError:
                            self.write_message(json.dumps({
                                'messageType': 'error',
                                'data': {
                                    'status': '403 Forbidden',
                                    'message': 'Read-only property',
                                },
                            }))
                elif msg_type == 'requestAction':
                    for action_name, action_params in message['data'].items():
                        input_ = None
                        if 'input' in action_params:
                            input_ = action_params['input']

                        action = self.thing.perform_action(action_name, input_)
                        if action:
                            tornado.ioloop.IOLoop.current().spawn_callback(
                                perform_action,
                                action,
                            )
                        else:
                            self.write_message(json.dumps({
                                'messageType': 'error',
                                'data': {
                                    'status': '400 Bad Request',
                                    'message': 'Invalid action request',
                                    'request': message,
                                },
                            }))
                elif msg_type == 'addEventSubscription':
                    for event_name in message['data'].keys():
                        self.thing.add_event_subscriber(event_name, self)
                else:
                    try:
                        self.write_message(json.dumps({
                            'messageType': 'error',
                            'data': {
                                'status': '400 Bad Request',
                                'message': 'Unknown messageType: ' + msg_type,
                                'request': message,
                            },
                        }))
                    except tornado.websocket.WebSocketClosedError:
                        pass
                */
            }
            ws::Message::Binary(_) => (),
            ws::Message::Close(_) => {
                /* TODO
                self.thing.remove_subscriber(self)
                */
            }
        }
    }
}

/// Handle a GET request to / when the server manages multiple things.
#[allow(non_snake_case)]
fn things_handler_GET(req: HttpRequest<AppState>) -> HttpResponse {
    let mut response: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();
    for thing in req.state().things.iter() {
        response.push(thing.read().unwrap().as_thing_description());
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
    /* TODO
    self.thing.add_subscriber(self)
    */
    ws::start(req, ThingWebSocket)
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
        let action = thing
            .write()
            .unwrap()
            .perform_action(action_name.to_string(), input);
        if action.is_some() {
            let mut action = action.unwrap();
            let description = action.as_action_description();
            response.insert(
                action_name.to_string(),
                description.get(action_name).unwrap().clone(),
            );

            // Start the action
            // TODO: do this in the background
            action.start();
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
        HttpResponse::Ok().json(action.unwrap().as_action_description())
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
    mdns: Option<mdns::Service>,
    system: actix::SystemRunner,
}

impl WebThingServer {
    /// Initialize the WebThingServer.
    ///
    /// things -- list of Things managed by this server
    /// name -- name of this device -- this is only needed if the server is
    ///         managing multiple things
    /// port -- port to listen on (defaults to 80)
    /// ssl_options -- dict of SSL options to pass to the tornado server
    pub fn new(
        mut things: Vec<RwLock<Box<Thing>>>,
        name: Option<String>,
        port: Option<u16>,
        ssl_options: Option<(String, String)>,
    ) -> WebThingServer {
        if things.len() > 1 && name.is_none() {
            panic!("name must be set when managing multiple things");
        }

        let ip = get_ip();

        let port = match port {
            Some(p) => p,
            None => 80,
        };

        let name = if things.len() == 1 {
            things[0].read().unwrap().get_name()
        } else {
            name.unwrap()
        };

        let ws_protocol = match ssl_options {
            Some(_) => "wss",
            None => "ws",
        };

        if things.len() > 1 {
            for (idx, thing) in things.iter_mut().enumerate() {
                let mut thing = thing.write().unwrap();
                thing.set_href_prefix(format!("/{}", idx));
                thing.set_ws_href(format!("{}://{}:{}/{}", ws_protocol, ip, port, idx));
            }
        } else {
            things[0]
                .write()
                .unwrap()
                .set_ws_href(format!("{}://{}:{}", ws_protocol, ip, port));
        }

        let things_arc = Arc::new(things);

        let server = if things_arc.len() > 1 {
            let inner_arc = things_arc.clone();
            server::new(move || {
                vec![
                    App::with_state(AppState {
                        things: inner_arc.clone(),
                    }).middleware(middleware::Logger::default())
                        .resource("/", |r| r.get().f(things_handler_GET))
                        .resource("/{thing_id}", |r| {
                            r.route()
                                .filter(pred::Get())
                                .filter(pred::Header("upgrade", "websocket"))
                                .f(thing_handler_WS);
                            r.get().f(thing_handler_GET)
                        })
                        .resource("/{thing_id}/properties", |r| {
                            r.get().f(properties_handler_GET)
                        })
                        .resource("/{thing_id}/properties/{property_name}", |r| {
                            r.get().f(property_handler_GET);
                            r.put().with2(property_handler_PUT);
                        })
                        .resource("/{thing_id}/actions", |r| {
                            r.get().f(actions_handler_GET);
                            r.post().with2(actions_handler_POST);
                        })
                        .resource("/{thing_id}/actions/{action_name}", |r| {
                            r.get().f(action_handler_GET)
                        })
                        .resource("/{thing_id}/actions/{action_name}/{action_id}", |r| {
                            r.get().f(action_id_handler_GET);
                            r.delete().f(action_id_handler_DELETE);
                            r.put().with2(action_id_handler_PUT);
                        })
                        .resource("/{thing_id}/events", |r| r.get().f(events_handler_GET))
                        .resource("/{thing_id}/events/{event_name}", |r| {
                            r.get().f(event_handler_GET)
                        })
                        .boxed(),
                ]
            })
        } else {
            let inner_arc = things_arc.clone();
            server::new(move || {
                vec![
                    App::with_state(AppState {
                        things: inner_arc.clone(),
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
        };

        let sys = actix::System::new("webthing");

        WebThingServer {
            ip: ip,
            port: port,
            name: name,
            ssl_options: ssl_options,
            server: server
                .bind(format!("0.0.0.0:{}", port))
                .expect("Failed to bind socket"),
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

        let responder = mdns::Responder::new().unwrap();
        let svc = responder.register(
            "_webthing._sub._http._tcp".to_owned(),
            self.name.clone(),
            self.port,
            &[&format!("url={}://{}:{}", protocol, self.ip, self.port)],
        );
        self.mdns = Some(svc);

        match self.ssl_options {
            Some(ref o) => {
                let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
                builder
                    .set_private_key_file(o.0.clone(), SslFiletype::PEM)
                    .unwrap();
                builder.set_certificate_chain_file(o.1.clone()).unwrap();
                self.server.start_ssl(builder).unwrap();
            }
            None => {
                self.server.start();
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
