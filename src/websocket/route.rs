use actix_web::{web, HttpResponse};
use actix_web_actors::ws;
use super::super::AppState;
use super::Subscriber;

pub async fn ws_route(
    req: actix_web::HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let subscriber = Subscriber {
        id: uuid::Uuid::new_v4().to_string(),
        tx: state.tx_channel.clone(),
        state: state.into_inner(),
    };
    ws::start(subscriber, &req, stream)
}
