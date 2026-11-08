use bevy::prelude::*;

pub const DEFAULT_USERNAME: &str = "spoon";
pub const CONNECTION_DATA_BYTES: usize = 256;
const HEADER: &[u8; 5] = b"APIC\x01";
const MAX_USERNAME_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientKind {
    Observer,
    Console,
    Touchpad,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    username: String,
    kind: ClientKind,
}

impl ClientIdentity {
    pub fn new(username: &str, kind: ClientKind) -> Result<Self, String> {
        if username.is_empty()
            || username.len() > MAX_USERNAME_BYTES
            || username.chars().any(|character| {
                character.is_whitespace() || character.is_control() || character == '@'
            })
        {
            return Err("Username must contain 1–64 UTF-8 bytes without whitespace, control characters or @".into());
        }
        Ok(Self {
            username: username.into(),
            kind,
        })
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn kind(&self) -> ClientKind {
        self.kind
    }

    pub fn encode(&self) -> [u8; CONNECTION_DATA_BYTES] {
        let mut data = [0; CONNECTION_DATA_BYTES];
        data[..5].copy_from_slice(HEADER);
        data[5] = match self.kind {
            ClientKind::Observer => 0,
            ClientKind::Console => 1,
            ClientKind::Touchpad => 2,
        };
        data[6] = self.username.len() as u8;
        data[7..7 + self.username.len()].copy_from_slice(self.username.as_bytes());
        data
    }

    pub fn decode(data: &[u8; CONNECTION_DATA_BYTES]) -> Result<Self, String> {
        if &data[..5] != HEADER {
            return Err("Missing or unsupported connection identity".into());
        }
        let kind = match data[5] {
            0 => ClientKind::Observer,
            1 => ClientKind::Console,
            2 => ClientKind::Touchpad,
            _ => return Err("Unknown client kind".into()),
        };
        let length = usize::from(data[6]);
        if length > MAX_USERNAME_BYTES {
            return Err("Username is too long".into());
        }
        let username =
            std::str::from_utf8(&data[7..7 + length]).map_err(|_| "Username must be UTF-8")?;
        Self::new(username, kind)
    }
}
