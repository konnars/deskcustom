use bytes::{Buf, BufMut, BytesMut};
use thiserror::Error;

pub const MAGIC: &[u8; 4] = b"DCST";
pub const PROTO_VERSION: u8 = 1;

/// Fast path: mouse deltas, ping — no ack needed.
pub const UDP_PORT: u16 = 24801;
/// Reliable path: hello, keyboard, screen switch.
pub const TCP_PORT: u16 = 24800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageKind {
    Hello = 1,
    MouseMove = 2,
    MouseButton = 3,
    Key = 4,
    Ping = 5,
    Pong = 6,
    ScreenEnter = 7,
    ScreenLeave = 8,
    Ack = 9,
    ClipboardSet = 10,
}

impl TryFrom<u8> for MessageKind {
    type Error = DecodeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Hello),
            2 => Ok(Self::MouseMove),
            3 => Ok(Self::MouseButton),
            4 => Ok(Self::Key),
            5 => Ok(Self::Ping),
            6 => Ok(Self::Pong),
            7 => Ok(Self::ScreenEnter),
            8 => Ok(Self::ScreenLeave),
            9 => Ok(Self::Ack),
            10 => Ok(Self::ClipboardSet),
            _ => Err(DecodeError::UnknownKind(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Extra(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Down,
    Up,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Hello {
        hostname: String,
        role: Role,
    },
    MouseMove {
        dx: i16,
        dy: i16,
        seq: u32,
    },
    MouseButton {
        button: MouseButton,
        pressed: bool,
    },
    Key {
        scancode: u16,
        action: KeyAction,
        modifiers: u8,
        seq: u32,
    },
    Ping {
        sent_us: u64,
    },
    Pong {
        sent_us: u64,
        recv_us: u64,
    },
    ScreenEnter {
        screen: String,
    },
    ScreenLeave {
        screen: String,
    },
    Ack {
        seq: u32,
    },
    ClipboardSet {
        text: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Server = 1,
    Client = 2,
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("buffer too short")]
    TooShort,
    #[error("bad magic")]
    BadMagic,
    #[error("unsupported protocol version {0}")]
    BadVersion(u8),
    #[error("unknown message kind {0}")]
    UnknownKind(u8),
    #[error("invalid utf-8")]
    InvalidUtf8,
}

pub fn encode(msg: &Message) -> BytesMut {
    let mut buf = BytesMut::with_capacity(64);
    buf.put_slice(MAGIC);
    buf.put_u8(PROTO_VERSION);

    match msg {
        Message::Hello { hostname, role } => {
            buf.put_u8(MessageKind::Hello as u8);
            buf.put_u8(*role as u8);
            let bytes = hostname.as_bytes();
            buf.put_u16(bytes.len() as u16);
            buf.put_slice(bytes);
        }
        Message::MouseMove { dx, dy, seq } => {
            buf.put_u8(MessageKind::MouseMove as u8);
            buf.put_u32(*seq);
            buf.put_i16(*dx);
            buf.put_i16(*dy);
        }
        Message::MouseButton { button, pressed } => {
            buf.put_u8(MessageKind::MouseButton as u8);
            buf.put_u8(match button {
                MouseButton::Left => 0,
                MouseButton::Right => 1,
                MouseButton::Middle => 2,
                MouseButton::Extra(id) => 3 + *id,
            });
            buf.put_u8(u8::from(*pressed));
        }
        Message::Key {
            scancode,
            action,
            modifiers,
            seq,
        } => {
            buf.put_u8(MessageKind::Key as u8);
            buf.put_u32(*seq);
            buf.put_u16(*scancode);
            buf.put_u8(*modifiers);
            buf.put_u8(match action {
                KeyAction::Down => 0,
                KeyAction::Up => 1,
            });
        }
        Message::Ping { sent_us } => {
            buf.put_u8(MessageKind::Ping as u8);
            buf.put_u64(*sent_us);
        }
        Message::Pong { sent_us, recv_us } => {
            buf.put_u8(MessageKind::Pong as u8);
            buf.put_u64(*sent_us);
            buf.put_u64(*recv_us);
        }
        Message::ScreenEnter { screen } => {
            buf.put_u8(MessageKind::ScreenEnter as u8);
            let bytes = screen.as_bytes();
            buf.put_u16(bytes.len() as u16);
            buf.put_slice(bytes);
        }
        Message::ScreenLeave { screen } => {
            buf.put_u8(MessageKind::ScreenLeave as u8);
            let bytes = screen.as_bytes();
            buf.put_u16(bytes.len() as u16);
            buf.put_slice(bytes);
        }
        Message::Ack { seq } => {
            buf.put_u8(MessageKind::Ack as u8);
            buf.put_u32(*seq);
        }
        Message::ClipboardSet { text } => {
            buf.put_u8(MessageKind::ClipboardSet as u8);
            let bytes = text.as_bytes();
            buf.put_u32(bytes.len() as u32);
            buf.put_slice(bytes);
        }
    }

    buf
}

pub fn decode(buf: &[u8]) -> Result<Message, DecodeError> {
    if buf.len() < 6 {
        return Err(DecodeError::TooShort);
    }
    if &buf[0..4] != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    let version = buf[4];
    if version != PROTO_VERSION {
        return Err(DecodeError::BadVersion(version));
    }

    let mut cursor = &buf[5..];
    let kind = MessageKind::try_from(cursor.get_u8())?;

    let msg = match kind {
        MessageKind::Hello => {
            if cursor.remaining() < 3 {
                return Err(DecodeError::TooShort);
            }
            let role = match cursor.get_u8() {
                1 => Role::Server,
                _ => Role::Client,
            };
            let len = cursor.get_u16() as usize;
            if cursor.remaining() < len {
                return Err(DecodeError::TooShort);
            }
            let hostname = std::str::from_utf8(&cursor[..len])
                .map_err(|_| DecodeError::InvalidUtf8)?
                .to_owned();
            cursor.advance(len);
            Message::Hello { hostname, role }
        }
        MessageKind::MouseMove => {
            if cursor.remaining() < 8 {
                return Err(DecodeError::TooShort);
            }
            let seq = cursor.get_u32();
            let dx = cursor.get_i16();
            let dy = cursor.get_i16();
            Message::MouseMove { dx, dy, seq }
        }
        MessageKind::MouseButton => {
            if cursor.remaining() < 2 {
                return Err(DecodeError::TooShort);
            }
            let id = cursor.get_u8();
            let pressed = cursor.get_u8() != 0;
            let button = match id {
                0 => MouseButton::Left,
                1 => MouseButton::Right,
                2 => MouseButton::Middle,
                n => MouseButton::Extra(n.saturating_sub(3)),
            };
            Message::MouseButton { button, pressed }
        }
        MessageKind::Key => {
            if cursor.remaining() < 8 {
                return Err(DecodeError::TooShort);
            }
            let seq = cursor.get_u32();
            let scancode = cursor.get_u16();
            let modifiers = cursor.get_u8();
            let action = if cursor.get_u8() == 0 {
                KeyAction::Down
            } else {
                KeyAction::Up
            };
            Message::Key {
                scancode,
                action,
                modifiers,
                seq,
            }
        }
        MessageKind::Ping => {
            if cursor.remaining() < 8 {
                return Err(DecodeError::TooShort);
            }
            Message::Ping {
                sent_us: cursor.get_u64(),
            }
        }
        MessageKind::Pong => {
            if cursor.remaining() < 16 {
                return Err(DecodeError::TooShort);
            }
            Message::Pong {
                sent_us: cursor.get_u64(),
                recv_us: cursor.get_u64(),
            }
        }
        MessageKind::ScreenEnter | MessageKind::ScreenLeave => {
            if cursor.remaining() < 2 {
                return Err(DecodeError::TooShort);
            }
            let len = cursor.get_u16() as usize;
            if cursor.remaining() < len {
                return Err(DecodeError::TooShort);
            }
            let screen = std::str::from_utf8(&cursor[..len])
                .map_err(|_| DecodeError::InvalidUtf8)?
                .to_owned();
            cursor.advance(len);
            if kind == MessageKind::ScreenEnter {
                Message::ScreenEnter { screen }
            } else {
                Message::ScreenLeave { screen }
            }
        }
        MessageKind::Ack => {
            if cursor.remaining() < 4 {
                return Err(DecodeError::TooShort);
            }
            Message::Ack { seq: cursor.get_u32() }
        }
        MessageKind::ClipboardSet => {
            if cursor.remaining() < 4 {
                return Err(DecodeError::TooShort);
            }
            let len = cursor.get_u32() as usize;
            if len > 1_048_576 {
                return Err(DecodeError::TooShort);
            }
            if cursor.remaining() < len {
                return Err(DecodeError::TooShort);
            }
            let text = std::str::from_utf8(&cursor[..len])
                .map_err(|_| DecodeError::InvalidUtf8)?
                .to_owned();
            cursor.advance(len);
            Message::ClipboardSet { text }
        }
    };

    Ok(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_mouse_move() {
        let msg = Message::MouseMove {
            dx: 12,
            dy: -5,
            seq: 42,
        };
        let encoded = encode(&msg);
        assert_eq!(decode(&encoded).unwrap(), msg);
    }

    #[test]
    fn roundtrip_key() {
        let msg = Message::Key {
            scancode: 0x1E,
            action: KeyAction::Down,
            modifiers: 0x02,
            seq: 7,
        };
        let encoded = encode(&msg);
        assert_eq!(decode(&encoded).unwrap(), msg);
    }

    #[test]
    fn roundtrip_clipboard() {
        let msg = Message::ClipboardSet {
            text: "hello clipboard".into(),
        };
        let encoded = encode(&msg);
        assert_eq!(decode(&encoded).unwrap(), msg);
    }
}
