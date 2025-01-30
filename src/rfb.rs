use std::string::FromUtf8Error;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use thiserror::Error;

// Reference: https://www.rfc-editor.org/rfc/rfc6143

#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("insufficient bytes")]
    InsufficientBytes,
    #[error("could not decode string")]
    Utf8(#[from] FromUtf8Error),
    #[error("unsupported client message")]
    UnsupportedC2S(u8),
    #[error("unsupported server message")]
    UnsupportedS2C(u8),
}

fn ensure_size(buf: &Bytes, size: usize) -> Result<(), DecodeError> {
    if buf.len() >= size {
        Ok(())
    } else {
        Err(DecodeError::InsufficientBytes)
    }
}

pub trait Message: Sized {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError>;
    fn write_to(&self, buf: &mut BytesMut);
}

/* All strings in VNC are either ASCII or Latin-1, both of which
are embedded in Unicode. */
impl Message for String {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 4)?;
        let len = buf.get_u32() as _;
        ensure_size(buf, len)?;
        let bytes = buf.split_to(len as _);
        Ok(String::from_utf8(bytes.to_vec())?)
    }

    fn write_to(&self, buf: &mut BytesMut) {
        let len = self.len().try_into().unwrap();
        buf.put_u32(len);
        buf.extend_from_slice(self.as_bytes());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version(Bytes);

impl Message for Version {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 12)?;
        let bytes = buf.split_to(12);
        Ok(Self(bytes))
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put(self.0.as_ref())
    }
}

/// ```text
/// +--------------------------+-------------+--------------------------+
/// | No. of bytes             | Type        | Description              |
/// |                          | [Value]     |                          |
/// +--------------------------+-------------+--------------------------+
/// | 1                        | U8          | number-of-security-types |
/// | number-of-security-types | U8 array    | security-types           |
/// +--------------------------+-------------+--------------------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityTypes(pub Bytes);

impl Message for SecurityTypes {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 1)?;
        let count = buf.get_u8() as _;
        ensure_size(buf, count)?;
        let bytes = buf.split_to(count);
        Ok(SecurityTypes(bytes))
    }

    fn write_to(&self, buf: &mut BytesMut) {
        let count = self.0.len().try_into().unwrap();
        buf.put_u8(count);
        buf.put(self.0.as_ref());
    }
}

/// ```text
/// +--------------+--------------+---------------+
/// | No. of bytes | Type [Value] | Description   |
/// +--------------+--------------+---------------+
/// | 1            | U8           | security-type |
/// +--------------+--------------+---------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityType(pub u8);

impl Message for SecurityType {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 1)?;
        Ok(Self(buf.get_u8()))
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u8(self.0);
    }
}

/// ```text
/// +--------------+--------------+-------------+
/// | No. of bytes | Type [Value] | Description |
/// +--------------+--------------+-------------+
/// | 4            | U32          | status:     |
/// |              | 0            | OK          |
/// |              | 1            | failed      |
/// +--------------+--------------+-------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SecurityResult(pub u32);

impl Message for SecurityResult {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 4)?;
        Ok(Self(buf.get_u32()))
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u32(self.0);
    }
}

/// ```text
/// +--------------+--------------+-------------+
/// | No. of bytes | Type [Value] | Description |
/// +--------------+--------------+-------------+
/// | 1            | U8           | shared-flag |
/// +--------------+--------------+-------------+
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientInit {
    pub shared: bool,
}

impl Message for ClientInit {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 1)?;
        let shared = buf.get_u8() != 0;
        Ok(ClientInit { shared })
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u8(if self.shared { 1 } else { 0 });
    }
}

/// ```text
/// +--------------+--------------+-----------------+
/// | No. of bytes | Type [Value] | Description     |
/// +--------------+--------------+-----------------+
/// | 1            | U8           | bits-per-pixel  |
/// | 1            | U8           | depth           |
/// | 1            | U8           | big-endian-flag |
/// | 1            | U8           | true-color-flag |
/// | 2            | U16          | red-max         |
/// | 2            | U16          | green-max       |
/// | 2            | U16          | blue-max        |
/// | 1            | U8           | red-shift       |
/// | 1            | U8           | green-shift     |
/// | 1            | U8           | blue-shift      |
/// | 3            |              | padding         |
/// +--------------+--------------+-----------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelFormat {
    pub bits_per_pixel: u8,
    pub depth: u8,
    pub big_endian: bool,
    pub true_colour: bool,
    pub red_max: u16,
    pub green_max: u16,
    pub blue_max: u16,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

impl Message for PixelFormat {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 16)?;
        let pixel_format = PixelFormat {
            bits_per_pixel: buf.get_u8(),
            depth: buf.get_u8(),
            big_endian: buf.get_u8() != 0,
            true_colour: buf.get_u8() != 0,
            red_max: buf.get_u16(),
            green_max: buf.get_u16(),
            blue_max: buf.get_u16(),
            red_shift: buf.get_u8(),
            green_shift: buf.get_u8(),
            blue_shift: buf.get_u8(),
        };
        let _pad = buf.split_to(3);
        Ok(pixel_format)
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u8(self.bits_per_pixel);
        buf.put_u8(self.depth);
        buf.put_u8(if self.big_endian { 1 } else { 0 });
        buf.put_u8(if self.true_colour { 1 } else { 0 });
        buf.put_u16(self.red_max);
        buf.put_u16(self.green_max);
        buf.put_u16(self.blue_max);
        buf.put_u8(self.red_shift);
        buf.put_u8(self.green_shift);
        buf.put_u8(self.blue_shift);
        buf.put_bytes(0, 3);
    }
}

/// ```text
/// +--------------+--------------+------------------------------+
/// | No. of bytes | Type [Value] | Description                  |
/// +--------------+--------------+------------------------------+
/// | 2            | U16          | framebuffer-width in pixels  |
/// | 2            | U16          | framebuffer-height in pixels |
/// | 16           | PIXEL_FORMAT | server-pixel-format          |
/// | 4            | U32          | name-length                  |
/// | name-length  | U8 array     | name-string                  |
/// +--------------+--------------+------------------------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInit {
    pub framebuffer_width: u16,
    pub framebuffer_height: u16,
    pub pixel_format: PixelFormat,
    pub name: String,
}

impl Message for ServerInit {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 4)?;
        Ok(ServerInit {
            framebuffer_width: buf.get_u16(),
            framebuffer_height: buf.get_u16(),
            pixel_format: PixelFormat::read_from(buf)?,
            name: String::read_from(buf)?,
        })
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u16(self.framebuffer_width);
        buf.put_u16(self.framebuffer_height);
        PixelFormat::write_to(&self.pixel_format, buf);
        String::write_to(&self.name, buf);
    }
}

/// ```text
/// +--------------+--------------+----------------+
/// | No. of bytes | Type [Value] | Description    |
/// +--------------+--------------+----------------+
/// | 2            | U16          | src-x-position |
/// | 2            | U16          | src-y-position |
/// +--------------+--------------+----------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyRect {
    pub src_x: u16,
    pub src_y: u16,
}

impl Message for CopyRect {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 4)?;
        Ok(CopyRect {
            src_x: buf.get_u16(),
            src_y: buf.get_u16(),
        })
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u16(self.src_x);
        buf.put_u16(self.src_y);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Unknown(i32),
    Raw,
    CopyRect,
    Rre,
    Hextile,
    Trle,
    Zrle,
    Cursor,
    DesktopSize,
}

impl Message for Encoding {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 4)?;
        let encoding = buf.get_i32();
        match encoding {
            0 => Ok(Encoding::Raw),
            1 => Ok(Encoding::CopyRect),
            2 => Ok(Encoding::Rre),
            5 => Ok(Encoding::Hextile),
            15 => Ok(Encoding::Trle),
            16 => Ok(Encoding::Zrle),
            -239 => Ok(Encoding::Cursor),
            -223 => Ok(Encoding::DesktopSize),
            n => Ok(Encoding::Unknown(n)),
        }
    }

    fn write_to(&self, buf: &mut BytesMut) {
        let encoding = match self {
            &Encoding::Raw => 0,
            &Encoding::CopyRect => 1,
            &Encoding::Rre => 2,
            &Encoding::Hextile => 5,
            Encoding::Trle => 15,
            &Encoding::Zrle => 16,
            &Encoding::Cursor => -239,
            &Encoding::DesktopSize => -223,
            &Encoding::Unknown(n) => n,
        };
        buf.put_i32(encoding);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum C2S {
    /// ```text
    /// +--------------+--------------+--------------+
    /// | No. of bytes | Type [Value] | Description  |
    /// +--------------+--------------+--------------+
    /// | 1            | U8 [0]       | message-type |
    /// | 3            |              | padding      |
    /// | 16           | PIXEL_FORMAT | pixel-format |
    /// +--------------+--------------+--------------+
    /// ```
    SetPixelFormat(PixelFormat),
    /// ```text
    /// +--------------+--------------+---------------------+
    /// | No. of bytes | Type [Value] | Description         |
    /// +--------------+--------------+---------------------+
    /// | 1            | U8 [2]       | message-type        |
    /// | 1            |              | padding             |
    /// | 2            | U16          | number-of-encodings |
    /// | 4 * n        | S32 array    | encoding-types      |
    /// +--------------+--------------+---------------------+
    /// ```
    SetEncodings(Vec<Encoding>),
    /// ```text
    /// +--------------+--------------+--------------+
    /// | No. of bytes | Type [Value] | Description  |
    /// +--------------+--------------+--------------+
    /// | 1            | U8 [3]       | message-type |
    /// | 1            | U8           | incremental  |
    /// | 2            | U16          | x-position   |
    /// | 2            | U16          | y-position   |
    /// | 2            | U16          | width        |
    /// | 2            | U16          | height       |
    /// +--------------+--------------+--------------+
    /// ```
    FramebufferUpdateRequest {
        incremental: bool,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    },
    /// ```text
    /// +--------------+--------------+--------------+
    /// | No. of bytes | Type [Value] | Description  |
    /// +--------------+--------------+--------------+
    /// | 1            | U8 [4]       | message-type |
    /// | 1            | U8           | down-flag    |
    /// | 2            |              | padding      |
    /// | 4            | U32          | key          |
    /// +--------------+--------------+--------------+
    /// ```
    KeyEvent { down: bool, key: u32 },
    /// ```text
    /// +--------------+--------------+--------------+
    /// | No. of bytes | Type [Value] | Description  |
    /// +--------------+--------------+--------------+
    /// | 1            | U8 [5]       | message-type |
    /// | 1            | U8           | button-mask  |
    /// | 2            | U16          | x-position   |
    /// | 2            | U16          | y-position   |
    /// +--------------+--------------+--------------+
    /// ```
    PointerEvent { button_mask: u8, x: u16, y: u16 },
    /// ```text
    /// +--------------+--------------+--------------+
    /// | No. of bytes | Type [Value] | Description  |
    /// +--------------+--------------+--------------+
    /// | 1            | U8 [6]       | message-type |
    /// | 3            |              | padding      |
    /// | 4            | U32          | length       |
    /// | length       | U8 array     | text         |
    /// +--------------+--------------+--------------+
    /// ```
    CutText(String),
    // extensions
}

impl Message for C2S {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 1)?;
        let message_type = buf.get_u8();
        match message_type {
            0 => {
                ensure_size(buf, 3)?;
                let _pad = buf.split_to(3);
                Ok(C2S::SetPixelFormat(PixelFormat::read_from(buf)?))
            }
            2 => {
                ensure_size(buf, 1)?;
                let _pad = buf.split_to(1);
                let count = buf.get_u16();
                let encodings = (0..count)
                    .map(|_| Encoding::read_from(buf))
                    .collect::<Result<_, _>>()?;
                Ok(C2S::SetEncodings(encodings))
            }
            3 => {
                ensure_size(buf, 9)?;
                Ok(C2S::FramebufferUpdateRequest {
                    incremental: buf.get_u8() != 0,
                    x: buf.get_u16(),
                    y: buf.get_u16(),
                    width: buf.get_u16(),
                    height: buf.get_u16(),
                })
            }
            4 => {
                ensure_size(buf, 7)?;
                let down = buf.get_u8() != 0;
                let _pad = buf.split_to(2);
                let key = buf.get_u32();
                Ok(C2S::KeyEvent { down, key })
            }
            5 => {
                ensure_size(buf, 5)?;
                Ok(C2S::PointerEvent {
                    button_mask: buf.get_u8(),
                    x: buf.get_u16(),
                    y: buf.get_u16(),
                })
            }
            6 => {
                ensure_size(buf, 3)?;
                let _pad = buf.split_to(3);
                Ok(C2S::CutText(String::read_from(buf)?))
            }
            m => Err(DecodeError::UnsupportedC2S(m)),
        }
    }

    fn write_to(&self, buf: &mut BytesMut) {
        match self {
            C2S::SetPixelFormat(pixel_format) => {
                buf.put_u8(0);
                buf.put_bytes(0, 3);
                PixelFormat::write_to(pixel_format, buf);
            }
            C2S::SetEncodings(encodings) => {
                buf.put_u8(2);
                buf.put_u8(0);
                buf.put_u16(encodings.len().try_into().unwrap());
                for encoding in encodings {
                    Encoding::write_to(encoding, buf);
                }
            }
            C2S::FramebufferUpdateRequest {
                incremental,
                x,
                y,
                width,
                height,
            } => {
                buf.put_u8(3);
                buf.put_u8(if *incremental { 1 } else { 0 });
                buf.put_u16(*x);
                buf.put_u16(*y);
                buf.put_u16(*width);
                buf.put_u16(*height);
            }
            C2S::KeyEvent { down, key } => {
                buf.put_u8(4);
                buf.put_u8(if *down { 1 } else { 0 });
                buf.put_bytes(0, 2);
                buf.put_u32(*key);
            }
            C2S::PointerEvent { button_mask, x, y } => {
                buf.put_u8(5);
                buf.put_u8(*button_mask);
                buf.put_u16(*x);
                buf.put_u16(*y);
            }
            C2S::CutText(text) => {
                String::write_to(text, buf);
            }
        }
    }
}

/// ```text
/// +--------------+--------------+---------------+
/// | No. of bytes | Type [Value] | Description   |
/// +--------------+--------------+---------------+
/// | 2            | U16          | x-position    |
/// | 2            | U16          | y-position    |
/// | 2            | U16          | width         |
/// | 2            | U16          | height        |
/// | 4            | S32          | encoding-type |
/// +--------------+--------------+---------------+
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rectangle {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub encoding: Encoding,
}

impl Message for Rectangle {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 8)?;
        Ok(Rectangle {
            x: buf.get_u16(),
            y: buf.get_u16(),
            width: buf.get_u16(),
            height: buf.get_u16(),
            encoding: Encoding::read_from(buf)?,
        })
    }

    fn write_to(&self, buf: &mut BytesMut) {
        buf.put_u16(self.x);
        buf.put_u16(self.y);
        buf.put_u16(self.width);
        buf.put_u16(self.height);
        Encoding::write_to(&self.encoding, buf);
    }
}

impl Rectangle {
    pub fn payload_size(&self, format: &PixelFormat) -> usize {
        match self.encoding {
            Encoding::Raw => {
                self.width as usize * self.height as usize * (format.bits_per_pixel / 8) as usize
            }
            Encoding::Cursor => {
                (self.width as usize * self.height as usize * (format.bits_per_pixel / 8) as usize)
                    + (((self.width as usize + 7) / 8) * self.height as usize)
            }
            Encoding::CopyRect => 4,
            e => unimplemented!("encoding: {e:?}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S2C {
    /// ```text
    /// +--------------+--------------+----------------------+
    /// | No. of bytes | Type [Value] | Description          |
    /// +--------------+--------------+----------------------+
    /// | 1            | U8 [0]       | message-type         |
    /// | 1            |              | padding              |
    /// | 2            | U16          | number-of-rectangles |
    /// +--------------+--------------+----------------------+
    /// ```
    /// followed by number-of-rectangles rectagles, each starting with a [Rectangle] header
    FramebufferUpdate { count: u16 },
    /// ```text
    /// +--------------+--------------+------------------+
    /// | No. of bytes | Type [Value] | Description      |
    /// +--------------+--------------+------------------+
    /// | 1            | U8 [1]       | message-type     |
    /// | 1            |              | padding          |
    /// | 2            | U16          | first-color      |
    /// | 2            | U16          | number-of-colors |
    /// +--------------+--------------+------------------+
    /// ```
    /// followed by number-of-colors RGB values (3 * U16)
    SetColorMapEntries { first_color: u16, colors: Bytes },
    /// ```text
    /// +--------------+--------------+--------------+
    /// | No. of bytes | Type [Value] | Description  |
    /// +--------------+--------------+--------------+
    /// | 1            | U8 [2]       | message-type |
    /// +--------------+--------------+--------------+
    /// ```
    Bell,
    /// ```text
    /// +--------------+--------------+--------------+
    /// | No. of bytes | Type [Value] | Description  |
    /// +--------------+--------------+--------------+
    /// | 1            | U8 [3]       | message-type |
    /// | 3            |              | padding      |
    /// | 4            | U32          | length       |
    /// | length       | U8 array     | text         |
    /// +--------------+--------------+--------------+
    /// ```
    CutText(String),
}

impl Message for S2C {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 1)?;
        let message_type = buf.get_u8();
        match message_type {
            0 => {
                ensure_size(buf, 3)?;
                let _pad = buf.get_u8();
                Ok(S2C::FramebufferUpdate {
                    count: buf.get_u16(),
                })
            }
            1 => {
                let _pad = buf.get_u8();
                let first_color = buf.get_u16();
                let count = buf.get_u16() as usize;
                ensure_size(buf, count * 3 * 2)?;
                let colors = buf.split_to(count * 3 * 2);
                Ok(S2C::SetColorMapEntries {
                    first_color,
                    colors,
                })
            }
            2 => Ok(S2C::Bell),
            3 => {
                ensure_size(buf, 3)?;
                let _pad = buf.split_to(3);
                Ok(S2C::CutText(String::read_from(buf)?))
            }
            m => Err(DecodeError::UnsupportedS2C(m)),
        }
    }

    fn write_to(&self, buf: &mut BytesMut) {
        match self {
            S2C::FramebufferUpdate { count } => {
                buf.put_u8(0);
                buf.put_u8(0);
                buf.put_u16(*count);
            }
            S2C::SetColorMapEntries {
                first_color,
                colors,
            } => {
                buf.put_u8(1);
                buf.put_u8(0);
                buf.put_u16(*first_color);
                let count = colors.len() / 6;
                buf.put_u16(count.try_into().unwrap());
                buf.put(colors.as_ref());
            }
            S2C::Bell => {
                buf.put_u8(2);
            }
            S2C::CutText(text) => {
                buf.put_u8(3);
                buf.put_bytes(0, 3);
                String::write_to(text, buf);
            }
        }
    }
}

/// ```text
/// +--------------+--------------+-------------+
/// | No. of bytes | Type [Value] | Description |
/// +--------------+--------------+-------------+
/// | 4            | U32          | length      |
/// | length       | U8 array     | zlibData    |
/// +--------------+--------------+-------------+
/// ```
pub struct Zrle(Bytes);

impl Message for Zrle {
    fn read_from(buf: &mut Bytes) -> Result<Self, DecodeError> {
        ensure_size(buf, 4)?;
        let len = buf.get_u32() as _;
        ensure_size(buf, len)?;
        Ok(Zrle(buf.split_to(len as _)))
    }

    fn write_to(&self, buf: &mut BytesMut) {
        let len = self.0.len().try_into().unwrap();
        buf.put_u32(len);
        buf.extend_from_slice(&self.0);
    }
}

pub mod io {
    use bytes::{Bytes, BytesMut};
    use std::{io, mem};
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

    use super::{DecodeError, Message};
    use crate::Result;

    pub struct RfbIo<S> {
        stream: S,
        buf: BytesMut,
    }

    impl<S> RfbIo<S> {
        pub fn new(stream: S) -> Self {
            Self {
                stream,
                buf: BytesMut::with_capacity(0x1000),
            }
        }
    }

    impl<S: AsyncRead + Unpin> RfbIo<S> {
        pub async fn read_message<M: Message>(&mut self) -> Result<M> {
            loop {
                if !self.buf.is_empty() {
                    // temporarily take out self.buf (leaving behind an empty buffer)
                    let buf = mem::take(&mut self.buf).freeze();
                    // create an RC copy for reading and leave buf untouched
                    let mut read_buf = buf.clone();

                    match M::read_from(&mut read_buf) {
                        Ok(msg) => {
                            // successfully read a message from read_buf
                            // throw away buf and put read_buf back into self.buf
                            drop(buf);
                            // converting read_buf back into a BytesMut may copy
                            // if msg holds references into the original buf
                            self.buf = read_buf.into();

                            return Ok(msg);
                        }
                        Err(DecodeError::InsufficientBytes) => {}
                        Err(e) => return Err(e.into()),
                    }

                    // we need more data to fully parse a message
                    // throw away read_buf to discard the cursor
                    drop(read_buf);
                    // conversion to BytesMut should never fail as no other
                    // references can exist at this point
                    self.buf = buf
                        .try_into_mut()
                        .expect("buf not unique after partial parse");
                }

                // this will reclaim memory if possible
                self.buf.reserve(0x100);
                let bytes_read = self.stream.read_buf(&mut self.buf).await?;
                if 0 == bytes_read {
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                }
            }
        }

        pub async fn read_data(&mut self, len: usize) -> Result<Bytes> {
            self.buf.reserve(len);
            while self.buf.len() < len {
                let bytes_read = self.stream.read_buf(&mut self.buf).await?;
                if 0 == bytes_read {
                    return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
                }
            }

            let payload = self.buf.split_to(len).freeze();
            Ok(payload)
        }
    }

    impl<S: AsyncWrite + Unpin> RfbIo<S> {
        pub async fn write_message<M: Message>(&mut self, message: M) -> Result<()> {
            self.buf.clear();
            message.write_to(&mut self.buf);
            self.stream.write_all(&self.buf).await?;
            Ok(())
        }

        pub async fn write_data(&mut self, data: Bytes) -> Result<()> {
            self.stream.write_all(&data).await?;
            Ok(())
        }
    }
}
