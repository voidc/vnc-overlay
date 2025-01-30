use std::{
    net::SocketAddr,
    ops::Deref,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

use bytes::Bytes;
use log::debug;
use tokio::{
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpStream,
    },
    select,
    sync::{mpsc, watch},
    task::JoinHandle,
};

use crate::rfb::{io::RfbIo, *};
use crate::{ClientId, Error, Event, Result, State};

pub struct Client<S: State> {
    pub id: ClientId,
    pub event_tx: mpsc::Sender<Event>,
    pub state_rx: watch::Receiver<S>,
}

impl<S: State> Clone for Client<S> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            event_tx: self.event_tx.clone(),
            state_rx: self.state_rx.clone(),
        }
    }
}

impl<S: State> Client<S> {
    pub async fn handle(self, stream: TcpStream, target: SocketAddr) -> Result<()> {
        let server = TcpStream::connect(target).await?;

        let (client_rx, client_tx) = stream.into_split();
        let (mut client_rx, mut client_tx) = (RfbIo::new(client_rx), RfbIo::new(client_tx));

        let (server_rx, server_tx) = server.into_split();
        let (mut server_rx, mut server_tx) = (RfbIo::new(server_rx), RfbIo::new(server_tx));

        let pixel_format = self
            .handshake(
                &mut client_rx,
                &mut client_tx,
                &mut server_rx,
                &mut server_tx,
            )
            .await?;

        let (fmt_tx, fmt_rx) = watch::channel(pixel_format);

        let (fbreq_tx, fbreq_rx) = mpsc::channel::<C2S>(1);

        let forward_request = Arc::new(AtomicBool::new(true));

        // client to server
        let mut c2s_handler = C2SHandler {
            client: self.clone(),
            client_rx,
            server_tx,
            fmt_tx,
            fbreq_tx,
            forward_request: forward_request.clone(),
            mouse_pressed: false,
        };

        let c2s: JoinHandle<Result<()>> = tokio::spawn(async move { c2s_handler.handle().await });

        // server to client
        let mut s2c_handler = S2CHandler {
            client: self.clone(),
            server_rx,
            client_tx,
            fmt_rx,
            fbreq_rx,
            forward_request,
            icon_sent: false,
        };

        let s2c: JoinHandle<Result<()>> = tokio::spawn(async move { s2c_handler.handle().await });

        let res = select! {
            r = c2s => r.unwrap(),
            r = s2c => r.unwrap(),
        };

        self.event_tx
            .send(Event::Disconnect { id: self.id })
            .await
            .unwrap();

        res
    }

    async fn handshake(
        &self,
        client_rx: &mut RfbIo<OwnedReadHalf>,
        client_tx: &mut RfbIo<OwnedWriteHalf>,
        server_rx: &mut RfbIo<OwnedReadHalf>,
        server_tx: &mut RfbIo<OwnedWriteHalf>,
    ) -> Result<PixelFormat> {
        let server_version: Version = server_rx.read_message().await?;
        client_tx.write_message(dbg!(server_version)).await?;

        let client_version: Version = client_rx.read_message().await?;
        server_tx.write_message(dbg!(client_version)).await?;

        let version = b"RFB 003.003\n";

        let sec_type = match version {
            b"RFB 003.003\n" => {
                let sec_type: SecurityResult = server_rx.read_message().await?;
                client_tx.write_message(dbg!(sec_type)).await?;

                if sec_type.0 == 0 {
                    let err = server_rx.read_message().await?;
                    Err(Error::Protocol(err))
                } else {
                    Ok(sec_type.0)
                }
            }
            _ => {
                let sec_types: SecurityTypes = server_rx.read_message().await?;
                let has_err = sec_types.0.is_empty();
                client_tx.write_message(dbg!(sec_types)).await?;

                if has_err {
                    let err = server_rx.read_message().await?;
                    Err(Error::Protocol(err))
                } else {
                    let sec_type: SecurityType = client_rx.read_message().await?;
                    server_tx.write_message(dbg!(sec_type)).await?;
                    Ok(sec_type.0 as _)
                }
            }
        }?;

        assert_eq!(sec_type, 1);

        if version == b"RFB 003.008\n" {
            let sec_res: SecurityResult = server_rx.read_message().await?;
            client_tx.write_message(dbg!(sec_res)).await?;
        }

        let client_init: ClientInit = client_rx.read_message().await?;
        server_tx.write_message(dbg!(client_init)).await?;

        let server_init: ServerInit = server_rx.read_message().await?;
        let pixel_format = server_init.pixel_format.clone();
        client_tx.write_message(dbg!(server_init)).await?;

        Ok(pixel_format)
    }

    fn send_action(&self) {
        let _ = self.event_tx.try_send(Event::Action { id: self.id });
    }
}

struct C2SHandler<S: State> {
    client: Client<S>,
    client_rx: RfbIo<OwnedReadHalf>,
    server_tx: RfbIo<OwnedWriteHalf>,
    fmt_tx: watch::Sender<PixelFormat>,
    fbreq_tx: mpsc::Sender<C2S>,
    forward_request: Arc<AtomicBool>,
    mouse_pressed: bool,
}

impl<S: State> C2SHandler<S> {
    async fn handle(&mut self) -> Result<()> {
        loop {
            let message: C2S = self.client_rx.read_message().await?;
            let message = match message {
                C2S::SetEncodings(e) => {
                    debug!("encodings: {e:?}");
                    Some(C2S::SetEncodings(vec![
                        Encoding::Raw,
                        Encoding::Cursor,
                        Encoding::CopyRect,
                        Encoding::Zrle,
                    ]))
                }

                C2S::SetPixelFormat(pixel_format) => {
                    debug!("pixel format: {pixel_format:?}");
                    let _ = self.fmt_tx.send_replace(pixel_format.clone());
                    Some(C2S::SetPixelFormat(pixel_format))
                }

                C2S::PointerEvent { button_mask, x, y } => {
                    let mouse_pressed_new = (button_mask & 1) > 0;
                    let click = self.mouse_pressed && !mouse_pressed_new;
                    self.mouse_pressed = mouse_pressed_new;

                    let mut caputured = false;
                    if click {
                        let icon = self.client.state_rx.borrow().icon(self.client.id);
                        if icon.in_bounds(x, y) {
                            self.client.send_action();
                            caputured = true;
                        }
                    }

                    if caputured {
                        None
                    } else {
                        Some(C2S::PointerEvent { button_mask, x, y })
                    }
                }

                req @ C2S::FramebufferUpdateRequest { .. } => {
                    let _ = self.fbreq_tx.try_send(req.clone());

                    // if there is a pending proxy update, do not forward the request
                    self.forward_request.load(Ordering::SeqCst).then_some(req)
                }

                m => Some(m),
            };

            if let Some(message) = message {
                self.server_tx.write_message(message).await?;
            }
        }
    }
}

struct S2CHandler<S: State> {
    client: Client<S>,
    server_rx: RfbIo<OwnedReadHalf>,
    client_tx: RfbIo<OwnedWriteHalf>,
    fmt_rx: watch::Receiver<PixelFormat>,
    fbreq_rx: mpsc::Receiver<C2S>,
    forward_request: Arc<AtomicBool>,
    icon_sent: bool,
}

impl<S: State> S2CHandler<S> {
    async fn handle(&mut self) -> Result<()> {
        // removing this leads to issues, why?
        self.client.state_rx.mark_unchanged();

        loop {
            select! {
                m = self.server_rx.read_message() => { self.handle_message(m?).await?; },
                Ok(_) = self.client.state_rx.changed() => { self.handle_state_changed().await?; },
            };
        }
    }

    async fn handle_message(&mut self, message: S2C) -> Result<()> {
        if let S2C::FramebufferUpdate { count } = message {
            let _fbreq = self.next_request().await;

            // TODO only send if intersects?
            let send_icon = self.fmt_rx.borrow().bits_per_pixel == 32;
            let message = if send_icon {
                S2C::FramebufferUpdate { count: count + 1 }
            } else {
                message
            };

            self.client_tx.write_message(message).await?;

            for _ in 0..count {
                let rect: Rectangle = self.server_rx.read_message().await?;
                self.client_tx.write_message(rect.clone()).await?;

                match rect.encoding {
                    Encoding::Zrle => {
                        let data: Zrle = self.server_rx.read_message().await?;
                        self.client_tx.write_message(data).await?;
                    }
                    Encoding::DesktopSize => {}
                    _ => {
                        let payload_size = rect.payload_size(self.fmt_rx.borrow().deref());
                        let data = self.server_rx.read_data(payload_size).await?;
                        self.client_tx.write_data(data).await?;
                    }
                }
            }

            if send_icon {
                self.send_icon().await?;
            }
        } else {
            self.client_tx.write_message(message).await?;
        }

        Ok(())
    }

    async fn handle_state_changed(&mut self) -> Result<()> {
        let send_icon = self.fmt_rx.borrow().bits_per_pixel == 32;
        if !send_icon {
            return Ok(());
        }

        let _fbreq = self.next_request().await;
        self.client_tx
            .write_message(S2C::FramebufferUpdate { count: 1 })
            .await?;

        self.send_icon().await?;
        Ok(())
    }

    async fn send_icon(&mut self) -> Result<()> {
        let icon = self.client.state_rx.borrow().icon(self.client.id);
        let rect = Rectangle {
            x: icon.x,
            y: icon.y,
            width: icon.width,
            height: icon.height,
            encoding: Encoding::Raw,
        };

        self.client_tx.write_message(rect).await?;
        self.client_tx
            .write_data(Bytes::from_static(icon.rgba_data))
            .await?;
        self.icon_sent = true;
        Ok(())
    }

    async fn next_request(&mut self) -> C2S {
        if let Ok(c2s) = self.fbreq_rx.try_recv() {
            c2s
        } else {
            let start = Instant::now();
            // if there is no request available, disable forwarding until we get one
            self.forward_request.store(false, Ordering::SeqCst);
            let c2s = self.fbreq_rx.recv().await.unwrap();
            self.forward_request.store(true, Ordering::SeqCst);
            debug!("waited {:?} for request", start.elapsed());
            c2s
        }
    }
}
