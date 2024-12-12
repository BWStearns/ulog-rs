pub mod data_message;
pub mod format_message;
pub mod info_message;
pub mod logged_message;
pub mod subscription_message;
pub mod tagged_logged_message;

use byteorder::{LittleEndian, ReadBytesExt};
use format_message::FormatMessage;
use info_message::*;
use logged_message::LoggedMessage;
use std::collections::HashMap;
use std::io::{self, Read};
use subscription_message::SubscriptionMessage;
use tagged_logged_message::*;
use thiserror::Error;

/// Maximum reasonable message size (64KB should be plenty)
const MAX_MESSAGE_SIZE: u16 = 65535;

#[derive(Debug, Clone, PartialEq)]
pub enum ULogType {
    Basic(ULogValueType),
    Message(String), // For nested message types
}

#[derive(Debug, Error)]
pub enum ULogError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid magic bytes")]
    InvalidMagic,
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u8),
    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),
    #[error("Invalid string data")]
    InvalidString,
    #[error("Invalid type name: {0}")]
    InvalidTypeName(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IncompatibleFlags: {:?}", .0)]
    IncompatibleFlags(Vec<u8>),
}
// File header (16 bytes)
#[derive(Debug)]
pub struct ULogHeader {
    pub version: u8,
    pub timestamp: u64,
}

// Message header (3 bytes)
#[derive(Debug)]
pub struct MessageHeader {
    pub msg_size: u16,
    pub msg_type: u8,
}

#[derive(Debug)]
pub struct FlagBitsMessage {
    pub compat_flags: [u8; 8],
    pub incompat_flags: [u8; 8],
    pub appended_offsets: [u64; 3],
}

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

#[derive(Debug)]
pub struct ULogParser<R: Read> {
    reader: R,
    _current_timestamp: u64,
    dropout_details: DropoutStats,
    header: ULogHeader,
    formats: HashMap<String, FormatMessage>,
    subscriptions: HashMap<u16, SubscriptionMessage>,
    logged_messages: Vec<LoggedMessage>,
    logged_messages_tagged: HashMap<u16, Vec<TaggedLoggedMessage>>,
    info_messages: HashMap<String, InfoMessage>,
    initial_params: HashMap<String, ParameterMessage>,
    multi_messages: HashMap<String, Vec<MultiMessage>>,
    default_params: HashMap<String, DefaultParameterMessage>,
    changed_params: HashMap<String, Vec<ParameterMessage>>,
}

struct NestedMessageResult {
    data: Vec<ULogValue>,
    bytes_read: usize,
}

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
    pub fn new(mut reader: R) -> Result<Self, ULogError> {
        // Read and verify magic bytes
        let mut magic = [0u8; 7];
        reader.read_exact(&mut magic)?;
        if magic != [0x55, 0x4C, 0x6F, 0x67, 0x01, 0x12, 0x35] {
            return Err(ULogError::InvalidMagic);
        }

        // Read version
        let version = reader.read_u8()?;
        if version != 1 {
            return Err(ULogError::UnsupportedVersion(version));
        }

        // Read timestamp
        let timestamp = reader.read_u64::<LittleEndian>()?;

        let header = ULogHeader { version, timestamp };

        Ok(ULogParser {
            reader,
            _current_timestamp: timestamp,
            dropout_details: DropoutStats {
                total_drops: 0,
                total_duration_ms: 0,
                dropouts: Vec::new(),
            },
            header,
            formats: HashMap::new(),
            subscriptions: HashMap::new(),
            logged_messages: Vec::new(),
            logged_messages_tagged: HashMap::new(),
            info_messages: HashMap::new(),
            initial_params: HashMap::new(),
            multi_messages: HashMap::new(),
            default_params: HashMap::new(),
            changed_params: HashMap::new(),
        })
    }

    pub fn header(&self) -> &ULogHeader {
        &self.header
    }

    pub fn formats(&self) -> &HashMap<String, FormatMessage> {
        &self.formats
    }

    pub fn subscriptions(&self) -> &HashMap<u16, SubscriptionMessage> {
        &self.subscriptions
    }

    pub fn initial_params(&self) -> &HashMap<String, ParameterMessage> {
        &self.initial_params
    }

    pub fn default_params(&self) -> &HashMap<String, DefaultParameterMessage> {
        &self.default_params
    }

    pub fn multi_messages(&self) -> &HashMap<String, Vec<MultiMessage>> {
        &self.multi_messages
    }

    pub fn info_messages(&self) -> &HashMap<String, InfoMessage> {
        &self.info_messages
    }

    pub fn dropout_details(&self) -> &DropoutStats {
        &self.dropout_details
    }

    fn _dump_next_bytes(&mut self, count: usize) -> Result<(), ULogError> {
        let mut buf = vec![0u8; count];
        self.reader.read_exact(&mut buf)?;
        println!("Next {} bytes: {:?}", count, buf);
        Ok(())
    }

    pub fn read_message_header(&mut self) -> Result<MessageHeader, ULogError> {
        let msg_size = self.reader.read_u16::<LittleEndian>()?;
        let msg_type = self.reader.read_u8()?;
        Ok(MessageHeader { msg_size, msg_type })
    }

    pub fn read_flag_bits(&mut self) -> Result<FlagBitsMessage, ULogError> {
        let mut compat_flags = [0u8; 8];
        let mut incompat_flags = [0u8; 8];
        let mut appended_offsets = [0u64; 3];

        self.reader.read_exact(&mut compat_flags)?;
        self.reader.read_exact(&mut incompat_flags)?;

        for offset in &mut appended_offsets {
            *offset = self.reader.read_u64::<LittleEndian>()?;
        }

        // Check incompatible flags
        if incompat_flags.iter().any(|&x| x != 0) {
            return Err(ULogError::IncompatibleFlags(incompat_flags.to_vec()));
        }

        Ok(FlagBitsMessage {
            compat_flags,
            incompat_flags,
            appended_offsets,
        })
    }

    fn parse_type_string(type_str: &str) -> Result<(ULogType, Option<usize>), ULogError> {
        let mut parts = type_str.split('[');
        let base_type = parts.next().unwrap_or("");

        let array_size = if let Some(size_str) = parts.next() {
            // Remove the trailing ']' and parse the size
            Some(
                size_str
                    .trim_end_matches(']')
                    .parse::<usize>()
                    .map_err(|_| ULogError::ParseError("Invalid array size".to_string()))?,
            )
        } else {
            None
        };

        let value_type = match base_type {
            // Basic types
            "int8_t" => ULogType::Basic(ULogValueType::Int8),
            "uint8_t" => ULogType::Basic(ULogValueType::UInt8),
            "int16_t" => ULogType::Basic(ULogValueType::Int16),
            "uint16_t" => ULogType::Basic(ULogValueType::UInt16),
            "int32_t" => ULogType::Basic(ULogValueType::Int32),
            "uint32_t" => ULogType::Basic(ULogValueType::UInt32),
            "int64_t" => ULogType::Basic(ULogValueType::Int64),
            "uint64_t" => ULogType::Basic(ULogValueType::UInt64),
            "float" => ULogType::Basic(ULogValueType::Float),
            "double" => ULogType::Basic(ULogValueType::Double),
            "bool" => ULogType::Basic(ULogValueType::Bool),
            "char" => ULogType::Basic(ULogValueType::Char),
            // Any other type is treated as a message type
            _ => ULogType::Message(base_type.to_string()),
        };

        Ok((value_type, array_size))
    }

    // Read a value of a given type from the reader
    fn read_typed_value(
        &mut self,
        value_type: &ULogValueType,
        array_size: Option<usize>,
    ) -> Result<ULogValue, ULogError> {
        match (value_type, array_size) {
            // Single values
            (ULogValueType::Int8, None) => Ok(ULogValue::Int8(self.reader.read_i8()?)),
            (ULogValueType::UInt8, None) => Ok(ULogValue::UInt8(self.reader.read_u8()?)),
            (ULogValueType::Int16, None) => {
                Ok(ULogValue::Int16(self.reader.read_i16::<LittleEndian>()?))
            }
            (ULogValueType::UInt16, None) => {
                Ok(ULogValue::UInt16(self.reader.read_u16::<LittleEndian>()?))
            }
            (ULogValueType::Int32, None) => {
                Ok(ULogValue::Int32(self.reader.read_i32::<LittleEndian>()?))
            }
            (ULogValueType::UInt32, None) => {
                Ok(ULogValue::UInt32(self.reader.read_u32::<LittleEndian>()?))
            }
            (ULogValueType::Int64, None) => {
                Ok(ULogValue::Int64(self.reader.read_i64::<LittleEndian>()?))
            }
            (ULogValueType::UInt64, None) => {
                Ok(ULogValue::UInt64(self.reader.read_u64::<LittleEndian>()?))
            }
            (ULogValueType::Float, None) => {
                Ok(ULogValue::Float(self.reader.read_f32::<LittleEndian>()?))
            }
            (ULogValueType::Double, None) => {
                Ok(ULogValue::Double(self.reader.read_f64::<LittleEndian>()?))
            }
            (ULogValueType::Bool, None) => Ok(ULogValue::Bool(self.reader.read_u8()? != 0)),
            (ULogValueType::Char, None) => {
                let c = self.reader.read_u8()? as char;
                Ok(ULogValue::Char(c))
            }

            // Array values
            (ULogValueType::Int8, Some(size)) => {
                let mut values = vec![0u8; size];
                self.reader.read_exact(values.as_mut_slice())?;
                Ok(ULogValue::Int8Array(
                    values.iter().map(|&x| x as i8).collect(),
                ))
            }
            (ULogValueType::UInt8, Some(size)) => {
                let mut values = vec![0u8; size];
                self.reader.read_exact(&mut values)?;
                Ok(ULogValue::UInt8Array(values))
            }
            // Special case for char arrays - treat as strings
            (ULogValueType::Char, Some(size)) => {
                let mut bytes = vec![0u8; size];
                self.reader.read_exact(&mut bytes)?;
                // Convert to string, trimming any null terminators
                let s = String::from_utf8_lossy(&bytes)
                    .trim_matches('\0')
                    .to_string();
                Ok(ULogValue::CharArray(s))
            }
            _ => {
                println!(
                    "Invalid type/size combination: {:?}, {:?}",
                    value_type, array_size
                );
                Err(ULogError::ParseError(
                    "Invalid type/size combination".to_string(),
                ))
            }
        }
    }

    fn read_string(&mut self, len: usize) -> Result<String, ULogError> {
        let mut buf = vec![0u8; len];
        self.reader.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|_| ULogError::InvalidString)
    }

    /// Check if a byte represents a valid ULog message type
    fn is_valid_message_type(msg_type: u8) -> bool {
        let valid = matches!(
            msg_type,
            b'A' | // Add message
            b'R' | // Remove message
            b'D' | // Data message
            b'I' | // Info message
            b'M' | // Multi info message
            b'P' | // Parameter message
            b'Q' | // Parameter default message
            b'L' | // Logged string
            b'C' | // Tagged logged string
            b'S' | // Synchronization
            b'O' // Dropout
        );
        if !valid {
            println!(
                "Found message type: {} (dec) / 0x{:02X} (hex) / {} (ascii)",
                msg_type,
                msg_type,
                std::char::from_u32(msg_type as u32).unwrap_or('?')
            );
        }
        valid
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

    fn handle_multi_message(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
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

    fn read_default_parameter(&mut self) -> Result<DefaultParameterMessage, ULogError> {
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

    fn handle_default_parameter(&mut self) -> Result<(), ULogError> {
        match self.read_default_parameter() {
            Ok(default_param) => {
                println!(
                    "Default parameter: {} = {:?} (types: {:?})",
                    default_param.key, default_param.value, default_param.default_types
                );
                self.default_params
                    .insert(default_param.key.clone(), default_param);
                Ok(())
            }
            Err(e) => {
                println!("Error reading default parameter: {}", e);
                Err(e)
            }
        }
    }

    fn handle_parameter_change(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_param_message(header.msg_size) {
            Ok(param) => {
                println!("Parameter change: {} = {:?}", param.key, param.value);
                let parameter_changes = self.changed_params.entry(param.key.clone()).or_default();
                parameter_changes.push(param);
                Ok(())
            }
            Err(e) => {
                println!("Error reading parameter change: {}", e);
                Err(e)
            }
        }
    }

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

    fn handle_dropout(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        match self.read_dropout_message(header.msg_size) {
            Ok(dropout) => {
                println!("Dropout: {:?}", dropout);
                self.dropout_details.total_drops += 1;
                self.dropout_details.total_duration_ms += dropout.duration as u32;
                self.dropout_details.dropouts.push(dropout);
                Ok(())
            }
            Err(e) => {
                println!("Error reading dropout: {}", e);
                Err(e)
            }
        }
    }

    pub fn parse_definitions(&mut self) -> Result<(), ULogError> {
        // Read flag bits message first
        let header = self.read_message_header()?;
        if header.msg_type != b'B' {
            return Err(ULogError::InvalidMessageType(header.msg_type));
        }
        let _flag_bits = self.read_flag_bits()?;

        // Parse definition section until we hit data section
        loop {
            let header = self.read_message_header()?;
            match header.msg_type {
                b'I' => self.handle_info_message(&header)?,
                b'F' => {
                    let format = self.read_format_message(header.msg_size)?;
                    self.formats.insert(format.name.clone(), format);
                }
                b'P' => {
                    let param = self.read_param_message(header.msg_size)?;
                    self.initial_params.insert(param.key.clone(), param);
                }
                b'M' => self.handle_multi_message(&header)?,
                b'Q' => self.handle_default_parameter()?,
                b'A' => {
                    // This is the first message in the data section
                    // Process the first subscription message but don't break yet
                    self.handle_subscription_message(&header)?;
                    // Now break to continue parsing data section
                    break;
                }
                _ => {
                    // Skip unknown message types
                    let mut buf = vec![0u8; header.msg_size as usize];
                    self.reader.read_exact(&mut buf)?;
                }
            }
        }

        Ok(())
    }

    fn read_nested_message(
        &mut self,
        format: &FormatMessage,
    ) -> Result<NestedMessageResult, ULogError> {
        let mut nested_data = Vec::new();
        let mut total_bytes_read = 0;

        for field in &format.fields {
            // Skip padding fields in nested messages
            if field.field_name.starts_with("_padding") {
                continue;
            }

            let (type_info, array_size) = Self::parse_type_string(&field.field_type)?;

            let (value, bytes) = match type_info {
                ULogType::Basic(value_type) => {
                    let value = self.read_typed_value(&value_type, array_size)?;
                    let bytes = match &value {
                        ULogValue::BoolArray(v) => v.len(),
                        ULogValue::CharArray(s) => s.len(),
                        ULogValue::DoubleArray(v) => v.len() * 8,
                        ULogValue::FloatArray(v) => v.len() * 4,
                        ULogValue::Int16(_) | ULogValue::UInt16(_) => 2,
                        ULogValue::Int16Array(v) => v.len() * 2,
                        ULogValue::Int32(_) | ULogValue::UInt32(_) | ULogValue::Float(_) => 4,
                        ULogValue::Int32Array(v) => v.len() * 4,
                        ULogValue::Int64(_) | ULogValue::UInt64(_) | ULogValue::Double(_) => 8,
                        ULogValue::Int64Array(v) => v.len() * 8,
                        ULogValue::Int8(_)
                        | ULogValue::UInt8(_)
                        | ULogValue::Bool(_)
                        | ULogValue::Char(_) => 1,
                        ULogValue::Int8Array(v) => v.len(),
                        ULogValue::UInt8Array(v) => v.len(),
                        ULogValue::UInt16Array(v) => v.len() * 2,
                        ULogValue::UInt32Array(v) => v.len() * 4,
                        ULogValue::UInt64Array(v) => v.len() * 8,
                        _ => 0, // Should never happen for basic types
                    };
                    (value, bytes)
                }
                ULogType::Message(msg_type) => {
                    let nested_format = self
                        .formats
                        .get(&msg_type)
                        .ok_or_else(|| {
                            ULogError::ParseError(format!("Unknown message type: {}", msg_type))
                        })?
                        .clone();

                    if let Some(size) = array_size {
                        let mut array_values = Vec::with_capacity(size);
                        let mut array_bytes = 0;
                        for _ in 0..size {
                            let result = self.read_nested_message(&nested_format)?;
                            array_bytes += result.bytes_read;
                            array_values.push(result.data);
                        }
                        (ULogValue::MessageArray(array_values), array_bytes)
                    } else {
                        let result = self.read_nested_message(&nested_format)?;
                        (ULogValue::Message(result.data), result.bytes_read)
                    }
                }
            };
            total_bytes_read += bytes;
            nested_data.push(value);
        }
        Ok(NestedMessageResult {
            data: nested_data,
            bytes_read: total_bytes_read,
        })
    }

    pub fn parse_data(&mut self) -> Result<(), ULogError> {
        println!("Data section messages:");
        loop {
            match self.read_message_header() {
                Ok(header) => {
                    if !Self::is_valid_message_type(header.msg_type) {
                        println!(
                            "Invalid message type: {} ({})",
                            header.msg_type as char, header.msg_type
                        );
                        return Ok(());
                    }
                    if header.msg_size > MAX_MESSAGE_SIZE {
                        println!("Invalid message size: {} bytes", header.msg_size);
                        return Ok(());
                    }

                    match header.msg_type {
                        b'A' => self.handle_subscription_message(&header)?,
                        b'I' => self.handle_info_message(&header)?,
                        b'M' => self.handle_multi_message(&header)?,
                        b'L' => self.handle_logged_message(&header)?,
                        b'C' => self.handle_tagged_logged_message(&header)?,
                        b'D' => self.handle_data_message(&header)?,
                        b'O' => self.handle_dropout(&header)?,
                        b'P' => self.handle_parameter_change(&header)?,
                        // Skip unsubscription messages for now since they're unused
                        b'R' => self.skip_message(&header)?,
                        // Skipping synchronization messages for now
                        b'S' => self.skip_message(&header)?,
                        b'Q' => {
                            self.handle_default_parameter()?;
                        }
                        _ => self.skip_message(&header)?,
                    }
                }
                Err(ULogError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    println!("Reached end of file while reading header");
                    break;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn skip_message(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        let mut buf = vec![0u8; header.msg_size as usize];
        self.reader.read_exact(&mut buf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_header() {
        let mut data = vec![];
        // Magic bytes
        data.extend_from_slice(&[0x55, 0x4C, 0x6F, 0x67, 0x01, 0x12, 0x35]);
        // Version
        data.push(1);
        // Timestamp
        data.extend_from_slice(&[0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77]);

        let parser = ULogParser::new(Cursor::new(data)).unwrap();
        assert_eq!(parser.header.version, 1);
        assert_eq!(parser.header.timestamp, 0x7766554433221100);
    }
}
