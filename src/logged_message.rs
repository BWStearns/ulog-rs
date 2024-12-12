use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{MessageHeader, ULogError, ULogParser};

#[derive(Debug, Clone)]
pub struct LoggedMessage {
    pub log_level: u8,
    pub timestamp: u64,
    pub message: String,
}

impl<R: Read> ULogParser<R> {
    pub fn logged_messages(&self) -> &[LoggedMessage] {
        &self.logged_messages
    }

    fn read_logged_message(&mut self, msg_size: u16) -> Result<LoggedMessage, ULogError> {
        let log_level = self.reader.read_u8()?;
        let timestamp = self.reader.read_u64::<LittleEndian>()?;
        // message size is total size minus 9 bytes (1 for log_level + 8 for timestamp)
        let message = self.read_string(msg_size as usize - 9)?;
        self._current_timestamp = timestamp;

        Ok(LoggedMessage {
            log_level,
            timestamp,
            message,
        })
    }

    pub fn handle_logged_message(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_logged_message(header.msg_size) {
            Ok(log_msg) => {
                println!(
                    "[{}][{} μs] {}",
                    Self::log_level_to_string(log_msg.log_level),
                    log_msg.timestamp,
                    log_msg.message
                );
                self.logged_messages.push(log_msg);
                Ok(())
            }
            Err(e) => {
                println!("Error reading logged message: {}", e);
                Err(e)
            }
        }
    }
}
