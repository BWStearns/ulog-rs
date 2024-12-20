use std::fs::File;
// use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_path = "./test_data/sample-small.ulg";
    // Open the file
    let file = File::open(&file_path)?;
    // Create parser instance
    let parser = ulog_rs::ULogParser::parse_reader(file)?;
    // Print header information
    println!("ULog Header:");
    println!("  Version: {}", parser.header().version);
    println!("  Timestamp: {} μs", parser.header().timestamp);
    println!("  Final Timestamp: {} μs", parser.last_timestamp());
    for message in parser.logged_messages() {
        println!("[{}] {}", message.timestamp, message.message);
    }

    println!("\n\n######################\n\n");

    let file_path = "./test_data/sample.ulg";
    // Open the file
    let file = File::open(&file_path)?;
    // Create parser instance
    let new_parser = ulog_rs::ULogParser::parse_reader(file)?;
    // Print header information
    println!("ULog Header:");
    println!("  Version: {}", new_parser.header().version);
    println!("  Timestamp: {} μs", new_parser.header().timestamp);
    println!("  Final Timestamp: {} μs", new_parser.last_timestamp());
    for message in new_parser.logged_messages() {
        println!("[{}] {}", message.timestamp, message.message);
    }
    // Parse definition section
    // Join the initial params into a single string
    // let init_params_str = parser
    //     .initial_params()
    //     .iter()
    //     .map(|(_, param)| format!("{}: {:?}", param.key, param.value))
    //     .collect::<Vec<_>>()
    //     .join(", ");
    // println!("Initial parameters: {}", init_params_str);

    // // Print format definitions
    // println!("\nFormat definitions found:");
    // for (name, format) in parser.formats() {
    //     println!("\nMessage: {}", name);
    //     for field in &format.fields {
    //         let array_size_annotation = field
    //             .array_size
    //             .map_or(String::new(), |size| format!("[{}]", size));
    //         println!(
    //             "  - {}{} {}",
    //             field.field_type, array_size_annotation, field.field_name
    //         );
    //     }
    // }

    // // Parse data section
    // println!("\nParsing data section...");
    // parser.parse_data()?;

    // // Print summary of logged messages
    // println!("\nSummary of logged messages:");
    // let logged_messages = parser.logged_messages();
    // println!("Found {} log messages", logged_messages.len());

    // // Count messages by log level
    // use std::collections::HashMap;
    // let mut level_counts = HashMap::new();
    // for msg in logged_messages {
    //     *level_counts.entry(msg.log_level).or_insert(0) += 1;
    // }

    // for (level, count) in level_counts.iter() {
    //     println!(
    //         "{}: {} messages",
    //         ULogParser::<File>::log_level_to_string(*level),
    //         count
    //     );
    // }

    // // Print summary of subscriptions
    // println!("\nSummary of subscriptions:");
    // let subscriptions = parser.subscriptions();
    // println!("Found {} subscriptions", subscriptions.len());
    // for (sub_id, subscription) in subscriptions {
    //     println!(
    //         "Subscription ID: {}, Message Name: {}, Multi ID: {}, Data Length: {}",
    //         sub_id,
    //         subscription.message_name,
    //         subscription.multi_id,
    //         subscription.data.len()
    //     );
    // }

    // Print out the dropout details
    // println!("\nDropout details:");
    // println!("{:?}", parser.dropout_details());

    // Print out the default params
    // println!("\nDefault Parameters:");
    // for (key, value) in parser.default_params() {
    //     println!("{:?}: {:?}", key, value);
    // }

    // Print out the multi messages
    // println!("\nMulti Messages:");
    // for (_, multi_msg) in parser.multi_messages() {
    //     println!("Multi ID: {}", multi_msg[0].key);
    //     let msg = multi_msg.combine_values().unwrap();
    //     println!("{:?}", msg);
    // }

    // println!("\nSpecific Messages:");
    // let specific_subscription = parser
    //     .subscriptions()
    //     .iter()
    //     .find(|(_, sub)| sub.message_name == "telemetry_status")
    //     .expect("Could not find telemetry_status subscription")
    //     .1;
    // for message in specific_subscription.data.iter() {
    //     println!("{:?}", message);
    // }

    // println!("{:?}", specific_subscription);

    Ok(())
}
