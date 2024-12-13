use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{MessageHeader, ULogError, ULogParser};

#[derive(Debug, Clone)]
pub struct DropoutMessage {
    pub timestamp: u64,
    pub duration: u16, // Duration of dropout in milliseconds
}

// Collection of dropouts that occurred during logging
#[derive(Debug, Clone)]
pub struct DropoutStats {
    pub total_drops: usize,
    pub total_duration_ms: u32,
    pub dropouts: Vec<DropoutMessage>,
}

impl<R: Read> ULogParser<R> {
    fn read_dropout_message(&mut self, msg_size: u16) -> Result<DropoutMessage, ULogError> {
        if msg_size != 2 {
            return Err(ULogError::ParseError(format!(
                "Invalid dropout message size: {}",
                msg_size
            )));
        }

        let duration = self.reader.read_u16::<LittleEndian>()?;

        Ok(DropoutMessage {
            duration,
            timestamp: self._current_timestamp,
        })
    }

    pub fn handle_dropout(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_dropout_message(header.msg_size) {
            Ok(dropout) => {
                self.dropout_details.total_drops += 1;
                self.dropout_details.total_duration_ms += dropout.duration as u32;
                self.dropout_details.dropouts.push(dropout);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn dropout_details(&self) -> &DropoutStats {
        &self.dropout_details
    }
}
