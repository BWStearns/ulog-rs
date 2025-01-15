use serde::Serialize;
use serde_derive::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use ulog_rs::{parameter_message::DefaultType, ULogParser, ULogValue};

#[derive(Serialize)]
struct Profile {
    file: String,
    datasets: HashMap<String, HashMap<String, usize>>,
    logged_messages: Vec<LoggedMessageProfile>,
    info_messages: HashMap<String, String>,
    default_params: HashMap<String, HashMap<String, ParamValue>>, // Group -> Params
    initial_params: HashMap<String, ParamValue>,
    changed_params: HashMap<String, Vec<ParamValue>>,
    dropout_details: Vec<DropoutProfile>,
}

#[derive(Serialize)]
struct LoggedMessageProfile {
    log_level: u8,
    timestamp: u64,
    message: String,
}

#[derive(Serialize)]
struct DropoutProfile {
    timestamp: u64,
    duration: u16,
}

// Use serde_json::Value to match Python's flexibility with numeric types
type ParamValue = serde_json::Value;

pub fn profile_ulog<P: AsRef<Path>>(path: P) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(path.as_ref())?;
    let parser = ULogParser::parse_reader(file)?;

    // Create profile structure
    let mut profile = Profile {
        file: path
            .as_ref()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string(),
        datasets: HashMap::new(),
        logged_messages: Vec::new(),
        info_messages: HashMap::new(),
        default_params: HashMap::new(),
        initial_params: HashMap::new(),
        changed_params: HashMap::new(),
        dropout_details: Vec::new(),
    };

    // Process subscriptions (datasets)
    for (msg_id, subscription) in parser.subscriptions() {
        let mut field_counts = HashMap::new();
        for field in &subscription.format.fields {
            if field.field_name.starts_with("_padding") {
                continue;
            }

            let field_name = if let Some(array_size) = field.array_size {
                if field.field_name.contains('[') {
                    field.field_name.clone()
                } else {
                    format!("{}[{}]", field.field_name, array_size - 1)
                }
            } else {
                field.field_name.clone()
            };

            field_counts.insert(field_name, subscription.data[0].len());
        }
        profile
            .datasets
            .insert(subscription.message_name.clone(), field_counts);
    }

    // Process logged messages
    profile.logged_messages = parser
        .logged_messages()
        .iter()
        .map(|msg| LoggedMessageProfile {
            log_level: msg.log_level,
            timestamp: msg.timestamp,
            message: msg.message.clone(),
        })
        .collect();

    // Process info messages
    for (key, info) in parser.info_messages() {
        if let Some(value) = info.as_string() {
            profile.info_messages.insert(key.clone(), value.to_string());
        }
    }

    // Process parameters
    for (key, param) in parser.initial_params() {
        let value = match &param.value {
            ULogValue::Float(f) => json!(f),
            ULogValue::Int32(i) => json!(i),
            _ => continue,
        };
        profile.initial_params.insert(key.clone(), value);
    }

    // Process default parameters with group support
    // Group 0 and 1 match Python implementation
    let default_groups = [0, 1];
    for group in default_groups {
        let mut group_params = HashMap::new();
        for (key, param) in parser.default_params() {
            if param.default_types.contains(&DefaultType::SystemWide) && group == 0
                || param.default_types.contains(&DefaultType::Configuration) && group == 1
            {
                let value = match &param.value {
                    ULogValue::Float(f) => json!(f),
                    ULogValue::Int32(i) => json!(i),
                    _ => continue,
                };
                group_params.insert(key.clone(), value);
            }
        }
        profile
            .default_params
            .insert(group.to_string(), group_params);
    }

    // Process changed parameters
    for (key, params) in parser.changed_params() {
        let values: Vec<serde_json::Value> = params
            .iter()
            .filter_map(|param| match &param.value {
                ULogValue::Float(f) => Some(json!(f)),
                ULogValue::Int32(i) => Some(json!(i)),
                _ => None,
            })
            .collect();
        if !values.is_empty() {
            profile.changed_params.insert(key.clone(), values);
        }
    }

    // Process dropouts
    profile.dropout_details = parser
        .dropout_details()
        .dropouts
        .iter()
        .map(|dropout| DropoutProfile {
            timestamp: dropout.timestamp,
            duration: dropout.duration,
        })
        .collect();

    // Write to file
    let output_path = format!("parity/results/{}.rs.json", profile.file);
    let output_file = File::create(output_path)?;
    serde_json::to_writer_pretty(output_file, &profile)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let test_files = vec!["test_data/sample.ulg", "test_data/sample-small.ulg"];

    for file in test_files {
        profile_ulog(file)?;
    }

    Ok(())
}
