use std::fs::File;
// use std::path::Path;

use ulog_parser::ULogParser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the file path from the home directory
    let home = dirs::home_dir().expect("Could not find home directory");
    let file_path = home.join("Downloads").join("sample-small.ulg");

    println!("Opening ULog file: {}", file_path.display());

    // Open the file
    let file = File::open(&file_path)?;

    // Create parser instance
    let mut parser = ulog_parser::ULogParser::new(file)?;

    // Print header information
    println!("ULog Header:");
    println!("  Version: {}", parser.header().version);
    println!("  Timestamp: {} μs", parser.header().timestamp);

    // Parse definition section
    println!("\nParsing definitions section...");
    parser.parse_definitions()?;
    // Join the initial params into a single string
    // let init_params_str = parser
    //     .initial_params()
    //     .iter()
    //     .map(|(_, param)| format!("{}: {:?}", param.key, param.value))
    //     .collect::<Vec<_>>()
    //     .join(", ");
    // println!("Initial parameters: {}", init_params_str);

    // Print format definitions
    println!("\nFormat definitions found:");
    for (name, format) in parser.formats() {
        println!("\nMessage: {}", name);
        for field in &format.fields {
            let array_size_annotation = field
                .array_size
                .map_or(String::new(), |size| format!("[{}]", size));
            println!(
                "  - {}{} {}",
                field.field_type, array_size_annotation, field.field_name
            );
        }
    }

    // Parse data section
    println!("\nParsing data section...");
    parser.parse_data()?;

    // Print summary of logged messages
    println!("\nSummary of logged messages:");
    let logged_messages = parser.logged_messages();
    println!("Found {} log messages", logged_messages.len());

    // Count messages by log level
    use std::collections::HashMap;
    let mut level_counts = HashMap::new();
    for msg in logged_messages {
        *level_counts.entry(msg.log_level).or_insert(0) += 1;
    }

    for (level, count) in level_counts.iter() {
        println!(
            "{}: {} messages",
            ULogParser::<File>::log_level_to_string(*level),
            count
        );
    }

    // Print summary of subscriptions
    println!("\nSummary of subscriptions:");
    let subscriptions = parser.subscriptions();
    println!("Found {} subscriptions", subscriptions.len());
    for (sub_id, subscription) in subscriptions {
        println!(
            "Subscription ID: {}, Message Name: {}, Multi ID: {}, Data Length: {}",
            sub_id,
            subscription.message_name,
            subscription.multi_id,
            subscription.data.len()
        );
    }

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
