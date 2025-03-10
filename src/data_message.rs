use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{
    format_message::FormatMessage, MessageHeader, ULogError, ULogParser, ULogType, ULogValue,
};

/// A struct representing the result of parsing a nested message in a ULog data message.
/// The `data` field contains the parsed ULog values, and `bytes_read` indicates the number of bytes consumed.
struct NestedMessageResult {
    data: Vec<ULogValue>,
    bytes_read: usize,
}

#[derive(Debug)]
/// A struct representing a data message in a ULog file.
/// The `msg_id` field contains the message ID, `time_us` contains the timestamp in microseconds,
/// and `data` contains the parsed ULog values for the message.
pub struct DataMessage {
    msg_id: u16,
    time_us: u64,
    data: Vec<ULogValue>,
}

impl<R: Read> ULogParser<R> {
    /// Reads a data message from the ULog file and returns a `DataMessage` struct containing the parsed data.
    ///
    /// This function takes the message ID, message size, and the format of the message as input. It then reads the message data, parsing the values according to the specified format, and returns a `DataMessage` struct containing the parsed data.
    ///
    /// The function handles padding fields, nested messages, and any remaining bytes in the message. It also updates the current timestamp if a new timestamp value is present in the message data.
    ///
    /// # Arguments
    /// * `msg_id` - The ID of the message to be read.
    /// * `msg_size` - The size of the message in bytes.
    /// * `format` - The format of the message, as a `FormatMessage` struct.
    ///
    /// # Returns
    /// A `Result` containing a `DataMessage` struct with the parsed data, or a `ULogError` if there was an error parsing the message.
    pub fn read_data_message(
        &mut self,
        msg_id: u16,
        msg_size: u16,
        format: &FormatMessage,
    ) -> Result<DataMessage, ULogError> {
        let mut data: Vec<ULogValue> = Vec::new();
        let mut bytes_read = 2; // Account for msg_id that was already read

        let has_trailing_padding = format
            .fields
            .last()
            .map(|f| f.field_name.starts_with("_padding"))
            .unwrap_or(false);

        for (i, field) in format.fields.iter().enumerate() {
            // Skip trailing padding field
            if has_trailing_padding
                && i == format.fields.len() - 1
                && field.field_name.starts_with("_padding")
            {
                continue;
            }
            // Handle padding fields
            if field.field_name.starts_with("_padding") {
                let padding_size = if let Some(size) = field.array_size {
                    size
                } else {
                    match field.field_type.as_str() {
                        "uint8_t" => 1,
                        "uint16_t" => 2,
                        "uint32_t" => 4,
                        "uint64_t" => 8,
                        _ => 1,
                    }
                };
                let mut padding = vec![0u8; padding_size];
                self.reader.read_exact(&mut padding)?;
                bytes_read += padding_size;
                continue;
            }
            let (mut type_info, array_size) = Self::parse_type_string(&field.field_type)?;
            // type_info.array_size = field.array_size;

            let (value, field_bytes) = match type_info.clone() {
                ULogType::Basic(value_type) => {
                    let value = self.read_typed_value(&value_type, field.array_size)?;
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
                        _ => 0,
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
                        let mut total_bytes = 0;
                        for _ in 0..size {
                            let result = self.read_nested_message(&nested_format)?;
                            total_bytes += result.bytes_read;
                            array_values.push(result.data);
                        }
                        (ULogValue::MessageArray(array_values), total_bytes)
                    } else {
                        let result = self.read_nested_message(&nested_format)?;
                        (ULogValue::Message(result.data), result.bytes_read)
                    }
                }
            };

            bytes_read += field_bytes;
            data.push(value);
        }

        // Handle any remaining bytes in the message
        if bytes_read < msg_size as usize {
            let remaining = msg_size as usize - bytes_read;
            let mut remaining_bytes = vec![0u8; remaining];
            self.reader.read_exact(&mut remaining_bytes)?;
        } else if bytes_read > msg_size as usize {
            return Err(ULogError::ParseError(format!(
                "Read too many bytes: {} > {} for message {}",
                bytes_read, msg_size, format.name
            )));
        }
        let timestamp = if let Some(ULogValue::UInt64(ts)) = data.first() {
            *ts
        } else {
            0
        };
        // Don't update the timestamp if there wasn't a new timestamp value
        if timestamp > 0 {
            self._current_timestamp = timestamp
        }
        Ok(DataMessage {
            msg_id,
            time_us: timestamp,
            data,
        })
    }

    pub fn handle_data_message(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        let msg_id = self.reader.read_u16::<LittleEndian>()?;
        let format_name = self
            .subscriptions
            .get(&msg_id)
            .ok_or_else(|| ULogError::ParseError(format!("Unknown msg_id: {}", msg_id)))?
            .message_name
            .clone();
        let format = self
            .formats
            .get(&format_name)
            .ok_or_else(|| ULogError::ParseError(format!("Unknown format: {}", format_name)))?
            .clone();
        let data = self.read_data_message(msg_id, header.msg_size, &format)?;

        self.subscriptions
            .get_mut(&msg_id)
            .unwrap()
            .insert_data(data.data);
        Ok(())
    }

    /// Reads a nested message from the ULog data stream.
    ///
    /// This function reads a nested message from the ULog data stream, based on the provided `FormatMessage` structure.
    /// It recursively reads the nested fields, handling both basic types and nested message types.
    /// The function returns a `NestedMessageResult` containing the read data and the total number of bytes read.
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

            let field_array_size = field.array_size.unwrap_or(1);

            let (type_info, array_size) = Self::parse_type_string(&field.field_type)?;

            let (value, bytes) = match type_info {
                ULogType::Basic(value_type) => {
                    let value = self.read_typed_value(&value_type, field.array_size)?;
                    let bytes = match &value {
                        ULogValue::BoolArray(_) => field_array_size,
                        ULogValue::CharArray(s) => s.len(),
                        ULogValue::DoubleArray(_) => field_array_size * 8,
                        ULogValue::FloatArray(_) => field_array_size * 4,
                        ULogValue::Int16(_) | ULogValue::UInt16(_) => 2,
                        ULogValue::Int16Array(_) => field_array_size * 2,
                        ULogValue::Int32(_) | ULogValue::UInt32(_) | ULogValue::Float(_) => 4,
                        ULogValue::Int32Array(_) => field_array_size * 4,
                        ULogValue::Int64(_) | ULogValue::UInt64(_) | ULogValue::Double(_) => 8,
                        ULogValue::Int64Array(_) => field_array_size * 8,
                        ULogValue::Int8(_)
                        | ULogValue::UInt8(_)
                        | ULogValue::Bool(_)
                        | ULogValue::Char(_) => 1,
                        ULogValue::Int8Array(_) => field_array_size,
                        ULogValue::UInt8Array(_) => field_array_size,
                        ULogValue::UInt16Array(_) => field_array_size * 2,
                        ULogValue::UInt32Array(_) => field_array_size * 4,
                        ULogValue::UInt64Array(_) => field_array_size * 8,
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

                    if let Some(size) = field.array_size {
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
}
