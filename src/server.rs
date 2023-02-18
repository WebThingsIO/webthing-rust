/// Rust Web Thing server implementation.
use actix;
use actix::prelude::*;
use actix_web;
use actix_web::body::EitherBody;
use actix_web::dev::{Server, ServiceRequest, ServiceResponse};
use actix_web::dev::{Service, Transform};
use actix_web::guard;
use actix_web::http::header::HeaderValue;
use actix_web::web::Data;
use actix_web::{middleware, web, App, Error, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use futures::future::{ok, LocalBoxFuture, Ready};
use hostname;
use libmdns;
#[cfg(feature = "ssl")]
use openssl::ssl::{SslAcceptor, SslFiletype, SslMethod};
use serde_json;
use serde_json::json;
use std::marker::{Send, Sync};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use std::time::Duration;
use uuid::Uuid;

pub use super::action_generator::ActionGenerator;
use super::thing::Thing;
use super::utils::get_addresses;

const SERVICE_TYPE: &str = "_webthing._tcp";

/// Represents the things managed by the server.
#[derive(Clone)]
pub enum ThingsType {
    /// Set when there are multiple things managed by the server
    Multiple(Vec<Arc<RwLock<Box<dyn Thing>>>>, String),
    /// Set when there is only one thing
    Single(Arc<RwLock<Box<dyn Thing>>>),
}

/// Shared app state, used by server threads.
struct AppState {
    things: Arc<ThingsType>,
    hosts: Arc<Vec<String>>,
    disable_host_validation: Arc<bool>,
    action_generator: Arc<dyn ActionGenerator>,
}

impl AppState {
    /// Get the thing this request is for.
    fn get_thing(&self, thing_id: Option<&str>) -> Option<Arc<RwLock<Box<dyn Thing>>>> {
        match self.things.as_ref() {
            ThingsType::Multiple(ref inner_things, _) => {
                let id = thing_id?.parse::<usize>().ok()?;
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

    fn get_action_generator(&self) -> Arc<dyn ActionGenerator> {
        self.action_generator.clone()
    }

    fn validate_host(&self, host: Option<&HeaderValue>) -> Result<(), ()> {
        if *self.disable_host_validation {
            return Ok(());
        }

        if let Some(Ok(host)) = host.map(|h| h.to_str()) {
            if self.hosts.contains(&host.to_lowercase()) {
                return Ok(());
            }
        }

        Err(())
    }
}

/// Host validation middleware
struct HostValidator;

impl<S, B> Transform<S, ServiceRequest> for HostValidator
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type InitError = ();
    type Transform = HostValidatorMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(HostValidatorMiddleware { service })
    }
}

struct HostValidatorMiddleware<S: Service<ServiceRequest>> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for HostValidatorMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if let Some(state) = req.app_data::<web::Data<AppState>>() {
            let host = req.headers().get("Host");
            match state.validate_host(host) {
                Ok(_) => {
                    let res = self.service.call(req);
                    Box::pin(async move { res.await.map(ServiceResponse::map_into_left_body) })
                }
                Err(_) => Box::pin(async {
                    Ok(req.into_response(HttpResponse::Forbidden().finish().map_into_right_body()))
                }),
            }
        } else {
            Box::pin(async {
                Ok(req.into_response(HttpResponse::Forbidden().finish().map_into_right_body()))
            })
        }
    }
}

/// Shared state used by individual websockets.
struct ThingWebSocket {
    id: String,
    thing_id: usize,
    things: Arc<ThingsType>,
    action_generator: Arc<dyn ActionGenerator>,
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

fn bad_request(message: impl AsRef<str>, request: Option<serde_json::Value>) -> serde_json::Value {
    if let Some(request) = request {
        json!({
              "messageType": "error",
              "data": {
                  "status": "400 Bad Request",
                  "message": message.as_ref(),
                  "request": request,
              }
        })
    } else {
        json!({
            "messageType": "error",
            "data": {
                "status": "400 Bad Request",
                "message": message.as_ref(),
            }
        })
    }
}

fn bad_request_string(message: impl AsRef<str>, request: Option<serde_json::Value>) -> String {
    serde_json::to_string(&bad_request(message, request)).unwrap()
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
                let message: serde_json::Value = if let Ok(message) = serde_json::from_str(&text) {
                    message
                } else {
                    return ctx.text(bad_request_string("Parsing request failed", None));
                };

                let message = if let Some(object) = message.as_object() {
                    object
                } else {
                    return ctx.text(bad_request_string("Parsing request failed", Some(message)));
                };

                if !message.contains_key("messageType") || !message.contains_key("data") {
                    return ctx.text(bad_request_string("Invalid message", Some(json!(message))));
                }

                let msg_type = message.get("messageType").unwrap().as_str();
                let data = message.get("data").unwrap().as_object();
                if msg_type.is_none() || data.is_none() {
                    return ctx.text(bad_request_string("Invalid message", Some(json!(message))));
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

                            if let Err(err) = result {
                                return ctx.text(bad_request_string(err, Some(json!(message))));
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
                                return ctx.text(bad_request_string(
                                    "Invalid action request",
                                    Some(json!(message)),
                                ));
                            }

                            let action = action.unwrap();
                            let id = action.get_id();
                            let action = Arc::new(RwLock::new(action));

                            {
                                let mut thing = thing.write().unwrap();
                                if let Err(err) = thing.add_action(action.clone(), input) {
                                    return ctx.text(bad_request_string(
                                        format!("Failed to start action: {}", err),
                                        Some(json!(message)),
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
                        return ctx.text(bad_request_string(
                            format!("Unknown messageType: {}", unknown),
                            Some(json!(message)),
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
async fn handle_get_things(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
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

            let mut form = serde_json::Map::new();
            form.insert(
                "href".to_owned(),
                json!(format!("{}{}", ws_href, thing.get_href())),
            );

            let mut description = thing.as_thing_description().clone();
            {
                let forms = description
                    .get_mut("forms")
                    .unwrap()
                    .as_array_mut()
                    .unwrap();
                forms.push(json!(form));
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
async fn handle_get_thing(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
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

            let mut form = serde_json::Map::new();
            form.insert("href".to_owned(), json!(ws_href));

            let mut description = thing.as_thing_description();
            {
                let forms = description
                    .get_mut("forms")
                    .unwrap()
                    .as_array_mut()
                    .unwrap();
                forms.push(json!(form));
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
async fn handle_ws_thing(
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
                thing_id,
                things: state.get_things(),
                action_generator: state.get_action_generator(),
            };
            thing.write().unwrap().add_subscriber(ws.get_id());
            ws::start(ws, &req, stream)
        }
    }
}

/// Handle a GET request to /properties.
async fn handle_get_properties(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    if let Some(thing) = state.get_thing(req.match_info().get("thing_id")) {
        let thing = thing.read().unwrap();
        HttpResponse::Ok().json(json!(thing.get_properties()))
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a GET request to /properties/<property>.
async fn handle_get_property(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = match state.get_thing(req.match_info().get("thing_id")) {
        Some(thing) => thing,
        None => return HttpResponse::NotFound().finish(),
    };

    let property_name = match req.match_info().get("property_name") {
        Some(property_name) => property_name,
        None => return HttpResponse::NotFound().finish(),
    };

    let thing = thing.read().unwrap();
    if let Some(property) = thing.get_property(property_name) {
        HttpResponse::Ok().json(json!(property))
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a PUT request to /properties/<property>.
async fn handle_put_property(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let thing = match state.get_thing(req.match_info().get("thing_id")) {
        Some(thing) => thing,
        None => return HttpResponse::NotFound().finish(),
    };

    let property_name = match req.match_info().get("property_name") {
        Some(property_name) => property_name,
        None => return HttpResponse::NotFound().finish(),
    };

    let args = body.into_inner();

    let mut thing = thing.write().unwrap();
    if thing.has_property(property_name) {
        let set_property_result = thing.set_property(property_name.to_string(), args.clone());

        match set_property_result {
            Ok(()) => HttpResponse::Ok().json(json!(thing.get_property(property_name).unwrap())),
            Err(err) => HttpResponse::BadRequest().json(bad_request(err, Some(json!(args)))),
        }
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a GET request to /actions.
async fn handle_get_actions(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    match state.get_thing(req.match_info().get("thing_id")) {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_action_descriptions(None)),
    }
}

/// Handle a POST request to /actions.
async fn handle_post_actions(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let thing = match state.get_thing(req.match_info().get("thing_id")) {
        Some(thing) => thing,
        None => return HttpResponse::NotFound().finish(),
    };

    let message = match body.as_object() {
        Some(message) => message,
        None => return HttpResponse::BadRequest().finish(),
    };

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

    if let Some(action) = action {
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
async fn handle_get_action(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    if let Some(thing) = state.get_thing(req.match_info().get("thing_id")) {
        if let Some(action_name) = req.match_info().get("action_name") {
            let thing = thing.read().unwrap();
            return HttpResponse::Ok()
                .json(thing.get_action_descriptions(Some(action_name.to_string())));
        }
    }

    HttpResponse::NotFound().finish()
}

/// Handle a POST request to /actions/<action_name>.
async fn handle_post_action(
    req: HttpRequest,
    state: web::Data<AppState>,
    body: web::Json<serde_json::Value>,
) -> HttpResponse {
    let thing = if let Some(thing) = state.get_thing(req.match_info().get("thing_id")) {
        thing
    } else {
        return HttpResponse::NotFound().finish();
    };

    let action_name = if let Some(action_name) = req.match_info().get("action_name") {
        action_name
    } else {
        return HttpResponse::NotFound().finish();
    };

    let message = if let Some(message) = body.as_object() {
        message
    } else {
        return HttpResponse::BadRequest().finish();
    };

    if message.keys().count() != 1 {
        return HttpResponse::BadRequest().finish();
    }

    let input = if let Some(action_params) = message.get(action_name) {
        action_params.get("input")
    } else {
        return HttpResponse::BadRequest().finish();
    };

    let action = state.get_action_generator().generate(
        Arc::downgrade(&thing.clone()),
        action_name.to_string(),
        input,
    );

    if let Some(action) = action {
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
async fn handle_get_action_id(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = if let Some(thing) = state.get_thing(req.match_info().get("thing_id")) {
        thing
    } else {
        return HttpResponse::NotFound().finish();
    };

    let action_name = req.match_info().get("action_name");
    let action_id = req.match_info().get("action_id");
    let (action_name, action_id) = if let Some(action) = action_name.zip(action_id) {
        action
    } else {
        return HttpResponse::NotFound().finish();
    };

    let thing = thing.read().unwrap();
    if let Some(action) = thing.get_action(action_name.to_string(), action_id.to_string()) {
        HttpResponse::Ok().json(action.read().unwrap().as_action_description())
    } else {
        HttpResponse::NotFound().finish()
    }
}

/// Handle a PUT request to /actions/<action_name>/<action_id>.
async fn handle_put_action_id(
    req: HttpRequest,
    state: web::Data<AppState>,
    _body: web::Json<serde_json::Value>,
) -> HttpResponse {
    match state.get_thing(req.match_info().get("thing_id")) {
        Some(_) => {
            // TODO: this is not yet defined in the spec
            HttpResponse::Ok().finish()
        }
        None => HttpResponse::NotFound().finish(),
    }
}

/// Handle a DELETE request to /actions/<action_name>/<action_id>.
async fn handle_delete_action_id(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = match state.get_thing(req.match_info().get("thing_id")) {
        Some(thing) => thing,
        None => return HttpResponse::NotFound().finish(),
    };

    let action_name = req.match_info().get("action_name");
    let action_id = req.match_info().get("action_id");
    if let Some((action_name, action_id)) = action_name.zip(action_id) {
        if thing
            .write()
            .unwrap()
            .remove_action(action_name.to_string(), action_id.to_string())
        {
            return HttpResponse::NoContent().finish();
        }
    }

    HttpResponse::NotFound().finish()
}

/// Handle a GET request to /events.
async fn handle_get_events(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    match state.get_thing(req.match_info().get("thing_id")) {
        None => HttpResponse::NotFound().finish(),
        Some(thing) => HttpResponse::Ok().json(thing.read().unwrap().get_event_descriptions(None)),
    }
}

/// Handle a GET request to /events/<event_name>.
async fn handle_get_event(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let thing = match state.get_thing(req.match_info().get("thing_id")) {
        Some(thing) => thing,
        None => return HttpResponse::NotFound().finish(),
    };

    let event_name = match req.match_info().get("event_name") {
        Some(event_name) => event_name,
        None => return HttpResponse::NotFound().finish(),
    };

    let thing = thing.read().unwrap();
    HttpResponse::Ok().json(thing.get_event_descriptions(Some(event_name.to_string())))
}

/// Server to represent a Web Thing over HTTP.
pub struct WebThingServer {
    things: ThingsType,
    base_path: String,
    disable_host_validation: bool,
    port: Option<u16>,
    hostname: Option<String>,
    dns_service: Option<libmdns::Service>,
    #[allow(dead_code)]
    ssl_options: Option<(String, String)>,
    generator_arc: Arc<dyn ActionGenerator>,
}

impl WebThingServer {
    /// Create a new WebThingServer.
    ///
    /// # Arguments
    ///
    /// * `things` - list of Things managed by this server
    /// * `port` - port to listen on (defaults to 80)
    /// * `hostname` - optional host name, i.e. mything.com
    /// * `ssl_options` - tuple of SSL options to pass to the actix web server
    /// * `action_generator` - action generator struct
    /// * `base_path` - base URL to use, rather than '/'
    /// * `disable_host_validation` - whether or not to disable host validation -- note that this
    ///   can lead to DNS rebinding attacks. `None` means to use the default,
    ///   which keeps it enabled.
    pub fn new(
        things: ThingsType,
        port: Option<u16>,
        hostname: Option<String>,
        ssl_options: Option<(String, String)>,
        action_generator: Box<dyn ActionGenerator>,
        base_path: Option<String>,
        disable_host_validation: Option<bool>,
    ) -> Self {
        Self {
            things,
            base_path: base_path
                .map(|p| p.trim_end_matches('/').to_string())
                .unwrap_or_else(|| "".to_owned()),
            disable_host_validation: disable_host_validation.unwrap_or(false),
            port,
            hostname,
            dns_service: None,
            ssl_options,
            generator_arc: Arc::from(action_generator),
        }
    }

    fn set_href_prefix(&mut self) {
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
    }

    /// Return the base actix configuration for the server
    /// useful for testing.
    pub fn make_config(&mut self) -> impl Fn(&mut web::ServiceConfig) + Clone + 'static {
        let port = self.port.unwrap_or(80);

        let mut hosts = vec!["localhost".to_owned(), format!("localhost:{}", port)];

        if let Ok(system_hostname) = hostname::get() {
            let name = system_hostname.into_string().unwrap().to_lowercase();
            hosts.push(format!("{}.local", name));
            hosts.push(format!("{}.local:{}", name, port));
        }

        for address in get_addresses() {
            hosts.push(address.clone());
            hosts.push(format!("{}:{}", address, port));
        }

        if let Some(ref hostname) = self.hostname {
            let name = hostname.to_lowercase();
            hosts.push(name.clone());
            hosts.push(format!("{}:{}", name, port));
        }

        self.set_href_prefix();

        let single = match &self.things {
            ThingsType::Multiple(_, _) => false,
            ThingsType::Single(_) => true,
        };
        let things_arc = Arc::new(self.things.clone());
        let hosts_arc = Arc::new(hosts.clone());
        let generator_arc_clone = self.generator_arc.clone();
        let disable_host_validation_arc = Arc::new(self.disable_host_validation);

        let bp = self.base_path.clone();

        move |app: &mut web::ServiceConfig| {
            app.app_data(Data::new(AppState {
                things: things_arc.clone(),
                hosts: hosts_arc.clone(),
                disable_host_validation: disable_host_validation_arc.clone(),
                action_generator: generator_arc_clone.clone(),
            }));

            if single {
                let root = if bp.is_empty() {
                    "/".to_owned()
                } else {
                    bp.clone()
                };

                app.service(
                    web::resource(&root)
                        .route(
                            web::route()
                                .guard(guard::Get())
                                .guard(guard::Header("upgrade", "websocket"))
                                .to(handle_ws_thing),
                        )
                        .route(web::get().to(handle_get_thing)),
                )
                .service(
                    web::resource(&format!("{}/properties", bp))
                        .route(web::get().to(handle_get_properties)),
                )
                .service(
                    web::resource(&format!("{}/properties/{{property_name}}", bp))
                        .route(web::get().to(handle_get_property))
                        .route(web::put().to(handle_put_property)),
                )
                .service(
                    web::resource(&format!("{}/actions", bp))
                        .route(web::get().to(handle_get_actions))
                        .route(web::post().to(handle_post_actions)),
                )
                .service(
                    web::resource(&format!("{}/actions/{{action_name}}", bp))
                        .route(web::get().to(handle_get_action))
                        .route(web::post().to(handle_post_action)),
                )
                .service(
                    web::resource(&format!("{}/actions/{{action_name}}/{{action_id}}", bp))
                        .route(web::get().to(handle_get_action_id))
                        .route(web::delete().to(handle_delete_action_id))
                        .route(web::put().to(handle_put_action_id)),
                )
                .service(
                    web::resource(&format!("{}/events", bp))
                        .route(web::get().to(handle_get_events)),
                )
                .service(
                    web::resource(&format!("{}/events/{{event_name}}", bp))
                        .route(web::get().to(handle_get_event)),
                );
            } else {
                app.service(web::resource("/").route(web::get().to(handle_get_things)))
                    .service(
                        web::scope(&format!("{}/{{thing_id}}", bp))
                            .service(
                                web::resource("")
                                    .route(
                                        web::route()
                                            .guard(guard::Get())
                                            .guard(guard::Header("upgrade", "websocket"))
                                            .to(handle_ws_thing),
                                    )
                                    .route(web::get().to(handle_get_thing)),
                            )
                            .service(
                                web::resource("/properties")
                                    .route(web::get().to(handle_get_properties)),
                            )
                            .service(
                                web::resource("/properties/{property_name}")
                                    .route(web::get().to(handle_get_property))
                                    .route(web::put().to(handle_put_property)),
                            )
                            .service(
                                web::resource("/actions")
                                    .route(web::get().to(handle_get_actions))
                                    .route(web::post().to(handle_post_actions)),
                            )
                            .service(
                                web::resource("/actions/{action_name}")
                                    .route(web::get().to(handle_get_action))
                                    .route(web::post().to(handle_post_action)),
                            )
                            .service(
                                web::resource("/actions/{action_name}/{action_id}")
                                    .route(web::get().to(handle_get_action_id))
                                    .route(web::delete().to(handle_delete_action_id))
                                    .route(web::put().to(handle_put_action_id)),
                            )
                            .service(
                                web::resource("/events").route(web::get().to(handle_get_events)),
                            )
                            .service(
                                web::resource("/events/{event_name}")
                                    .route(web::get().to(handle_get_event)),
                            ),
                    );
            }
        }
    }

    /// Start listening for incoming connections.
    pub fn start(
        &mut self,
        configure: Option<&'static (dyn Fn(&mut web::ServiceConfig) + Send + Sync + 'static)>,
    ) -> Server {
        let port = self.port.unwrap_or(80);

        let name = match &self.things {
            ThingsType::Single(thing) => thing.read().unwrap().get_title(),
            ThingsType::Multiple(_, name) => name.to_owned(),
        };

        let things_config = self.make_config();

        let server = HttpServer::new(move || {
            let app = App::new()
                .wrap(middleware::Logger::default())
                .wrap(HostValidator)
                .wrap(
                    middleware::DefaultHeaders::new()
                        .add(("Access-Control-Allow-Origin", "*"))
                        .add((
                            "Access-Control-Allow-Methods",
                            "GET, HEAD, PUT, POST, DELETE, OPTIONS",
                        ))
                        .add((
                            "Access-Control-Allow-Headers",
                            "Origin, Content-Type, Accept, X-Requested-With",
                        )),
                )
                .configure(&things_config);

            if let Some(ref configure) = configure {
                app.configure(configure)
            } else {
                app
            }
        });

        let responder = libmdns::Responder::new().unwrap();

        #[cfg(feature = "ssl")]
        match self.ssl_options {
            Some(ref o) => {
                self.dns_service = Some(responder.register(
                    SERVICE_TYPE.to_owned(),
                    name.clone(),
                    port,
                    &["path=/", "tls=1"],
                ));

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
                self.dns_service = Some(responder.register(
                    SERVICE_TYPE.to_owned(),
                    name.clone(),
                    port,
                    &["path=/"],
                ));
                server
                    .bind(format!("0.0.0.0:{}", port))
                    .expect("Failed to bind socket")
                    .run()
            }
        }

        #[cfg(not(feature = "ssl"))]
        {
            self.dns_service =
                Some(responder.register(SERVICE_TYPE.to_owned(), name, port, &["path=/"]));
            server
                .bind(format!("0.0.0.0:{}", port))
                .expect("Failed to bind socket")
                .run()
        }
    }
}
