use core::error;
use std::io::{self, Read};
use std::collections::HashMap;
use byteorder::{LittleEndian, ReadBytesExt};
use thiserror::Error;

/// Maximum reasonable message size (64KB should be plenty)
const MAX_MESSAGE_SIZE: u16 = 65535;

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

// Define the possible C types that can appear in info messages
#[derive(Debug, Clone, PartialEq)]
pub enum InfoValueType {
    Int8,
    UInt8,
    Int16,
    UInt16,
    Int32,
    UInt32,
    Int64,
    UInt64,
    Float,
    Double,
    Bool,
    Char,
}

// The actual value stored in an info message
#[derive(Debug, Clone)]
pub enum InfoValue {
    Int8(i8),
    UInt8(u8),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Float(f32),
    Double(f64),
    Bool(bool),
    Char(char),
    // Array variants
    Int8Array(Vec<i8>),
    UInt8Array(Vec<u8>),
    Int16Array(Vec<i16>),
    UInt16Array(Vec<u16>),
    Int32Array(Vec<i32>),
    UInt32Array(Vec<u32>),
    Int64Array(Vec<i64>),
    UInt64Array(Vec<u64>),
    FloatArray(Vec<f32>),
    DoubleArray(Vec<f64>),
    BoolArray(Vec<bool>),
    CharArray(String),  // Special case: char arrays are strings
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

// Flag bits message
#[derive(Debug)]
pub struct FlagBitsMessage {
    pub compat_flags: [u8; 8],
    pub incompat_flags: [u8; 8],
    pub appended_offsets: [u64; 3],
}

// Format message field
#[derive(Debug, Clone)]
pub struct Field {
    pub field_type: String,
    pub field_name: String,
    pub array_size: Option<usize>,
}

// Format message
#[derive(Debug, Clone)]
pub struct FormatMessage {
    pub name: String,
    pub fields: Vec<Field>,
}

// Information message
#[derive(Debug)]
pub struct InfoMessage {
    pub key: String,           // The name part of the key (e.g., "ver_hw")
    pub value_type: InfoValueType,  // The type part (e.g., "char[10]")
    pub array_size: Option<usize>,  // Size if it's an array type
    pub value: InfoValue,      // The actual value
}

// Parameter message
#[derive(Debug)]
pub struct ParameterMessage {
    pub key: String,
    pub value: String,
}

// Subscription message
#[derive(Debug)]
pub struct SubscriptionMessage {
    pub multi_id: u8,
    pub msg_id: u16,
    pub message_name: String,
}

#[derive(Debug)]
pub struct ULogParser<R: Read> {
    reader: R,
    header: ULogHeader,
    formats: HashMap<String, FormatMessage>,
    subscriptions: HashMap<u16, SubscriptionMessage>,
    logged_messages: Vec<LoggedMessage>,
    info_messages: HashMap<String, InfoMessage>,
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
            header,
            formats: HashMap::new(),
            subscriptions: HashMap::new(),
            logged_messages: Vec::new(),
            info_messages: HashMap::new(),
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

    pub fn logged_messages(&self) -> &[LoggedMessage] {
        &self.logged_messages
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

    fn parse_type_string(type_str: &str) -> Result<(InfoValueType, Option<usize>), ULogError> {
        let mut parts = type_str.split('[');
        let base_type = parts.next().unwrap_or("");
        
        let array_size = if let Some(size_str) = parts.next() {
            // Remove the trailing ']' and parse the size
            Some(size_str.trim_end_matches(']').parse::<usize>().map_err(|_| {
                ULogError::ParseError("Invalid array size".to_string())
            })?)
        } else {
            None
        };

        let value_type = match base_type {
            "int8_t" => InfoValueType::Int8,
            "uint8_t" => InfoValueType::UInt8,
            "int16_t" => InfoValueType::Int16,
            "uint16_t" => InfoValueType::UInt16,
            "int32_t" => InfoValueType::Int32,
            "uint32_t" => InfoValueType::UInt32,
            "int64_t" => InfoValueType::Int64,
            "uint64_t" => InfoValueType::UInt64,
            "float" => InfoValueType::Float,
            "double" => InfoValueType::Double,
            "bool" => InfoValueType::Bool,
            "char" => InfoValueType::Char,
            _ => return Err(ULogError::InvalidTypeName(base_type.to_string())),
        };

        Ok((value_type, array_size))
    }

    // Read a value of a given type from the reader
    fn read_typed_value(&mut self, value_type: &InfoValueType, array_size: Option<usize>) 
        -> Result<InfoValue, ULogError> {
        match (value_type, array_size) {
            // Single values
            (InfoValueType::Int8, None) => Ok(InfoValue::Int8(self.reader.read_i8()?)),
            (InfoValueType::UInt8, None) => Ok(InfoValue::UInt8(self.reader.read_u8()?)),
            (InfoValueType::Int16, None) => Ok(InfoValue::Int16(self.reader.read_i16::<LittleEndian>()?)),
            (InfoValueType::UInt16, None) => Ok(InfoValue::UInt16(self.reader.read_u16::<LittleEndian>()?)),
            (InfoValueType::Int32, None) => Ok(InfoValue::Int32(self.reader.read_i32::<LittleEndian>()?)),
            (InfoValueType::UInt32, None) => Ok(InfoValue::UInt32(self.reader.read_u32::<LittleEndian>()?)),
            (InfoValueType::Int64, None) => Ok(InfoValue::Int64(self.reader.read_i64::<LittleEndian>()?)),
            (InfoValueType::UInt64, None) => Ok(InfoValue::UInt64(self.reader.read_u64::<LittleEndian>()?)),
            (InfoValueType::Float, None) => Ok(InfoValue::Float(self.reader.read_f32::<LittleEndian>()?)),
            (InfoValueType::Double, None) => Ok(InfoValue::Double(self.reader.read_f64::<LittleEndian>()?)),
            (InfoValueType::Bool, None) => Ok(InfoValue::Bool(self.reader.read_u8()? != 0)),
            (InfoValueType::Char, None) => {
                let c = self.reader.read_u8()? as char;
                Ok(InfoValue::Char(c))
            },

            // Array values
            (InfoValueType::Int8, Some(size)) => {
                let mut values = vec![0u8; size];
                self.reader.read_exact(values.as_mut_slice())?;
                Ok(InfoValue::Int8Array(values.iter().map(|&x| x as i8).collect()))
            },
            (InfoValueType::UInt8, Some(size)) => {
                let mut values = vec![0u8; size];
                self.reader.read_exact(&mut values)?;
                Ok(InfoValue::UInt8Array(values))
            },
            // Special case for char arrays - treat as strings
            (InfoValueType::Char, Some(size)) => {
                let mut bytes = vec![0u8; size];
                self.reader.read_exact(&mut bytes)?;
                // Convert to string, trimming any null terminators
                let s = String::from_utf8_lossy(&bytes)
                    .trim_matches('\0')
                    .to_string();
                Ok(InfoValue::CharArray(s))
            },
            // Add other array types as needed...
            _ => Err(ULogError::ParseError("Unsupported type/size combination".to_string())),
        }
    }


    fn read_string(&mut self, len: usize) -> Result<String, ULogError> {
        let mut buf = vec![0u8; len];
        self.reader.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|_| ULogError::InvalidString)
    }

    pub fn read_format_message(&mut self, msg_size: u16) -> Result<FormatMessage, ULogError> {
        let format_str = self.read_string(msg_size as usize)?;
        
        let parts: Vec<&str> = format_str.split(':').collect();
        if parts.len() != 2 {
            return Err(ULogError::InvalidMessageType(b'F'));
        }

        let name = parts[0].to_string();
        let fields = parts[1]
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|field| {
                let mut parts = field.split_whitespace();
                let type_part = parts.next().unwrap_or("");
                let name_part = parts.next().unwrap_or("");

                // Parse array size if present
                let (field_type, array_size) = if type_part.contains('[') {
                    let array_parts: Vec<&str> = type_part.split(['[', ']']).collect();
                    (
                        array_parts[0].to_string(),
                        Some(array_parts[1].parse().unwrap_or(0)),
                    )
                } else {
                    (type_part.to_string(), None)
                };

                Field {
                    field_type,
                    field_name: name_part.to_string(),
                    array_size,
                }
            })
            .collect();

        Ok(FormatMessage { name, fields })
    }

    fn read_subscription(&mut self, msg_size: u16) -> Result<SubscriptionMessage, ULogError> {
        let multi_id = self.reader.read_u8()?;
        let msg_id = self.reader.read_u16::<LittleEndian>()?;
        let name = self.read_string(msg_size as usize - 3)?; // -3 for multi_id and msg_id
        Ok(SubscriptionMessage {
            multi_id,
            msg_id,
            message_name: name,
        })
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
                b'I' => {
                    match self.read_info_message(header.msg_size) {
                        Ok(info) => {
                            println!("Info message - {}: {:?}", info.key.clone(), info.value.clone());
                            self.info_messages.insert(info.key.clone(), info);
                        },
                        Err(e) => println!("Error reading info message: {}", e),
                    }
                },
                b'F' => {
                    let format = self.read_format_message(header.msg_size)?;
                    self.formats.insert(format.name.clone(), format);
                }
                b'A' => {
                    // Process the first subscription message but don't break yet
                    let subscription = self.read_subscription(header.msg_size)?;
                    println!("Found subscription: {} (msg_id: {})", subscription.message_name, subscription.msg_id);
                    self.subscriptions.insert(subscription.msg_id, subscription);
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

    /// Check if a byte represents a valid ULog message type
    fn is_valid_message_type(msg_type: u8) -> bool {
        matches!(msg_type, 
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
            b'O'   // Dropout
        )
    }
}


#[derive(Debug, Clone)]
pub struct LoggedMessage {
    pub log_level: u8,
    pub timestamp: u64,
    pub message: String,
}

impl<R: Read> ULogParser<R> {
    fn read_logged_message(&mut self, msg_size: u16) -> Result<LoggedMessage, ULogError> {
        let log_level = self.reader.read_u8()?;
        let timestamp = self.reader.read_u64::<LittleEndian>()?;
        // message size is total size minus 9 bytes (1 for log_level + 8 for timestamp)
        let message = self.read_string(msg_size as usize - 9)?;
        
        Ok(LoggedMessage {
            log_level,
            timestamp,
            message,
        })
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

    // Add method to read an information message
    fn read_info_message(&mut self, msg_size: u16) -> Result<InfoMessage, ULogError> {
        let key_len = self.reader.read_u8()? as usize;
        let key_str = self.read_string(key_len)?;

        // Split the key string into type and name
        let parts: Vec<&str> = key_str.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ULogError::ParseError("Invalid info message key format".to_string()));
        }

        let (value_type, array_size) = Self::parse_type_string(parts[0])?;
        let key_name = parts[1].to_string();

        // Calculate remaining bytes for value
        let value_size = msg_size as usize - 1 - key_len;  // -1 for key_len byte
        
        // Read the value according to its type
        let value = self.read_typed_value(&value_type, array_size)?;

        Ok(InfoMessage {
            key: key_name,
            value_type,
            array_size,
            value,
        })
    }

    pub fn parse_data(&mut self) -> Result<(), ULogError> {
        println!("Data section messages:");
        loop {
            match self.read_message_header() {
                Ok(header) => {
                    if !Self::is_valid_message_type(header.msg_type) {
                        println!("Invalid message type: {} ({})", header.msg_type as char, header.msg_type);
                        return Ok(());
                    }
                    if header.msg_size > MAX_MESSAGE_SIZE {
                        println!("Invalid message size: {} bytes", header.msg_size);
                        return Ok(());
                    }

                    match header.msg_type as char {
                        'I' => {
                            match self.read_info_message(header.msg_size) {
                                Ok(info) => {
                                    println!("Info message - {}: {:?}", info.key.clone(), info.value.clone());
                                    self.info_messages.insert(info.key.clone(), info);
                                },
                                Err(e) => println!("Error reading info message: {}", e),
                            }
                        },
                        'L' => {
                            match self.read_logged_message(header.msg_size) {
                                Ok(log_msg) => {
                                    println!("[{}][{} μs] {}", 
                                        Self::log_level_to_string(log_msg.log_level),
                                        log_msg.timestamp,
                                        log_msg.message);
                                    self.logged_messages.push(log_msg);
                                },
                                Err(e) => println!("Error reading log message: {}", e),
                            }
                        },
                        'D' => {
                            // Skip data messages for now
                            let mut buf = vec![0u8; header.msg_size as usize];
                            self.reader.read_exact(&mut buf)?;
                            // println!("Data message ({} bytes)", header.msg_size);
                        },
                        'S' => {
                            let mut buf = vec![0u8; header.msg_size as usize];
                            self.reader.read_exact(&mut buf)?;
                            // println!("Sync message ({} bytes)", header.msg_size);
                        },
                        _ => {
                            let mut buf = vec![0u8; header.msg_size as usize];
                            self.reader.read_exact(&mut buf)?;
                            // println!("Other message type: {} ({} bytes)", 
                            //         header.msg_type as char, header.msg_size);
                        }
                    }
                },
                Err(ULogError::Io(e)) if e.kind() == io::ErrorKind::UnexpectedEof => {
                    println!("Reached end of file while reading header");
                    break;
                },
                Err(e) => return Err(e),
            }
        }
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
