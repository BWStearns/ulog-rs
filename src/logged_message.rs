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

    pub fn log_level_to_string(level: u8) -> &'static str {
        let level = (level as char).to_digit(10).unwrap_or(0);
        match level {
            0 => "EMERG",
            1 => "ALERT",
            2 => "CRIT",
            3 => "ERR",
            4 => "WARNING",
            5 => "NOTICE",
            6 => "INFO",
            7 => "DEBUG",
            _ => "UNKNOWN",
        }
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
                self.logged_messages.push(log_msg);
                Ok(())
            }
            Err(e) => {
                log::error!("Error reading logged message: {}", e);
                Err(e)
            }
        }
    }
}
