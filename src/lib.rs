use std::net::SocketAddr;

use log::info;
use thiserror::Error;
use tokio::{
    net::TcpListener,
    select,
    sync::{mpsc, watch},
};

use client::Client;
pub use rfb::DecodeError;

mod client;
mod rfb;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),
    #[error("could not decode message")]
    Decode(#[from] DecodeError),
    #[error("Protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, Error>;

pub type ClientId = usize;

pub trait State: Send + Sync + 'static {
    fn icon(&self, id: ClientId) -> Icon;
    fn handle_event(&mut self, event: Event) -> bool;
    fn enable_input(&self, id: ClientId) -> bool;
}

#[derive(Debug)]
pub enum Event {
    Action { id: ClientId },
    Disconnect { id: ClientId },
}

pub struct Icon {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub rgba_data: &'static [u8],
}

impl Icon {
    fn in_bounds(&self, x: u16, y: u16) -> bool {
        self.x <= x && x < self.x + self.width && self.y <= y && y < self.y + self.height
    }
}

pub async fn run_proxy<S: State>(
    proxy_addr: SocketAddr,
    dest_addr: SocketAddr,
    initial: S,
) -> Result<()> {
    let listener = TcpListener::bind(proxy_addr).await?;

    let mut client_counter = 0;
    let (event_tx, mut event_rx) = mpsc::channel(16);
    let (state_tx, state_rx) = watch::channel(initial);

    loop {
        select! {
            incoming = listener.accept() => {
                let (stream, _) = incoming?;
                info!("Connection from {}", stream.peer_addr()?);
                let event_tx = event_tx.clone();
                let state_rx = state_rx.clone();
                let id = client_counter;
                client_counter += 1;

                tokio::spawn(async move {
                    let client = Client {
                        id,
                        event_tx,
                        state_rx,
                    };
                    client.handle(stream, dest_addr).await.unwrap();
                });
            }
            Some(event) = event_rx.recv() => {
                state_tx.send_if_modified(|state| state.handle_event(event));
            }
        }
    }
}
