use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::ws::{Message, WebSocket},
};
use openmls::framing::MlsMessageIn;
use tokio::sync::{
    mpsc::{self, error::SendError},
    Mutex,
};

struct UserActor {
    receiver: mpsc::Receiver<UserActorMessage>,
    websocket: WebSocket,
    other_actors: Arc<Mutex<Vec<UserActorHandle>>>,
}
enum UserActorMessage {
    /// Instruct the actor to send a message to the user represented by the actor
    SendMessage(Vec<u8>),
}

enum Instruction {
    Continue,
    Stop,
}

impl UserActor {
    fn new(
        websocket: WebSocket,
        receiver: mpsc::Receiver<UserActorMessage>,
        other_actors: Arc<Mutex<Vec<UserActorHandle>>>,
    ) -> Self {
        UserActor {
            receiver,
            websocket,
            other_actors,
        }
    }

    async fn handle_websocket_message(&mut self, message: Message) -> Instruction {
        if let Message::Close(close) = message {
            tracing::debug!("Received close message: {:?}", close);
            return Instruction::Stop;
        }

        let Message::Binary(binary) = message else {
            tracing::warn!("Received unexpected message type");
            return Instruction::Continue;
        };

        let mut others = self.other_actors.lock().await;
        let mut dead_actors = Vec::new();
        for (index, other) in others.iter().enumerate() {
            //TODO use shared reference instead to avoid cloning of possibly large messages
            let result = other.send_message(binary.clone()).await;
            // Erros when channel is closed
            if let Err(error) = result {
                dead_actors.push(index);
                tracing::error!("Error sending actor message: {:?}", error);
            }
        }

        for index in dead_actors.into_iter() {
            tracing::debug!("Cleaning up deceased actor remains at index: {:?}", index);
            others.remove(index);
        }

        Instruction::Continue
    }

    async fn handle_message(&mut self, message: UserActorMessage) -> Instruction {
        match message {
            UserActorMessage::SendMessage(message) => {
                tracing::debug!("Sending message: {:?}", message);
                let result = self.websocket.send(Message::Binary(message)).await;
                if let Err(error) = result {
                    tracing::error!("Error sending message: {:?}", error);
                    return Instruction::Stop;
                }

                Instruction::Continue
            }
        }
    }
}

async fn run_my_actor(mut actor: UserActor) {
    tracing::debug!("Actor started");
    loop {
        tokio::select! {
            Some(message) = actor.receiver.recv() => {
                let result = actor.handle_message(message).await;
                if let Instruction::Stop = result {
                    break;
                }
            },
            // Stop actor on error
            Some(Ok(message)) = actor.websocket.recv() => {
              let result = actor.handle_websocket_message(message).await;
               if let Instruction::Stop = result {
                   break;
               }
            },
            else => break,
        }
    }

    tracing::debug!("Actor stopped");
}

pub(crate) struct UserActorHandle {
    sender: mpsc::Sender<UserActorMessage>,
}

impl UserActorHandle {
    pub(crate) fn new(
        websocket: WebSocket,
        other_actors: Arc<Mutex<Vec<UserActorHandle>>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(8);
        let actor = UserActor::new(websocket, receiver, other_actors);
        tokio::spawn(run_my_actor(actor));
        Self { sender }
    }

    /// Errors when actor stopped receiving messages meaning the channel is closed and the actor is deceased
    pub(crate) async fn send_message(
        &self,
        message: Vec<u8>,
    ) -> Result<(), impl std::error::Error> {
        self.sender
            .send(UserActorMessage::SendMessage(message))
            .await
    }
}
