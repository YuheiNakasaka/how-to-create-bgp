use bytes::BytesMut;

use crate::error::ConvertBytesToBgpMessageError;

use super::header::{Header, MessageType};

#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub struct KeepaliveMessage {
    header: Header,
}

// ByteとKeepaliveMessageの変換用
impl TryFrom<BytesMut> for KeepaliveMessage {
    type Error = ConvertBytesToBgpMessageError;

    fn try_from(bytes: BytesMut) -> Result<Self, Self::Error> {
        let header = Header::try_from(bytes)?;
        if header.type_ != MessageType::Keepalive {
            return Err(anyhow::anyhow!("bytes列のtypeがkeepaliveではありません。").into());
        }
        Ok(Self { header })
    }
}

// ByteとKeepaliveMessageの変換用
impl From<KeepaliveMessage> for BytesMut {
    fn from(keepalive: KeepaliveMessage) -> Self {
        keepalive.header.into()
    }
}

impl KeepaliveMessage {
    pub fn new() -> Self {
        let header = Header::new(19, MessageType::Keepalive);
        Self { header }
    }
}
