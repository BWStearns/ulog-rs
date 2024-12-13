use std::{collections::HashMap, io::Read};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{MessageHeader, ULogError, ULogParser, ULogType, ULogValue, ULogValueType};

#[derive(Debug)]
pub struct ParameterMessage {
    pub key: String,
    pub value: ULogValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultType {
    SystemWide = 1,    // 1<<0: system wide default
    Configuration = 2, // 1<<1: default for current configuration
}

#[derive(Debug, Clone)]
pub struct DefaultParameterMessage {
    pub key: String,
    pub value: ULogValue,
    pub default_types: Vec<DefaultType>, // A parameter can have multiple default types
}

impl DefaultParameterMessage {
    // Helper method to parse default types from a bitfield
    fn parse_default_types(bitfield: u8) -> Vec<DefaultType> {
        let mut types = Vec::new();

        if bitfield & (1 << 0) != 0 {
            types.push(DefaultType::SystemWide);
        }
        if bitfield & (1 << 1) != 0 {
            types.push(DefaultType::Configuration);
        }

        // Verify at least one bit is set as per spec
        if types.is_empty() {
            // Default to system-wide if none specified (though this shouldn't happen)
            types.push(DefaultType::SystemWide);
        }

        types
    }
}

impl<R: Read> ULogParser<R> {
    pub fn initial_params(&self) -> &HashMap<String, ParameterMessage> {
        &self.initial_params
    }

    pub fn default_params(&self) -> &HashMap<String, DefaultParameterMessage> {
        &self.default_params
    }

    fn read_param_message(&mut self, _msg_size: u16) -> Result<ParameterMessage, ULogError> {
        let key_len: usize = self.reader.read_u8()? as usize;
        let key_str = self.read_string(key_len)?;
        // The key_str is of the format "<type> <name>", like "float foo"
        let parts: Vec<&str> = key_str.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ULogError::ParseError(
                "Invalid parameter message key format".to_string(),
            ));
        }
        let param_name = parts[1].to_string();

        // Parse the type and handle it appropriately
        let (ulog_type, array_size) = Self::parse_type_string(parts[0])?;

        let value = match ulog_type {
            ULogType::Basic(value_type) => {
                // Verify that only float and int32 are used in parameters
                match value_type {
                    ULogValueType::Float | ULogValueType::Int32 => {
                        self.read_typed_value(&value_type, array_size)?
                    }
                    _ => {
                        return Err(ULogError::ParseError(
                            "Parameters must be float or int32".to_string(),
                        ))
                    }
                }
            }
            ULogType::Message(_) => {
                return Err(ULogError::ParseError(
                    "Message types not allowed in parameters".to_string(),
                ));
            }
        };

        Ok(ParameterMessage {
            key: param_name,
            value,
        })
    }

    pub fn read_default_parameter(&mut self) -> Result<DefaultParameterMessage, ULogError> {
        // Read the default types bitfield
        let default_types_byte = self.reader.read_u8()?;

        // Read key length and key
        let key_len = self.reader.read_u8()? as usize;
        let key_str = self.read_string(key_len)?;

        // Parse the key string (format is same as regular parameters)
        let parts: Vec<&str> = key_str.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ULogError::ParseError(
                "Invalid default parameter key format".to_string(),
            ));
        }

        let param_name = parts[1].to_string();

        // Parse the type and verify it's either float or int32
        let (ulog_type, array_size) = Self::parse_type_string(parts[0])?;
        let value = match ulog_type {
            ULogType::Basic(value_type) => {
                // Verify the type is float or int32 as per spec
                match value_type {
                    ULogValueType::Float | ULogValueType::Int32 => {
                        self.read_typed_value(&value_type, array_size)?
                    }
                    _ => {
                        return Err(ULogError::ParseError(
                            "Default parameters must be float or int32".to_string(),
                        ))
                    }
                }
            }
            ULogType::Message(_) => {
                return Err(ULogError::ParseError(
                    "Message types not allowed in default parameters".to_string(),
                ));
            }
        };

        Ok(DefaultParameterMessage {
            key: param_name,
            value,
            default_types: DefaultParameterMessage::parse_default_types(default_types_byte),
        })
    }

    pub fn handle_default_parameter(&mut self) -> Result<(), ULogError> {
        match self.read_default_parameter() {
            Ok(default_param) => {
                self.default_params
                    .insert(default_param.key.clone(), default_param);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn get_default_value(
        &self,
        param_name: &str,
        default_type: DefaultType,
    ) -> Option<&ULogValue> {
        self.default_params.get(param_name).and_then(|param| {
            if param.default_types.contains(&default_type) {
                Some(&param.value)
            } else {
                None
            }
        })
    }

    pub fn handle_initial_param(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_param_message(header.msg_size) {
            Ok(param) => {
                self.initial_params.insert(param.key.clone(), param);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn handle_parameter_change(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_param_message(header.msg_size) {
            Ok(param) => {
                let parameter_changes = self.changed_params.entry(param.key.clone()).or_default();
                parameter_changes.push(param);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}
