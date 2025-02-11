use std::{io::Cursor, sync::OnceLock};

use image::{ImageReader, RgbaImage};
use log::{debug, info};

use vncproxy::*;

struct Icons {
    red: RgbaImage,
    green: RgbaImage,
    blue: RgbaImage,
}

fn icons() -> &'static Icons {
    const RED_BYTES: &[u8] = include_bytes!("../res/red.png");
    const GREEN_BYTES: &[u8] = include_bytes!("../res/green.png");
    const BLUE_BYTES: &[u8] = include_bytes!("../res/blue.png");

    fn load(bytes: &[u8]) -> RgbaImage {
        ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .expect("could not guess image format")
            .decode()
            .expect("could not decode image file")
            .into_rgba8()
    }

    static ICONS: OnceLock<Icons> = OnceLock::new();
    ICONS.get_or_init(|| Icons {
        red: load(RED_BYTES),
        green: load(GREEN_BYTES),
        blue: load(BLUE_BYTES),
    })
}

#[derive(Debug, Clone)]
pub struct Lock(Option<ClientId>);

impl Lock {
    fn icon_kind(&self, id: ClientId) -> IconKind {
        match self.0 {
            None => IconKind::Nobody,
            Some(lock_id) if lock_id == id => IconKind::Me,
            _ => IconKind::Peer,
        }
    }
}

impl State for Lock {
    fn icon(&self, id: ClientId) -> Icon {
        self.icon_kind(id).icon()
    }

    fn handle_event(&mut self, event: Event) -> bool {
        debug!("client event {event:?}");
        match event {
            Event::Action { id } => match self.0 {
                None => {
                    self.0 = Some(id);
                    true
                }
                Some(lock_id) if lock_id == id => {
                    self.0 = None;
                    true
                }
                _ => false,
            },
            Event::Disconnect { id } => match self.0 {
                Some(lock_id) if lock_id == id => {
                    self.0 = None;
                    true
                }
                _ => false,
            },
        }
    }

    fn enable_input(&self, id: ClientId) -> bool {
        self.icon_kind(id) == IconKind::Me
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum IconKind {
    Me,
    Peer,
    Nobody,
}

impl IconKind {
    fn icon(self) -> Icon {
        let icon = match self {
            IconKind::Me => &icons().green,
            IconKind::Peer => &icons().red,
            IconKind::Nobody => &icons().blue,
        };

        Icon {
            x: 0,
            y: 0,
            width: icon.width().try_into().unwrap(),
            height: icon.height().try_into().unwrap(),
            rgba_data: icon.as_raw(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    info!("Running");

    // preload
    let _ = icons();

    run_proxy(
        "0.0.0.0:5911".parse().unwrap(),
        "127.0.0.1:5900".parse().unwrap(),
        Lock(None),
    )
    .await
}
