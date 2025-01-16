use std::{collections::HashMap, io::Read};

use crate::{MessageHeader, ULogError, ULogParser};
use byteorder::{LittleEndian, ReadBytesExt};
use serde_derive::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TaggedLoggedMessage {
    pub log_level: u8,
    pub tag: u16,
    pub timestamp: u64,
    pub message: String,
}

impl<R: Read> ULogParser<R> {
    pub fn read_tagged_logged_message(
        &mut self,
        msg_size: u16,
    ) -> Result<TaggedLoggedMessage, ULogError> {
        let log_level = self.reader.read_u8()?;
        let tag = self.reader.read_u16::<LittleEndian>()?;
        let timestamp = self.reader.read_u64::<LittleEndian>()?;
        // Message size is total size minus 11 bytes (1 for log_level + 2 for tag + 8 for timestamp)
        let message = self.read_string(msg_size as usize - 11)?;

        Ok(TaggedLoggedMessage {
            log_level,
            tag,
            timestamp,
            message,
        })
    }

    pub fn handle_tagged_logged_message(
        &mut self,
        header: &MessageHeader,
    ) -> Result<(), ULogError> {
        let msg = self.read_tagged_logged_message(header.msg_size)?;
        if let Some(tag_hash) = self.logged_messages_tagged.get_mut(&msg.tag) {
            tag_hash.push(msg);
        } else {
            self.logged_messages_tagged.insert(msg.tag, vec![msg]);
        }
        Ok(())
    }

    pub fn logged_messages_tagged(&self) -> &HashMap<u16, Vec<TaggedLoggedMessage>> {
        &self.logged_messages_tagged
    }
}
