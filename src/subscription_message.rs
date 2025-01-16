use std::{collections::HashMap, io::Read};

use byteorder::{LittleEndian, ReadBytesExt};
use serde_derive::Serialize;

use crate::{format_message::FormatMessage, MessageHeader, ULogError, ULogParser, ULogValue};

// Subscription message
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionMessage {
    pub multi_id: u8,
    pub msg_id: u16,
    pub message_name: String,
    pub data: Vec<Vec<ULogValue>>,
    pub format: FormatMessage,
}

impl SubscriptionMessage {
    pub fn insert_data(&mut self, data: Vec<ULogValue>) {
        // Iterate over the data and format fields, and insert the data into the data vector
        for i in 0..data.len() {
            self.data[i].push(data[i].clone());
        }
    }
}

impl<R: Read> ULogParser<R> {
    pub fn subscriptions(&self) -> &HashMap<u16, SubscriptionMessage> {
        &self.subscriptions
    }

    fn read_subscription(&mut self, msg_size: u16) -> Result<SubscriptionMessage, ULogError> {
        let multi_id = self.reader.read_u8()?;
        let msg_id = self.reader.read_u16::<LittleEndian>()?;
        let name = self.read_string(msg_size as usize - 3)?; // -3 for multi_id and msg_id
        let format = self
            .formats
            .get(&name)
            .ok_or_else(|| ULogError::ParseError(format!("Unknown format name: {}", name)))?;
        Ok(SubscriptionMessage {
            multi_id,
            msg_id,
            message_name: name,
            // Initialize data as an empty vector of n empty vectors (n = number of fields in format)
            data: vec![Vec::new(); format.fields.len()],
            format: format.clone(),
        })
    }

    pub fn handle_subscription_message(&mut self, header: &MessageHeader) -> Result<(), ULogError> {
        let subscription = self.read_subscription(header.msg_size)?;
        self.subscriptions.insert(subscription.msg_id, subscription);
        Ok(())
    }
}
