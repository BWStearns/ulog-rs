pub mod info_message;

use byteorder::{LittleEndian, ReadBytesExt};
use core::error;
use info_message::*;
use std::collections::HashMap;
use std::io::{self, Read};
use thiserror::Error;

/// Maximum reasonable message size (64KB should be plenty)
const MAX_MESSAGE_SIZE: u16 = 65535;

#[derive(Debug, Clone)]
pub struct LoggedMessage {
    pub log_level: u8,
    pub timestamp: u64,
    pub message: String,
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

#[derive(Debug)]
pub struct DataMessage {
    msg_id: u16,
    time_us: u64,
    data: Vec<ULogValue>
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

// Parameter message
#[derive(Debug)]
pub struct ParameterMessage {
    pub key: String,
    pub value: ULogValue,
}

// Subscription message
#[derive(Debug)]
pub struct SubscriptionMessage {
    pub multi_id: u8,
    pub msg_id: u16,
    pub message_name: String,
    pub data: Vec<Vec<ULogValue>>
}

#[derive(Debug)]
pub struct ULogParser<R: Read> {
    reader: R,
    header: ULogHeader,
    formats: HashMap<String, FormatMessage>,
    subscriptions: HashMap<u16, SubscriptionMessage>,
    logged_messages: Vec<LoggedMessage>,
    info_messages: HashMap<String, InfoMessage>,
    initial_params: HashMap<String, ParameterMessage>,
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
            initial_params: HashMap::new(),
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

    pub fn initial_params(&self) -> &HashMap<String, ParameterMessage> {
        &self.initial_params
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

    fn parse_type_string(type_str: &str) -> Result<(ULogValueType, Option<usize>), ULogError> {
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
            "int8_t" => ULogValueType::Int8,
            "uint8_t" => ULogValueType::UInt8,
            "int16_t" => ULogValueType::Int16,
            "uint16_t" => ULogValueType::UInt16,
            "int32_t" => ULogValueType::Int32,
            "uint32_t" => ULogValueType::UInt32,
            "int64_t" => ULogValueType::Int64,
            "uint64_t" => ULogValueType::UInt64,
            "float" => ULogValueType::Float,
            "double" => ULogValueType::Double,
            "bool" => ULogValueType::Bool,
            "char" => ULogValueType::Char,
            _ => return Err(ULogError::InvalidTypeName(base_type.to_string())),
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
            // Add other array types as needed...
            _ => {
                println!(
                    "Invalid type/size combination: {:?}, {:?}",
                    value_type, array_size
                );
                return Err(ULogError::ParseError(
                    "Invalid type/size combination".to_string(),
                ));
            }
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

                // Check if this is a padding field
                if name_part.starts_with("_padding") {
                    // Parse the padding size from field name (format: _padding[N])
                    if let Some(size_str) = name_part.strip_prefix("_padding")
                        .and_then(|s| s.strip_prefix('['))
                        .and_then(|s| s.strip_suffix(']'))
                        .and_then(|s| s.parse::<usize>().ok()) 
                    {
                        // Determine correct type based on padding size
                        let field_type = match size_str {
                            1 => "uint8_t".to_string(),
                            2 => "uint16_t".to_string(),
                            4 => "uint32_t".to_string(),
                            8 => "uint64_t".to_string(),
                            n => format!("uint8_t[{}]", n), // default to byte array for other sizes
                        };
                        
                        return Field {
                            field_type,
                            field_name: name_part.to_string(),
                            array_size: if size_str > 8 { Some(size_str) } else { None },
                        };
                    }
                }
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
            data: vec![],
        })
    }

    /// Check if a byte represents a valid ULog message type
    fn is_valid_message_type(msg_type: u8) -> bool {
        let valid = matches!(
            msg_type,
            b'A' | // Add message
            b'R' | // Remove message
            b'D' | // Data message
            b'H' | // Heartbeat message
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
            println!("Found message type: {} (dec) / 0x{:02X} (hex) / {} (ascii)", 
                    msg_type, msg_type, 
                    std::char::from_u32(msg_type as u32).unwrap_or('?'));
        }
        valid
    }

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
            return Err(ULogError::ParseError(
                "Invalid info message key format".to_string(),
            ));
        }

        let (value_type, array_size) = Self::parse_type_string(parts[0])?;
        let key_name = parts[1].to_string();

        // Read the value according to its type
        let value = self.read_typed_value(&value_type, array_size)?;

        Ok(InfoMessage {
            key: key_name,
            value_type,
            array_size,
            value,
        })
    }

    fn read_param_message(&mut self, msg_size: u16) -> Result<ParameterMessage, ULogError> {
        let key_len = self.reader.read_u8()? as usize;
        let key_str = self.read_string(key_len)?;
        // The key_str is of the format "<type> <name>", like "float foo"
        let parts: Vec<&str> = key_str.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(ULogError::ParseError(
                "Invalid parameter message key format".to_string(),
            ));
        }
        let param_name = parts[1].to_string();
        // Probably also want to check that the type is valid (only float, and int are allowed)
        let value_type_name = parts[0].to_string();
        let (value_type, _) = Self::parse_type_string(&value_type_name)?;
        let value = self.read_typed_value(&value_type, None)?;
        Ok(ParameterMessage {
            key: param_name,
            value: value,
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
                b'I' => match self.read_info_message(header.msg_size) {
                    Ok(info) => {
                        println!(
                            "Info message - {}: {:?}",
                            info.key.clone(),
                            info.value.clone()
                        );
                        self.info_messages.insert(info.key.clone(), info);
                    }
                    Err(e) => println!("Error reading info message: {}", e),
                },
                b'F' => {
                    let format = self.read_format_message(header.msg_size)?;
                    self.formats.insert(format.name.clone(), format);
                }
                b'P' => {
                    let param = self.read_param_message(header.msg_size)?;
                    // println!(
                    //     "Parameter message - {}: {:?}",
                    //     param.key.clone(),
                    //     param.value.clone()
                    // );
                    self.initial_params.insert(param.key.clone(), param);
                }
                // Multis are gonna be a pain, skipping for now
                // b'M' => {
                //     // let message = self.read_message(header.msg_size)?;
                //     // self.messages.insert(message.id, message);
                // }
                // Also a pain, skipping for now
                // b'Q' => {
                //     let param = self.read_param_default_message(header.msg_size)?;
                //     // println!(
                //     //     "Parameter default message - {}: {:?}",
                //     //     param.key.clone(),
                //     //     param.value.clone()
                //     // );
                //     self.default_params.insert(param.key.clone(), param);
                // }
                b'A' => {
                    // This is the first message in the data section
                    // Process the first subscription message but don't break yet
                    let subscription = self.read_subscription(header.msg_size)?;
                    println!(
                        "Found subscription: {} (msg_id: {})",
                        subscription.message_name, subscription.msg_id
                    );
                    self.subscriptions.insert(subscription.msg_id, subscription);
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

    pub fn read_data_message(&mut self, msg_id: u16, msg_size: u16, format: &FormatMessage) -> Result<DataMessage, ULogError> {        
        let mut data = Vec::new();
        let mut bytes_read = 2; // Account for msg_id that was already read
        
        for field in &format.fields {            
            // Skip padding fields entirely
            if field.field_name.starts_with("_padding") {
                // Calculate padding size
                let padding_size = if let Some(size) = field.array_size {
                    size
                } else {
                    // For non-array padding, use the base type size
                    match field.field_type.as_str() {
                        "uint8_t" => 1,
                        "uint16_t" => 2,
                        "uint32_t" => 4,
                        "uint64_t" => 8,
                        _ => 1, // Default to 1 byte if unknown
                    }
                };
                
                // Skip the padding bytes unless it's the last field
                if field != format.fields.last().unwrap() {
                    let mut padding = vec![0u8; padding_size];
                    self.reader.read_exact(&mut padding)?;
                    bytes_read += padding_size;
                }
                continue;
            }
    
            println!("Reading field: {}, {}", field.field_name, field.field_type);
            // Parse the field type
            let (value_type, array_size) = Self::parse_type_string(&field.field_type)?;
            
            // Read the value
            let value = self.read_typed_value(&value_type, array_size)?;
            
            // Calculate bytes read for this field
            let field_size = match &value {
                ULogValue::Int8(_) | ULogValue::UInt8(_) | ULogValue::Bool(_) | ULogValue::Char(_) => 1,
                ULogValue::Int16(_) | ULogValue::UInt16(_) => 2,
                ULogValue::Int32(_) | ULogValue::UInt32(_) | ULogValue::Float(_) => 4,
                ULogValue::Int64(_) | ULogValue::UInt64(_) | ULogValue::Double(_) => 8,
                ULogValue::Int8Array(v) => v.len(),
                ULogValue::UInt8Array(v) => v.len(),
                ULogValue::BoolArray(v) => v.len(),
                ULogValue::Int16Array(v) => v.len() * 2,
                ULogValue::Int32Array(v) => v.len() * 4,
                ULogValue::FloatArray(v) => v.len() * 4,
                ULogValue::Int64Array(v) => v.len() * 8,
                ULogValue::DoubleArray(v) => v.len() * 8,
                ULogValue::CharArray(s) => s.len(),
                ULogValue::UInt16Array(vec) => vec.len() * 2,
                ULogValue::UInt32Array(vec) => vec.len() * 4,
                ULogValue::UInt64Array(vec) => vec.len() * 8,
            };
            bytes_read += field_size;
            data.push(value);
        }
    
        // Verify we've read the correct number of bytes
        if bytes_read < msg_size as usize {
            // Skip any remaining bytes
            let mut remaining = vec![0u8; msg_size as usize - bytes_read];
            self.reader.read_exact(&mut remaining)?;
        }
    
        let dm = DataMessage {
            msg_id,
            time_us: if let Some(ULogValue::UInt64(ts)) = data.first() { *ts } else { 0 },
            data,
        };
        Ok(dm)
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
                        b'A' => {
                            let subscription = self.read_subscription(header.msg_size)?;
                            println!(
                                "Found subscription: {} (msg_id: {})",
                                subscription.message_name, subscription.msg_id
                            );
                            self.subscriptions.insert(subscription.msg_id, subscription);
                        }
                        b'I' => match self.read_info_message(header.msg_size) {
                            Ok(info) => {
                                println!("Info message: {:?}", info.clone());
                                self.info_messages.insert(info.key.clone(), info);
                            }
                            Err(e) => println!("Error reading info message: {}", e),
                        },
                        b'L' => match self.read_logged_message(header.msg_size) {
                            Ok(log_msg) => {
                                println!(
                                    "[{}][{} μs] {}",
                                    Self::log_level_to_string(log_msg.log_level),
                                    log_msg.timestamp,
                                    log_msg.message
                                );
                                self.logged_messages.push(log_msg);
                            }
                            Err(e) => println!("Error reading log message: {}", e),
                        },
                        b'D' => {
                            let msg_id = self.reader.read_u16::<LittleEndian>()?;
                            let format_name = self.subscriptions.get(&msg_id)
                                .ok_or_else(|| ULogError::ParseError(format!("Unknown msg_id: {}", msg_id)))?.message_name.clone();
                            let format = self.formats.get(&format_name)
                                .ok_or_else(|| ULogError::ParseError(format!("Unknown format: {}", format_name)))?.clone();
                            let data = self.read_data_message(msg_id, header.msg_size, &format)?;
                            self.subscriptions.get_mut(&msg_id).unwrap().data.push(data.data);
                        },
                        b'R' => {
                            // Skip unsubscription messages for now since they're unused
                            let mut buf = vec![0u8; header.msg_size as usize];
                            self.reader.read_exact(&mut buf)?;
                        }
                        b'S' => {
                            let mut buf = vec![0u8; header.msg_size as usize];
                            self.reader.read_exact(&mut buf)?;
                        }
                        _ => {
                            let mut buf = vec![0u8; header.msg_size as usize];
                            self.reader.read_exact(&mut buf)?;
                        }
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
