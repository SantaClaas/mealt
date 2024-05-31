use axum::extract::ws::WebSocket;

struct WebSocketActor {
    websocket: WebSocket,
}

impl WebSocketActor {
    pub(crate) fn new(websocket: WebSocket) -> Self {
        Self { websocket }
    }
}

async fn run_websocket_actor(mut actor: WebSocketActor) {}
