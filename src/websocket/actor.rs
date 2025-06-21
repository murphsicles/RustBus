use actix::{Actor, StreamHandler, AsyncContext};
use actix_web_actors::ws::{self, WebsocketContext};
use log::{info, warn};
use serde_json;
use super::super::models::Subscription;
use super::super::AppState;
use tokio::sync::broadcast;

pub struct Subscriber {
    pub id: String,
    pub tx: broadcast::Sender<super::super::models::IndexedTx>,
    pub state: std::sync::Arc<AppState>,
}

impl Actor for Subscriber {
    type Context = WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        super::super::metrics::ACTIVE_SUBS.inc();
        let tx = self.tx.clone();
        let mut rx = tx.subscribe();
        ctx.run_interval(std::time::Duration::from_millis(100), move |act, ctx| {
            while let Ok(tx) = rx.try_recv() {
                if act.state.subscriptions.contains_key(&act.id) {
                    ctx.text(serde_json::to_string(&tx).unwrap_or_default());
                }
            }
        });
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        super::super::metrics::ACTIVE_SUBS.dec();
        self.state.subscriptions.remove(&self.id);
        info!("Subscriber {} disconnected", self.id);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Subscriber {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                match serde_json::from_str::<Subscription>(&text) {
                    Ok(sub) => {
                        let valid = sub.filter_type.is_some() || sub.op_return_pattern.is_some();
                        if valid {
                            self.state.subscriptions.insert(self.id.clone(), sub.clone());
                            info!("New subscription for client {}: {:?}", self.id, sub);
                            ctx.text(format!("Subscribed: {:?}", sub));
                        } else {
                            warn!("Invalid subscription from {}: no filters specified", self.id);
                            ctx.text("Invalid subscription: must specify filter_type or op_return_pattern");
                        }
                    }
                    Err(e) => {
                        warn!("Invalid subscription format from {}: {}", self.id, e);
                        ctx.text("Invalid subscription format");
                    }
                }
            }
            Ok(ws::Message::Binary(bin)) => {
                info!("Received binary message from {}: {} bytes", self.id, bin.len());
            }
            Ok(ws::Message::Close(reason)) => {
                info!("WebSocket closed for {}: {:?}", self.id, reason);
                ctx.close(reason);
            }
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => {},
            Ok(ws::Message::Nop) => {},
            Ok(ws::Message::Continuation(_)) => {
                warn!("Received continuation message for {}: ignoring", self.id);
            }
            Err(e) => {
                warn!("WebSocket protocol error: {:?}", e);
                ctx.close(None);
            }
        }
    }
}
