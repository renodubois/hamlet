// NOTE(reno): The example I'm following here uses `parking_lot` which is a crate that claims to
// have the Rust synchronization primitives like Mutex, but more efficient. Maybe a thing to look
// into?
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use actix_sse::{Data, Event, Sse};
use actix_web::Responder;
use futures_util::future;
use tokio::{sync::mpsc, time::interval};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Debug)]
pub struct Broadcaster {
    inner: Mutex<BroadcasterInner>,
}

#[derive(Debug, Clone, Default)]
struct BroadcasterInner {
    clients: Vec<mpsc::Sender<Event>>,
}

impl Broadcaster {
    pub fn new() -> Arc<Self> {
        Arc::new(Broadcaster {
            inner: Mutex::new(BroadcasterInner::default()),
        })
    }

    pub fn create() -> Arc<Self> {
        let this = Self::new();
        Broadcaster::spawn_ping(Arc::clone(&this));
        this
    }

    fn spawn_ping(this: Arc<Self>) {
        actix_web::rt::spawn(async move {
            let mut interval = interval(Duration::from_secs(10));

            loop {
                interval.tick().await;
                this.remove_stale_clients().await;
            }
        });
    }

    async fn remove_stale_clients(&self) {
        let clients = self.inner.lock().unwrap().clients.clone();

        let mut ok_clients = Vec::new();

        for client in clients {
            if client.send(Event::Comment("ping".into())).await.is_ok() {
                ok_clients.push(client.clone());
            }
        }

        self.inner.lock().unwrap().clients = ok_clients;
    }

    pub async fn new_client(&self) -> impl Responder + use<> {
        let (tx, rx) = mpsc::channel(10);

        tx.send(Data::new("connected").into()).await.unwrap();

        self.inner.lock().unwrap().clients.push(tx);

        let event_stream = ReceiverStream::new(rx);
        Sse::from_infallible_stream(event_stream)
    }

    pub async fn broadcast(&self, msg: &str) {
        println!("broadcasting!");
        let clients = self.inner.lock().unwrap().clients.clone();

        let send_futures = clients
            .iter()
            .map(|client| client.send(Data::new(msg).into()));

        future::join_all(send_futures).await;
    }

    #[cfg(test)]
    pub fn test_client(&self) -> tokio::sync::mpsc::Receiver<actix_sse::Event> {
        let (tx, rx) = tokio::sync::mpsc::channel(10);
        self.inner.lock().unwrap().clients.push(tx);
        rx
    }
}

#[cfg(test)]
mod test {
    // use super::*;

    // TODO(reno): how do I do async testing?
    // #[test]
    // fn broadcast_message_recieves() {
    //     let b = Broadcaster::create();
    //
    //     b.new_client()
    // }
}
