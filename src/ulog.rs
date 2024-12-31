use std::collections::HashMap;
use std::io::Read;

use crate::{
    dropout_message::DropoutStats,
    format_message::FormatMessage,
    info_message::InfoMessage,
    logged_message::LoggedMessage,
    multi_message::MultiMessage,
    parameter_message::{DefaultParameterMessage, ParameterMessage},
    subscription_message::SubscriptionMessage,
    tagged_logged_message::TaggedLoggedMessage,
    ULogError, ULogParser, ULogValue,
};

/// A structured representation of a fully parsed ULog file.
#[derive(Debug)]
pub struct ULog {
    // Metadata
    pub timestamp_start: u64,
    pub version: u8,

    // Messages
    pub formats: HashMap<String, FormatMessage>,
    pub subscriptions: HashMap<u16, SubscriptionMessage>,
    pub logged_messages: Vec<LoggedMessage>,
    pub tagged_messages: HashMap<u16, Vec<TaggedLoggedMessage>>,
    pub info_messages: HashMap<String, InfoMessage>,
    pub initial_parameters: HashMap<String, ParameterMessage>,
    pub changed_parameters: HashMap<String, Vec<ParameterMessage>>,
    pub multi_messages: HashMap<String, Vec<MultiMessage>>,
    pub default_parameters: HashMap<String, DefaultParameterMessage>,
    pub dropout_stats: DropoutStats,
}

impl ULog {
    /// Parse a ULog from any type that implements Read
    pub fn parse<R: Read>(reader: R) -> Result<Self, ULogError> {
        let mut parser = ULogParser::new(reader)?;
        parser.parse_definitions()?;
        parser.parse_data()?;

        Ok(Self {
            timestamp_start: parser.header().timestamp,
            version: parser.header().version,
            formats: parser.formats().clone(),
            subscriptions: parser.subscriptions().clone(),
            logged_messages: parser.logged_messages().to_vec(),
            tagged_messages: parser.logged_messages_tagged().clone(),
            info_messages: parser.info_messages().clone(),
            initial_parameters: parser.initial_params().clone(),
            changed_parameters: parser.changed_params().clone(),
            multi_messages: parser.multi_messages().clone(),
            default_parameters: parser.default_params().clone(),
            dropout_stats: parser.dropout_details().clone(),
        })
    }

    /// Get the start timestamp in microseconds
    pub fn start_timestamp(&self) -> u64 {
        self.timestamp_start
    }

    /// Get file format version
    pub fn version(&self) -> u8 {
        self.version
    }

    /// Get message formats by name
    pub fn get_format(&self, name: &str) -> Option<&FormatMessage> {
        self.formats.get(name)
    }

    /// Get subscription by message ID
    pub fn get_subscription(&self, msg_id: u16) -> Option<&SubscriptionMessage> {
        self.subscriptions.get(&msg_id)
    }

    /// Get all messages for a given subscription by name
    pub fn get_subscription_messages(&self, name: &str) -> Option<&Vec<Vec<ULogValue>>> {
        self.subscriptions
            .values()
            .find(|sub| sub.message_name == name)
            .map(|sub| &sub.data)
    }

    /// Get info message by key
    pub fn get_info(&self, key: &str) -> Option<&InfoMessage> {
        self.info_messages.get(key)
    }

    /// Get system name from info messages
    pub fn system_name(&self) -> Option<&str> {
        self.get_info("sys_name").and_then(|msg| msg.as_string())
    }

    /// Get hardware version from info messages
    pub fn hardware_version(&self) -> Option<&str> {
        self.get_info("ver_hw").and_then(|msg| msg.as_string())
    }

    /// Get software version from info messages
    pub fn software_version(&self) -> Option<&str> {
        self.get_info("ver_sw").and_then(|msg| msg.as_string())
    }

    /// Get initial parameter value
    pub fn get_initial_parameter(&self, name: &str) -> Option<&ParameterMessage> {
        self.initial_parameters.get(name)
    }

    /// Get parameter changes over time
    pub fn get_parameter_changes(&self, name: &str) -> Option<&Vec<ParameterMessage>> {
        self.changed_parameters.get(name)
    }

    /// Get all log messages at a specific log level
    pub fn get_logged_messages_by_level(&self, level: u8) -> Vec<&LoggedMessage> {
        self.logged_messages
            .iter()
            .filter(|msg| msg.log_level == level)
            .collect()
    }

    /// Get all log messages with a specific tag
    pub fn get_tagged_messages_by_tag(&self, tag: u16) -> Option<&Vec<TaggedLoggedMessage>> {
        self.tagged_messages.get(&tag)
    }

    /// Get total number of dropouts
    pub fn dropout_count(&self) -> usize {
        self.dropout_stats.total_drops
    }

    /// Get total duration of dropouts in milliseconds
    pub fn total_dropout_duration(&self) -> u32 {
        self.dropout_stats.total_duration_ms
    }

    /// Calculate the approximate data loss percentage due to dropouts
    pub fn data_loss_percentage(&self) -> f32 {
        let total_time = self.last_timestamp() - self.timestamp_start;
        if total_time == 0 {
            return 0.0;
        }
        (self.dropout_stats.total_duration_ms as f64 * 1000.0 / total_time as f64) as f32 * 100.0
    }

    /// Get the last timestamp in the log
    pub fn last_timestamp(&self) -> u64 {
        // Check various sources for the last timestamp
        let mut last_timestamp = self.timestamp_start;

        // Check logged messages
        if let Some(last_msg) = self.logged_messages.last() {
            last_timestamp = last_timestamp.max(last_msg.timestamp);
        }

        // Check tagged messages
        for messages in self.tagged_messages.values() {
            if let Some(last_msg) = messages.last() {
                last_timestamp = last_timestamp.max(last_msg.timestamp);
            }
        }

        // Check subscription data
        for sub in self.subscriptions.values() {
            if let Some(last_data) = sub.data.last() {
                if let Some(first_value) = last_data.first() {
                    if let ULogValue::UInt64(ts) = first_value {
                        last_timestamp = last_timestamp.max(*ts);
                    }
                }
            }
        }

        last_timestamp
    }

    /// Calculate total log duration in seconds
    pub fn duration_seconds(&self) -> f64 {
        (self.last_timestamp() - self.timestamp_start) as f64 / 1_000_000.0
    }

    /// Get all message names that have data
    pub fn available_messages(&self) -> Vec<&str> {
        self.subscriptions
            .values()
            .filter(|sub| !sub.data.is_empty())
            .map(|sub| sub.message_name.as_str())
            .collect()
    }

    /// Get message frequency for a given message name
    pub fn message_frequency(&self, message_name: &str) -> Option<f64> {
        let data = self.get_subscription_messages(message_name)?;
        if data.len() < 2 {
            return None;
        }

        // Get first timestamp from first message
        let first_ts = if let Some(first_msg) = data.first() {
            if let Some(ULogValue::UInt64(ts)) = first_msg.first() {
                *ts
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Get last timestamp from last message
        let last_ts = if let Some(last_msg) = data.last() {
            if let Some(ULogValue::UInt64(ts)) = last_msg.first() {
                *ts
            } else {
                return None;
            }
        } else {
            return None;
        };

        let duration_s = (last_ts - first_ts) as f64 / 1_000_000.0;
        if duration_s <= 0.0 {
            return None;
        }

        Some(data.len() as f64 / duration_s)
    }
}
