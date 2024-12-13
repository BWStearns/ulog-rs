use std::{collections::HashMap, io::Read};

use byteorder::ReadBytesExt;

use crate::{MessageHeader, ULogError, ULogParser, ULogType, ULogValue, ULogValueType};

#[derive(Debug, Clone)]
pub struct MultiMessage {
    pub is_continued: bool,
    pub key: String,
    pub value_type: ULogValueType,
    pub array_size: Option<usize>,
    pub value: ULogValue,
}

pub trait MultiMessageCombiner {
    fn combine_values(&self) -> Option<ULogValue>;
}

impl MultiMessageCombiner for Vec<MultiMessage> {
    fn combine_values(&self) -> Option<ULogValue> {
        if self.is_empty() {
            return None;
        }

        // All messages should have the same type, so use the first one's type
        let first = &self[0];
        match &first.value {
            ULogValue::CharArray(_) => {
                // Combine string values
                let combined: String = self
                    .iter()
                    .filter_map(|msg| {
                        if let ULogValue::CharArray(s) = &msg.value {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();
                Some(ULogValue::CharArray(combined))
            }
            ULogValue::UInt8Array(_arr) => {
                // Combine byte arrays
                let mut combined = Vec::new();
                for msg in self {
                    if let ULogValue::UInt8Array(arr) = &msg.value {
                        combined.extend_from_slice(arr);
                    }
                }
                Some(ULogValue::UInt8Array(combined))
            }
            // Add other array types as needed...
            _ => {
                println!("Unsupported multi message value type");
                None
            }
        }
    }
}

impl<R: Read> ULogParser<R> {
    pub fn multi_messages(&self) -> &HashMap<String, Vec<MultiMessage>> {
        &self.multi_messages
    }

    pub fn handle_multi_message(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_multi_message(header.msg_size) {
            Ok(multi_msg) => {
                // If this is a continuation of a previous message
                if multi_msg.is_continued {
                    if let Some(existing_msgs) = self.multi_messages.get_mut(&multi_msg.key) {
                        existing_msgs.push(multi_msg);
                    } else {
                        // This is an error case - got a continuation without a start
                        println!(
                            "Warning: Got continuation message without initial message for key: {}",
                            multi_msg.clone().key
                        );
                        let mut msgs = Vec::new();
                        msgs.push(multi_msg.clone());
                        self.multi_messages.insert(multi_msg.key.clone(), msgs);
                    }
                } else {
                    // This is a new message or the start of a new series
                    let mut msgs = Vec::new();
                    msgs.push(multi_msg.clone());
                    self.multi_messages.insert(multi_msg.key.clone(), msgs);
                }
                Ok(())
            }
            Err(e) => {
                println!("Error reading multi message: {}", e);
                Err(e)
            }
        }
    }

    fn read_multi_message(&mut self, msg_size: u16) -> Result<MultiMessage, ULogError> {
        // Read is_continued flag
        let is_continued = self.reader.read_u8()? != 0;

        // Read key length and key
        let key_len = self.reader.read_u8()? as usize;
        let key_str = self.read_string(key_len)?;

        // Split the key string into type and name
        let parts: Vec<&str> = key_str.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ULogError::ParseError(
                "Invalid multi message key format".to_string(),
            ));
        }

        let (ulog_type, _array_size) = Self::parse_type_string(parts[0])?;
        let key_name = parts[1].to_string();

        // Calculate remaining bytes for value
        // 2 bytes for is_continued and key_len, plus key_len bytes for the key
        let value_size = msg_size as usize - 2 - key_len;

        // Handle basic types and message types differently
        let value = match &ulog_type {
            ULogType::Basic(value_type) => {
                // Read the value based on the remaining size
                let custom_size = Some(value_size);
                self.read_typed_value(value_type, custom_size)?
            }
            ULogType::Message(_) => {
                return Err(ULogError::ParseError(
                    "Message types not allowed in multi messages".to_string(),
                ));
            }
        };

        // Extract the value_type for storage
        let value_type = match ulog_type {
            ULogType::Basic(vt) => vt,
            _ => unreachable!(), // We've already handled the Message case above
        };

        Ok(MultiMessage {
            is_continued,
            key: key_name,
            value_type,
            array_size: Some(value_size), // Store the actual size we read
            value,
        })
    }
}
