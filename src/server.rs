/// Rust Web Thing server implementation.
use actix;
use actix::prelude::*;
use actix_service::{Service, Transform};
use actix_web;
use actix_web::dev::{Server, ServiceRequest, ServiceResponse};
use actix_web::guard;
use actix_web::{middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use futures::future::{ok, Either, Ready};
use hostname;
use libmdns;
#[cfg(feature = "ssl")]
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use serde_json;
use serde_json::json;
use std::marker::{Send, Sync};
use std::sync::{Arc, RwLock, Weak};
use std::task::{Context, Poll};
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

impl<S, B> Transform<S> for HostValidator
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = HostValidatorMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(HostValidatorMiddleware { service })
    }
}

struct HostValidatorMiddleware<S> {
    service: S,
}

impl<S, B> Service for HostValidatorMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = Either<S::Future, Ready<Result<Self::Response, Self::Error>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        let host = req.headers().get("Host");
        if host.is_none() {
            return Either::Right(ok(
                req.into_response(HttpResponse::Forbidden().finish().into_body())
            ));
        }

        let host = host.unwrap().to_str();
        if host.is_err() {
            return Either::Right(ok(
                req.into_response(HttpResponse::Forbidden().finish().into_body())
            ));
        }

        let host = host.unwrap();

        let state = req.app_data::<web::Data<AppState>>();
        if state.is_none() {
            return Either::Right(ok(
                req.into_response(HttpResponse::Forbidden().finish().into_body())
            ));
        }

        let state = state.unwrap();
        match state.validate_host(host.to_owned()) {
            Ok(_) => Either::Left(self.service.call(req)),
            Err(_) => Either::Right(ok(
                req.into_response(HttpResponse::Forbidden().finish().into_body())
            )),
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
    fn drain_queue(&self, ctx: &mut ws::WebsocketContext<Self>) {
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
    type Context = ws::WebsocketContext<Self>;
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ThingWebSocket {
    fn started(&mut self, ctx: &mut Self::Context) {
        self.drain_queue(ctx);
    }

    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => (),
            Ok(ws::Message::Text(text)) => {
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
            Ok(ws::Message::Close(_)) => {
                let thing = self.get_thing();
                thing.write().unwrap().remove_subscriber(self.get_id());
            }
            _ => (),
        }
    }
}

/// Handle a GET request to / when the server manages multiple things.
#[allow(non_snake_case)]
fn things_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
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

    if let ThingsType::Multiple(things, _) = state.things.as_ref() {
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
fn thing_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
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
async fn thing_handler_WS(
    req: HttpRequest,
    state: web::Data<AppState>,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    let thing_id = req.match_info().get("thing_id");

    match state.get_thing(thing_id) {
        None => Ok(HttpResponse::NotFound().finish()),
        Some(thing) => {
            let thing_id = match thing_id {
                None => 0,
                Some(id) => id.parse::<usize>().unwrap(),
            };
            let ws = ThingWebSocket {
                id: Uuid::new_v4().to_string(),
                thing_id: thing_id,
                things: state.get_things(),
                action_generator: state.get_action_generator(),
            };
            thing.write().unwrap().add_subscriber(ws.get_id());
            ws::start(ws, &req, stream)
        }
    }
}

/// Handle a GET request to /properties.
#[allow(non_snake_case)]
async fn properties_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
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
fn property_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
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
    if thing.has_property(&property_name.to_string()) {
        HttpResponse::Ok()
            .json(json!({property_name: thing.get_property(&property_name.to_string()).unwrap()}))
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a PUT request to /properties/<property>.
#[allow(non_snake_case)]
fn property_handler_PUT(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    let property_name = req.match_info().get("property_name");
    if property_name.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let property_name = property_name.unwrap();

    if !body.is_object() {
        return HttpResponse::BadRequest().finish();
    }

    let args = body.as_object().unwrap();

    if !args.contains_key(property_name) {
        return HttpResponse::BadRequest().finish();
    }

    let mut thing = thing.write().unwrap();
    if thing.has_property(&property_name.to_string()) {
        if thing
            .set_property(
                property_name.to_string(),
                args.get(property_name).unwrap().clone(),
            )
            .is_ok()
        {
            HttpResponse::Ok().json(
                json!({property_name: thing.get_property(&property_name.to_string()).unwrap()}),
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
fn actions_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_action_descriptions(None)),
    }
}

/// Handle a POST request to /actions.
#[allow(non_snake_case)]
fn actions_handler_POST(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    if !body.is_object() {
        return HttpResponse::BadRequest().finish();
    }

    let message = body.as_object().unwrap();

    let keys: Vec<&String> = message.keys().collect();
    if keys.len() != 1 {
        return HttpResponse::BadRequest().finish();
    }

    let action_name = keys[0];
    let action_params = message.get(action_name).unwrap();
    let input = action_params.get("input");

    let action = state.get_action_generator().generate(
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
                return HttpResponse::BadRequest().finish();
            }
        }

        let mut response: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
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

        HttpResponse::Created().json(response)
    } else {
        HttpResponse::BadRequest().finish()
    }
}

/// Handle a GET request to /actions/<action_name>.
#[allow(non_snake_case)]
fn action_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
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
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let thing = thing.unwrap();

    if !body.is_object() {
        return HttpResponse::BadRequest().finish();
    }

    let action_name = req.match_info().get("action_name");
    if action_name.is_none() {
        return HttpResponse::NotFound().finish();
    }

    let action_name = action_name.unwrap();

    let message = body.as_object().unwrap();

    let keys: Vec<&String> = message.keys().collect();
    if keys.len() != 1 {
        return HttpResponse::BadRequest().finish();
    }

    if keys[0] != action_name {
        return HttpResponse::BadRequest().finish();
    }

    let action_params = message.get(action_name).unwrap();
    let input = action_params.get("input");

    let action = state.get_action_generator().generate(
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
                return HttpResponse::BadRequest().finish();
            }
        }

        let mut response: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
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

        HttpResponse::Created().json(response)
    } else {
        HttpResponse::BadRequest().finish()
    }
}

/// Handle a GET request to /actions/<action_name>/<action_id>.
#[allow(non_snake_case)]
fn action_id_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
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
    req: HttpRequest,
    state: web::Data<AppState>,
    _body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
    if thing.is_none() {
        HttpResponse::NotFound().finish()
    } else {
        // TODO: this is not yet defined in the spec
        HttpResponse::Ok().finish()
    }
}

/// Handle a DELETE request to /actions/<action_name>/<action_id>.
#[allow(non_snake_case)]
fn action_id_handler_DELETE(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
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
fn events_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
    match thing {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_event_descriptions(None)),
    }
}

/// Handle a GET request to /events/<event_name>.
#[allow(non_snake_case)]
fn event_handler_GET(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = state.get_thing(req.match_info().get("thing_id"));
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

/// Server to represent a Web Thing over HTTP.
#[allow(dead_code)]
pub struct WebThingServer {
    things: ThingsType,
    base_path: String,
    port: Option<u16>,
    hostname: Option<String>,
    ssl_options: Option<(String, String)>,
    generator_arc: Arc<Box<dyn ActionGenerator>>,
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
    /// base_path -- base URL to use, rather than '/'
    pub fn new(
        things: ThingsType,
        port: Option<u16>,
        hostname: Option<String>,
        ssl_options: Option<(String, String)>,
        action_generator: Box<dyn ActionGenerator>,
        base_path: Option<String>,
    ) -> Self {
        Self {
            things,
            base_path: base_path
                .map(|p| p.trim_end_matches("/").to_string())
                .unwrap_or_else(|| "".to_owned()),
            port,
            hostname,
            ssl_options,
            generator_arc: Arc::new(action_generator),
        }
    }

    /// Start listening for incoming connections.
    pub fn start<F>(&mut self, configure: Option<Arc<F>>) -> Server
    where
        F: Fn(&mut web::ServiceConfig) + Send + Sync + 'static,
    {
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

        let bp = self.base_path.clone();
        let server = HttpServer::new(move || {
            let bp = bp.clone();
            let app = App::new()
                .data(AppState {
                    things: things_arc.clone(),
                    hosts: hosts_arc.clone(),
                    action_generator: generator_arc_clone.clone(),
                })
                .wrap(middleware::Logger::default())
                .wrap(HostValidator)
                .wrap(
                    middleware::DefaultHeaders::new()
                        .header("Access-Control-Allow-Origin", "*")
                        .header(
                            "Access-Control-Allow-Methods",
                            "GET, HEAD, PUT, POST, DELETE, OPTIONS",
                        )
                        .header(
                            "Access-Control-Allow-Headers",
                            "Origin, Content-Type, Accept, X-Requested-With",
                        ),
                );

            let app = if configure.is_some() {
                let configure = configure.clone().unwrap();
                unsafe { app.configure(&*Arc::into_raw(configure)) }
            } else {
                app
            };

            if single {
                let root = if bp == "" { "/".to_owned() } else { bp.clone() };

                app.service(
                    web::resource(&root)
                        .route(
                            web::route()
                                .guard(guard::Get())
                                .guard(guard::Header("upgrade", "websocket"))
                                .to(thing_handler_WS),
                        )
                        .route(web::get().to(thing_handler_GET)),
                )
                .service(
                    web::resource(&format!("{}/properties", bp))
                        .route(web::get().to(properties_handler_GET)),
                )
                .service(
                    web::resource(&format!("{}/properties/{{property_name}}", bp))
                        .route(web::get().to(property_handler_GET))
                        .route(web::put().to(property_handler_PUT)),
                )
                .service(
                    web::resource(&format!("{}/actions", bp))
                        .route(web::get().to(actions_handler_GET))
                        .route(web::post().to(actions_handler_POST)),
                )
                .service(
                    web::resource(&format!("{}/actions/{{action_name}}", bp))
                        .route(web::get().to(action_handler_GET))
                        .route(web::post().to(action_handler_POST)),
                )
                .service(
                    web::resource(&format!("{}/actions/{{action_name}}/{{action_id}}", bp))
                        .route(web::get().to(action_id_handler_GET))
                        .route(web::delete().to(action_id_handler_DELETE))
                        .route(web::put().to(action_id_handler_PUT)),
                )
                .service(
                    web::resource(&format!("{}/events", bp))
                        .route(web::get().to(events_handler_GET)),
                )
                .service(
                    web::resource(&format!("{}/events/{{event_name}}", bp))
                        .route(web::get().to(event_handler_GET)),
                )
            } else {
                app.service(web::resource("/").route(web::get().to(things_handler_GET)))
                    .service(
                        web::scope(&format!("{}/{{thing_id}}", bp))
                            .service(
                                web::resource("")
                                    .route(
                                        web::route()
                                            .guard(guard::Get())
                                            .guard(guard::Header("upgrade", "websocket"))
                                            .to(thing_handler_WS),
                                    )
                                    .route(web::get().to(thing_handler_GET)),
                            )
                            .service(
                                web::resource("/properties")
                                    .route(web::get().to(properties_handler_GET)),
                            )
                            .service(
                                web::resource("/properties/{property_name}")
                                    .route(web::get().to(property_handler_GET))
                                    .route(web::put().to(property_handler_PUT)),
                            )
                            .service(
                                web::resource("/actions")
                                    .route(web::get().to(actions_handler_GET))
                                    .route(web::post().to(actions_handler_POST)),
                            )
                            .service(
                                web::resource("/actions/{action_name}")
                                    .route(web::get().to(action_handler_GET))
                                    .route(web::post().to(action_handler_POST)),
                            )
                            .service(
                                web::resource("/actions/{action_name}/{action_id}")
                                    .route(web::get().to(action_id_handler_GET))
                                    .route(web::delete().to(action_id_handler_DELETE))
                                    .route(web::put().to(action_id_handler_PUT)),
                            )
                            .service(
                                web::resource("/events").route(web::get().to(events_handler_GET)),
                            )
                            .service(
                                web::resource("/events/{event_name}")
                                    .route(web::get().to(event_handler_GET)),
                            ),
                    )
            }
        });

        let responder = libmdns::Responder::new().unwrap();

        #[cfg(feature = "ssl")]
        match self.ssl_options {
            Some(ref o) => {
                responder.register(
                    "_webthing._tcp".to_owned(),
                    name.clone(),
                    port,
                    &["path=/", "tls=1"],
                );

                let mut builder = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
                builder
                    .set_private_key_file(o.0.clone(), SslFiletype::PEM)
                    .unwrap();
                builder.set_certificate_chain_file(o.1.clone()).unwrap();
                server
                    .bind_openssl(format!("0.0.0.0:{}", port), builder)
                    .expect("Failed to bind socket")
                    .run()
            }
            None => {
                responder.register("_webthing._tcp".to_owned(), name.clone(), port, &["path=/"]);
                server
                    .bind(format!("0.0.0.0:{}", port))
                    .expect("Failed to bind socket")
                    .run()
            }
        }

        #[cfg(not(feature = "ssl"))]
        {
            responder.register("_webthing._tcp".to_owned(), name.clone(), port, &["path=/"]);
            server
                .bind(format!("0.0.0.0:{}", port))
                .expect("Failed to bind socket")
                .run()
        }
    }
}
