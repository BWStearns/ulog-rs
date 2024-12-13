use serde::Serialize;
use serde_derive::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use ulog_rs::{ULogParser, ULogValue};

#[derive(Serialize)]
struct Profile {
    file: String,
    datasets: HashMap<String, HashMap<String, usize>>,
    logged_messages: Vec<LoggedMessageProfile>,
    info_messages: HashMap<String, String>,
    default_params: HashMap<String, ParamValue>,
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

#[derive(Serialize)]
enum ParamValue {
    Float(f32),
    Int(i32),
}

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
        let topic_counts =
            HashMap::from_iter([(subscription.message_name.clone(), subscription.data.len())]);
        profile
            .datasets
            .insert(subscription.message_name.clone(), topic_counts);
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
            ULogValue::Float(f) => ParamValue::Float(*f),
            ULogValue::Int32(i) => ParamValue::Int(*i),
            _ => continue,
        };
        profile.initial_params.insert(key.clone(), value);
    }

    // Process default parameters
    for (key, param) in parser.default_params() {
        let value = match &param.value {
            ULogValue::Float(f) => ParamValue::Float(*f),
            ULogValue::Int32(i) => ParamValue::Int(*i),
            _ => continue,
        };
        profile.default_params.insert(key.clone(), value);
    }

    // Process changed parameters
    for (key, params) in parser.changed_params() {
        let values = params
            .iter()
            .map(|param| match &param.value {
                ULogValue::Float(f) => ParamValue::Float(*f),
                ULogValue::Int32(i) => ParamValue::Int(*i),
                _ => unreachable!(),
            })
            .collect();
        profile.changed_params.insert(key.clone(), values);
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
