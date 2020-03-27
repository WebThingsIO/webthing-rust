/// Rust Web Thing server implementation.
use actix;
use actix::prelude::*;
use actix_net::server::Server;
use actix_web;
use actix_web::error::{ErrorForbidden, ParseError};
use actix_web::http::header;
use actix_web::server::{HttpHandler, HttpHandlerTask};
use actix_web::HttpMessage;
use actix_web::{middleware, pred, server, ws, App, Error, HttpRequest, HttpResponse, Json};
use hostname;
use libmdns;
#[cfg(feature = "ssl")]
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use serde_json;
use std::marker::{Send, Sync};
use std::sync::{Arc, RwLock, Weak};
use std::time::Duration;
use uuid::Uuid;

use super::action::Action;
use super::thing::Thing;
use super::utils::get_addresses;

/// Represents the things managed by the server.
#[derive(Clone)]
pub enum ThingsType {
    /// Set when there are multiple things managed by the server
    Multiple(Vec<Arc<RwLock<Box<dyn Thing>>>>, String),
    /// Set when there is only one thing
    Single(Arc<RwLock<Box<dyn Thing>>>),
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
        thing: Weak<RwLock<Box<dyn Thing>>>,
        name: String,
        input: Option<&serde_json::Value>,
    ) -> Option<Box<dyn Action>>;
}

/// Shared app state, used by server threads.
pub struct AppState {
    things: Arc<ThingsType>,
    hosts: Arc<Vec<String>>,
    action_generator: Arc<Box<dyn ActionGenerator>>,
}

impl AppState {
    /// Get the thing this request is for.
    ///
    /// thing_id -- ID of the thing to get, in string form
    ///
    /// Returns the thing, or None if not found.
    fn get_thing(&self, thing_id: Option<&str>) -> Option<Arc<RwLock<Box<dyn Thing>>>> {
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

    fn get_action_generator(&self) -> Arc<Box<dyn ActionGenerator>> {
        self.action_generator.clone()
    }

    fn validate_host(&self, host: String) -> Result<(), ()> {
        if self.hosts.contains(&host.to_lowercase()) {
            Ok(())
        } else {
            Err(())
        }
    }
}

/// Host validation middleware
struct HostValidator;

impl middleware::Middleware<AppState> for HostValidator {
    fn start(&self, req: &HttpRequest<AppState>) -> actix_web::Result<middleware::Started> {
        let r = req.clone();

        let host = r
            .headers()
            .get("Host")
            .ok_or(ErrorForbidden(ParseError::Header))?
            .to_str()
            .map_err(ErrorForbidden)?;

        match r.state().validate_host(host.to_string()) {
            Ok(_) => Ok(middleware::Started::Done),
            Err(_) => Err(ErrorForbidden(ParseError::Header)),
        }
    }
}

/// Shared state used by individual websockets.
struct ThingWebSocket {
    id: String,
    thing_id: usize,
    things: Arc<ThingsType>,
    action_generator: Arc<Box<dyn ActionGenerator>>,
}

impl ThingWebSocket {
    /// Get the ID of this websocket.
    fn get_id(&self) -> String {
        self.id.clone()
    }

    /// Get the thing associated with this websocket.
    fn get_thing(&self) -> Arc<RwLock<Box<dyn Thing>>> {
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
                    "setProperty" => {
                        for (property_name, property_value) in data.iter() {
                            let result = thing
                                .write()
                                .unwrap()
                                .set_property(property_name.to_string(), property_value.clone());

                            if result.is_err() {
                                let err = result.unwrap_err();
                                return ctx.text(format!(
                                    r#"
                                    {{
                                        "messageType": "error",
                                        "data": {{
                                            "status": "400 Bad Request",
                                            "message": "{}"
                                        }}
                                    }}"#,
                                    err
                                ));
                            }
                        }
                    }
                    "requestAction" => {
                        for (action_name, action_params) in data.iter() {
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
                        }
                    }
                    "addEventSubscription" => {
                        for event_name in data.keys() {
                            thing
                                .write()
                                .unwrap()
                                .add_event_subscriber(event_name.to_string(), self.get_id());
                        }
                    }
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
fn things_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
    let mut response: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

    // The host header is already checked by HostValidator, so the unwrapping is safe here.
    let host = req.headers().get("Host").unwrap().to_str().unwrap();
    let connection = req.connection_info();
    let scheme = connection.scheme();
    let ws_href = format!(
        "{}://{}",
        if scheme == "https" { "wss" } else { "ws" },
        host
    );

    if let ThingsType::Multiple(things, _) = req.state().things.as_ref() {
        for thing in things.iter() {
            let thing = thing.read().unwrap();

            let mut link = serde_json::Map::new();
            link.insert("rel".to_owned(), json!("alternate"));
            link.insert(
                "href".to_owned(),
                json!(format!("{}{}", ws_href, thing.get_href())),
            );

            let mut description = thing.as_thing_description().clone();
            {
                let links = description
                    .get_mut("links")
                    .unwrap()
                    .as_array_mut()
                    .unwrap();
                links.push(json!(link));
            }

            description.insert("href".to_owned(), json!(thing.get_href()));
            description.insert(
                "base".to_owned(),
                json!(format!("{}://{}{}", scheme, host, thing.get_href())),
            );
            description.insert(
                "securityDefinitions".to_owned(),
                json!({"nosec_sc": {"scheme": "nosec"}}),
            );
            description.insert("security".to_owned(), json!("nosec_sc"));

            response.push(description);
        }
    }
    HttpResponse::Ok().json(response)
}

/// Handle a GET request to /.
#[allow(non_snake_case)]
fn thing_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => {
            let thing = thing.read().unwrap();

            // The host header is already checked by HostValidator, so the unwrapping is safe here.
            let host = req.headers().get("Host").unwrap().to_str().unwrap();
            let connection = req.connection_info();
            let scheme = connection.scheme();
            let ws_href = format!(
                "{}://{}{}",
                if scheme == "https" { "wss" } else { "ws" },
                host,
                thing.get_href()
            );

            let mut link = serde_json::Map::new();
            link.insert("rel".to_owned(), json!("alternate"));
            link.insert("href".to_owned(), json!(ws_href));

            let mut description = thing.as_thing_description().clone();
            {
                let links = description
                    .get_mut("links")
                    .unwrap()
                    .as_array_mut()
                    .unwrap();
                links.push(json!(link));
            }

            description.insert(
                "base".to_owned(),
                json!(format!("{}://{}{}", scheme, host, thing.get_href())),
            );
            description.insert(
                "securityDefinitions".to_owned(),
                json!({"nosec_sc": {"scheme": "nosec"}}),
            );
            description.insert("security".to_owned(), json!("nosec_sc"));

            HttpResponse::Ok().json(description)
        }
    }
}

/// Handle websocket on /.
#[allow(non_snake_case)]
fn thing_handler_WS(req: &HttpRequest<AppState>) -> Result<HttpResponse, Error> {
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
            ws::start(&req, ws)
        }
    }
}

/// Handle a GET request to /properties.
#[allow(non_snake_case)]
fn properties_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    match thing {
        Some(thing) => {
            let thing = thing.read().unwrap();
            HttpResponse::Ok().json(json!(thing.get_properties()))
        }
        None => HttpResponse::NotFound().finish(),
    }
}

/// Handle a GET request to /properties/<property>.
#[allow(non_snake_case)]
fn property_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
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
    (req, message): (HttpRequest<AppState>, Json<serde_json::Value>),
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
            HttpResponse::BadRequest().finish()
        }
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a GET request to /actions.
#[allow(non_snake_case)]
fn actions_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_action_descriptions(None)),
    }
}

/// Handle a POST request to /actions.
#[allow(non_snake_case)]
fn actions_handler_POST(
    (req, message): (HttpRequest<AppState>, Json<serde_json::Value>),
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
fn action_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    let action_name = req.match_info().get("action_name");
    if action_name.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.read().unwrap();
    HttpResponse::Ok().json(thing.get_action_descriptions(Some(action_name.unwrap().to_string())))
}

/// Handle a POST request to /actions/<action_name>.
#[allow(non_snake_case)]
fn action_handler_POST(
    (req, message): (HttpRequest<AppState>, Json<serde_json::Value>),
) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    if !message.is_object() {
        return HttpResponse::BadRequest().finish();
    }

    let action_name = req.match_info().get("action_name");
    if action_name.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let action_name = action_name.unwrap();

    let message = message.as_object().unwrap();

    let mut response: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    for (name, action_params) in message.iter() {
        if name != action_name {
            continue;
        }

        let input = action_params.get("input");

        let action = req.state().get_action_generator().generate(
            Arc::downgrade(&thing.clone()),
            name.to_string(),
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
                name.to_string(),
                action
                    .read()
                    .unwrap()
                    .as_action_description()
                    .get(name)
                    .unwrap()
                    .clone(),
            );

            thing.write().unwrap().start_action(name.to_string(), id);
        }
    }

    HttpResponse::Created().json(response)
}

/// Handle a GET request to /actions/<action_name>/<action_id>.
#[allow(non_snake_case)]
fn action_id_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
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
    (req, _message): (HttpRequest<AppState>, Json<serde_json::Value>),
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
fn action_id_handler_DELETE(req: &HttpRequest<AppState>) -> HttpResponse {
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
fn events_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_event_descriptions(None)),
    }
}

/// Handle a GET request to /events/<event_name>.
#[allow(non_snake_case)]
fn event_handler_GET(req: &HttpRequest<AppState>) -> HttpResponse {
    let thing = req.state().get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    let event_name = req.match_info().get("event_name");
    if event_name.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.read().unwrap();
    HttpResponse::Ok().json(thing.get_event_descriptions(Some(event_name.unwrap().to_string())))
}

fn build_app(
    things: &Arc<ThingsType>,
    hosts: &Arc<Vec<String>>,
    action_generator: &Arc<Box<dyn ActionGenerator>>,
) -> App<AppState> {
    App::with_state(AppState {
        things: things.clone(),
        hosts: hosts.clone(),
        action_generator: action_generator.clone(),
    })
    .middleware(middleware::Logger::default())
    .middleware(HostValidator)
    .middleware(
        middleware::cors::Cors::build()
            .send_wildcard()
            .allowed_methods(vec!["GET", "HEAD", "PUT", "POST", "DELETE"])
            .allowed_headers(vec![
                header::ORIGIN,
                header::CONTENT_TYPE,
                header::ACCEPT,
                header::HeaderName::from_lowercase(b"x-requested-with").unwrap(),
            ])
            .finish(),
    )
}

fn build_server(
    app: App<AppState>,
    single: bool,
    base_path: String,
) -> Box<(dyn HttpHandler<Task = Box<(dyn HttpHandlerTask + 'static)>> + 'static)> {
    if single {
        let root = if base_path == "" {
            "/".to_owned()
        } else {
            base_path.clone()
        };

        app.resource(&root, |r| {
            r.route()
                .filter(pred::Get())
                .filter(pred::Header("upgrade", "websocket"))
                .f(thing_handler_WS);
            r.get().f(thing_handler_GET)
        })
        .resource(&format!("{}/properties", base_path), |r| {
            r.get().f(properties_handler_GET)
        })
        .resource(
            &format!("{}/properties/{{property_name}}", base_path),
            |r| {
                r.get().f(property_handler_GET);
                r.put().with(property_handler_PUT);
            },
        )
        .resource(&format!("{}/actions", base_path), |r| {
            r.get().f(actions_handler_GET);
            r.post().with(actions_handler_POST);
        })
        .resource(&format!("{}/actions/{{action_name}}", base_path), |r| {
            r.get().f(action_handler_GET);
            r.post().with(action_handler_POST);
        })
        .resource(
            &format!("{}/actions/{{action_name}}/{{action_id}}", base_path),
            |r| {
                r.get().f(action_id_handler_GET);
                r.delete().f(action_id_handler_DELETE);
                r.put().with(action_id_handler_PUT);
            },
        )
        .resource(&format!("{}/events", base_path), |r| {
            r.get().f(events_handler_GET)
        })
        .resource(&format!("{}/events/{{event_name}}", base_path), |r| {
            r.get().f(event_handler_GET)
        })
        .boxed()
    } else {
        app.resource("/", |r| r.get().f(things_handler_GET))
            .scope(&format!("{}/{{thing_id}}", base_path), |scope| {
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
                        r.put().with(property_handler_PUT);
                    })
                    .resource("/actions", |r| {
                        r.get().f(actions_handler_GET);
                        r.post().with(actions_handler_POST);
                    })
                    .resource("/actions/{action_name}", |r| {
                        r.get().f(action_handler_GET);
                        r.post().with(action_handler_POST);
                    })
                    .resource("/actions/{action_name}/{action_id}", |r| {
                        r.get().f(action_id_handler_GET);
                        r.delete().f(action_id_handler_DELETE);
                        r.put().with(action_id_handler_PUT);
                    })
                    .resource("/events", |r| r.get().f(events_handler_GET))
                    .resource("/events/{event_name}", |r| r.get().f(event_handler_GET))
            })
            .boxed()
    }
}

/// Server to represent a Web Thing over HTTP.
#[allow(dead_code)]
pub struct WebThingServer {
    things: ThingsType,
    base_path: String,
    port: Option<u16>,
    hostname: Option<String>,
    ssl_options: Option<(String, String)>,
    generator_arc: Arc<Box<dyn ActionGenerator>>,
    router_arc: Option<Arc<Box<dyn Fn(App<AppState>) -> App<AppState> + Send + Sync>>>,
    system: actix::SystemRunner,
}

impl WebThingServer {
    /// Create a new WebThingServer.
    ///
    /// things -- list of Things managed by this server
    /// name -- name of this device -- this is only needed if the server is
    ///         managing multiple things
    /// port -- port to listen on (defaults to 80)
    /// hostname -- optional host name, i.e. mything.com
    /// ssl_options -- tuple of SSL options to pass to the actix web server
    /// action_generator -- action generator struct
    /// router -- additional router to add to server
    /// base_path -- base URL to use, rather than '/'
    pub fn new(
        things: ThingsType,
        port: Option<u16>,
        hostname: Option<String>,
        ssl_options: Option<(String, String)>,
        action_generator: Box<dyn ActionGenerator>,
        router: Option<Box<dyn Fn(App<AppState>) -> App<AppState> + Send + Sync>>,
        base_path: Option<String>,
    ) -> WebThingServer {
        let sys = actix::System::new("webthing");
        let generator_arc = Arc::new(action_generator);
        let router = match router {
            Some(r) => Some(Arc::new(r)),
            None => None,
        };

        let base_path = match base_path {
            Some(p) => p.trim_end_matches("/").to_string(),
            None => "".to_owned(),
        };

        WebThingServer {
            things: things,
            base_path: base_path,
            port: port,
            hostname: hostname,
            ssl_options: ssl_options,
            generator_arc: generator_arc,
            router_arc: router,
            system: sys,
        }
    }

    /// Start listening for incoming connections.
    pub fn create(&mut self) -> actix::Addr<Server> {
        let port = match self.port {
            Some(p) => p,
            None => 80,
        };

        let mut hosts = vec!["localhost".to_owned(), format!("localhost:{}", port)];

        let system_hostname = hostname::get();
        if system_hostname.is_ok() {
            let name = system_hostname
                .unwrap()
                .into_string()
                .unwrap()
                .to_lowercase();
            hosts.push(format!("{}.local", name));
            hosts.push(format!("{}.local:{}", name, port));
        }

        for address in get_addresses() {
            hosts.push(address.clone());
            hosts.push(format!("{}:{}", address, port));
        }

        if self.hostname.is_some() {
            let name = self.hostname.clone().unwrap().to_lowercase();
            hosts.push(name.clone());
            hosts.push(format!("{}:{}", name, port));
        }

        let name = match &self.things {
            ThingsType::Single(thing) => thing.read().unwrap().get_title(),
            ThingsType::Multiple(_, name) => name.to_owned(),
        };

        match &mut self.things {
            ThingsType::Multiple(ref mut things, _) => {
                for (idx, thing) in things.iter_mut().enumerate() {
                    let mut thing = thing.write().unwrap();
                    thing.set_href_prefix(format!("{}/{}", self.base_path, idx));
                }
            }
            ThingsType::Single(ref mut thing) => {
                thing
                    .write()
                    .unwrap()
                    .set_href_prefix(self.base_path.clone());
            }
        }

        let single = match &self.things {
            ThingsType::Multiple(_, _) => false,
            ThingsType::Single(_) => true,
        };
        let things_arc = Arc::new(self.things.clone());
        let hosts_arc = Arc::new(hosts.clone());
        let generator_arc_clone = self.generator_arc.clone();
        let router = match self.router_arc {
            Some(ref r) => Some(r.clone()),
            None => None,
        };

        let bp = self.base_path.clone();
        let server = server::new(move || {
            let bp = bp.clone();
            let app = build_app(&things_arc, &hosts_arc, &generator_arc_clone);
            match router {
                Some(ref r) => {
                    let app = r(app);
                    build_server(app, single, bp)
                }
                None => build_server(app, single, bp),
            }
        });

        let responder = libmdns::Responder::new().unwrap();
        responder.register("_webthing._tcp".to_owned(), name.clone(), port, &["path=/"]);

        #[cfg(feature = "ssl")]
        match self.ssl_options {
            Some(ref o) => {
                let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
                builder
                    .set_private_key_file(o.0.clone(), SslFiletype::PEM)
                    .unwrap();
                builder.set_certificate_chain_file(o.1.clone()).unwrap();
                server
                    .bind_ssl(format!("0.0.0.0:{}", port), builder)
                    .expect("Failed to bind socket")
                    .start()
            }
            None => server
                .bind(format!("0.0.0.0:{}", port))
                .expect("Failed to bind socket")
                .start(),
        }

        #[cfg(not(feature = "ssl"))]
        {
            server
                .bind(format!("0.0.0.0:{}", port))
                .expect("Failed to bind socket")
                .start()
        }
    }

    /// Start the system and run the server. This is a blocking method call.
    pub fn start(self) {
        self.system.run();
    }
}
