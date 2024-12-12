use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{Field, MessageHeader, ULogError, ULogParser};

#[derive(Debug, Clone)]
pub struct FormatMessage {
    pub name: String,
    pub fields: Vec<Field>,
}

impl<R: Read> ULogParser<R> {
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
                    if let Some(size_str) = name_part
                        .strip_prefix("_padding")
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

    pub fn handle_thing(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        let format = self.read_format_message(header.msg_size)?;
        self.formats.insert(format.name.clone(), format);
        Ok(())
    }
}
