// Format message field
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Field {
    pub field_type: String,
    pub field_name: String,
    pub array_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct FormatMessage {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone)]
pub struct InfoMessage {
    pub key: String,               // The name part of the key (e.g., "ver_hw")
    pub value_type: ULogValueType, // The type part (e.g., "char[10]")
    pub array_size: Option<usize>, // Size if it's an array type
    pub value: ULogValue,          // The actual value
}

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
                let combined: String = self.iter()
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

// Define the possible C types that can appear in info messages
#[derive(Debug, Clone, PartialEq)]
pub enum ULogValueType {
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
pub enum ULogValue {
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
    CharArray(String),                 // Special case: char arrays are strings
    Message(Vec<ULogValue>),           // For nested message types
    MessageArray(Vec<Vec<ULogValue>>), // For arrays of nested message types
}
