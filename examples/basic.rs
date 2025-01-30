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

#[derive(Debug, Clone, Copy)]
pub enum Basic {
    Red,
    Green,
    Blue,
}

impl State for Basic {
    fn icon(&self, _id: ClientId) -> Icon {
        let icon = match self {
            Basic::Red => &icons().red,
            Basic::Green => &icons().green,
            Basic::Blue => &icons().blue,
        };

        Icon {
            x: 0,
            y: 0,
            width: icon.width().try_into().unwrap(),
            height: icon.height().try_into().unwrap(),
            rgba_data: icon.as_raw(),
        }
    }

    fn handle_event(&mut self, event: Event) -> bool {
        debug!("client event {event:?}");
        match (event, &self) {
            (Event::Action { .. }, Basic::Red) => {
                *self = Basic::Green;
                true
            }
            (Event::Action { .. }, Basic::Green) => {
                *self = Basic::Blue;
                true
            }
            (Event::Action { .. }, Basic::Blue) => {
                *self = Basic::Red;
                true
            }
            (Event::Disconnect { .. }, _) => false,
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
        Basic::Blue,
    )
    .await
}
