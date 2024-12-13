pub mod data_message;
pub mod dropout_message;
pub mod format_message;
pub mod info_message;
pub mod logged_message;
pub mod multi_message;
pub mod parameter_message;
pub mod subscription_message;
pub mod tagged_logged_message;

use byteorder::{LittleEndian, ReadBytesExt};
use dropout_message::DropoutStats;
use format_message::FormatMessage;
use info_message::*;
use logged_message::LoggedMessage;
use multi_message::MultiMessage;
use parameter_message::{DefaultParameterMessage, ParameterMessage};
use std::collections::HashMap;
use std::fs::File;
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
                log::error!("Unsupported type/size combination");
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
        let is_valid = matches!(
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
        if !is_valid {
            log::warn!("Invalid message type: {}", msg_type);
        }
        is_valid
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
                b'F' => self.handle_format_message(&header)?,
                b'P' => self.handle_initial_param(&header)?,
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

    pub fn parse_data(&mut self) -> Result<(), ULogError> {
        loop {
            match self.read_message_header() {
                Ok(header) => {
                    if !Self::is_valid_message_type(header.msg_type) {
                        return Ok(());
                    }
                    if header.msg_size > MAX_MESSAGE_SIZE {
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
                    log::info!("Reached end of file");
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

    #[allow(dead_code)]
    pub fn parse_reader(reader: R) -> Result<ULogParser<R>, ULogError> {
        let mut parser = ULogParser::new(reader)?;
        parser.parse_definitions()?;
        parser.parse_data()?;
        Ok(parser)
    }

    pub fn last_timestamp(&self) -> u64 {
        self._current_timestamp
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
