use std::{collections::HashMap, io::Read};

use byteorder::ReadBytesExt;
use serde_derive::Serialize;

use crate::{MessageHeader, ULogError, ULogParser, ULogType, ULogValue, ULogValueType};

// Format message field
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Field {
    pub field_type: String,
    pub field_name: String,
    pub array_size: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InfoMessage {
    pub key: String,               // The name part of the key (e.g., "ver_hw")
    pub value_type: ULogValueType, // The type part (e.g., "char[10]")
    pub array_size: Option<usize>, // Size if it's an array type
    pub value: ULogValue,          // The actual value
}

impl InfoMessage {
    pub fn as_string(&self) -> Option<&str> {
        if let ULogValue::CharArray(s) = &self.value {
            Some(s)
        } else {
            None
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        if let ULogValue::UInt32(v) = self.value {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        if let ULogValue::UInt64(v) = self.value {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        if let ULogValue::Int32(v) = self.value {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        if let ULogValue::Float(v) = self.value {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        if let ULogValue::Double(v) = self.value {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let ULogValue::Bool(v) = self.value {
            Some(v)
        } else {
            None
        }
    }

    // Array access methods
    pub fn as_string_array(&self) -> Option<&str> {
        self.as_string() // Since we store char arrays as strings anyway
    }

    pub fn as_u32_array(&self) -> Option<&[u32]> {
        if let ULogValue::UInt32Array(v) = &self.value {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_f32_array(&self) -> Option<&[f32]> {
        if let ULogValue::FloatArray(v) = &self.value {
            Some(v)
        } else {
            None
        }
    }

    // Method to get type information
    pub fn value_type(&self) -> &ULogValueType {
        &self.value_type
    }

    // Method to check if value is an array
    pub fn is_array(&self) -> bool {
        self.array_size.is_some()
    }

    // Generic method to get raw value
    pub fn raw_value(&self) -> &ULogValue {
        &self.value
    }
}

impl<R: Read> ULogParser<R> {
    pub fn info_messages(&self) -> &HashMap<String, InfoMessage> {
        &self.info_messages
    }

    // In read_info_message, modify the value reading section:
    pub fn read_info_message(&mut self) -> Result<InfoMessage, ULogError> {
        let key_len = self.reader.read_u8()? as usize;
        let key_str = self.read_string(key_len)?;

        // Split the key string into type and name
        let parts: Vec<&str> = key_str.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ULogError::ParseError(
                "Invalid info message key format".to_string(),
            ));
        }

        let (ulog_type, array_size) = Self::parse_type_string(parts[0])?;
        let key_name = parts[1].to_string();

        // Handle basic types and message types differently
        let value = match &ulog_type {
            ULogType::Basic(value_type) => {
                // Now we correctly pass a ULogValueType
                self.read_typed_value(value_type, array_size)?
            }
            ULogType::Message(_) => {
                return Err(ULogError::ParseError(
                    "Message types not allowed in info messages".to_string(),
                ));
            }
        };

        // Extract the value_type for storage in InfoMessage
        let value_type = match ulog_type {
            ULogType::Basic(vt) => vt,
            _ => unreachable!(), // We've already handled the Message case above
        };

        Ok(InfoMessage {
            key: key_name,
            value_type,
            array_size,
            value,
        })
    }

    pub fn handle_info_message(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_info_message() {
            Ok(info) => {
                self.info_messages.insert(info.key.clone(), info);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
