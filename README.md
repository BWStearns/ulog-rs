# Rust ULog Parser

A robust Rust implementation of a parser for the ULog file format, commonly used in PX4 flight stack for logging system data. This parser provides a safe, efficient way to read and process ULog files with strong type checking and error handling.

## Features

- Complete implementation of the ULog file format specification
- Support for all ULog message types including:
  - Data messages
  - Format messages
  - Parameter messages
  - Logged messages (both plain and tagged)
  - Multi messages
  - Subscription messages
  - Dropout tracking
- Safe handling of nested message types
- Comprehensive error handling
- Zero-copy parsing where possible
- Support for appended data sections

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
ulog_rs = "0.1.0"
```

### Basic Example

```rust
use std::fs::File;
use ulog_rs::ULogParser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open a ULog file
    let file = File::open("sample.ulg")?;
    
    // Create parser instance
    let parser = ULogParser::parse_reader(file)?;
    
    // Access header information
    println!("ULog Header:");
    println!("  Version: {}", parser.header().version);
    println!("  Timestamp: {} μs", parser.header().timestamp);
    println!("  Final Timestamp: {} μs", parser.last_timestamp());
    
    // Access logged messages
    for message in parser.logged_messages() {
        println!("[{}] {}", message.timestamp, message.message);
    }
    
    Ok(())
}
```

## Project Structure

The project is organized into several modules, each handling specific aspects of ULog parsing:

- `lib.rs`: Core parser implementation and type definitions
- `data_message.rs`: Handles data message parsing
- `dropout_message.rs`: Manages dropout tracking and statistics
- `format_message.rs`: Processes message format definitions
- `info_message.rs`: Handles info message parsing
- `logged_message.rs`: Manages regular logged messages
- `multi_message.rs`: Handles multi-part messages
- `parameter_message.rs`: Processes parameter messages
- `subscription_message.rs`: Manages message subscriptions
- `tagged_logged_message.rs`: Handles tagged log messages

## API Overview

### Main Parser Interface

The `ULogParser` struct provides the main interface for parsing ULog files:

```rust
pub struct ULogParser<R: Read> {
    // ... internal fields ...
}

impl<R: Read> ULogParser<R> {
    // Create a new parser and parse the entire file
    pub fn parse_reader(reader: R) -> Result<ULogParser<R>, ULogError>;
    
    // Access various components
    pub fn header(&self) -> &ULogHeader;
    pub fn formats(&self) -> &HashMap<String, FormatMessage>;
    pub fn subscriptions(&self) -> &HashMap<u16, SubscriptionMessage>;
    pub fn logged_messages(&self) -> &[LoggedMessage];
    pub fn info_messages(&self) -> &HashMap<String, InfoMessage>;
    pub fn initial_params(&self) -> &HashMap<String, ParameterMessage>;
    pub fn multi_messages(&self) -> &HashMap<String, Vec<MultiMessage>>;
    pub fn default_params(&self) -> &HashMap<String, DefaultParameterMessage>;
    pub fn dropout_details(&self) -> &DropoutStats;
    pub fn last_timestamp(&self) -> u64;
}
```

### Error Handling

The parser uses a custom error type `ULogError` for comprehensive error handling:

```rust
pub enum ULogError {
    Io(std::io::Error),
    InvalidMagic,
    UnsupportedVersion(u8),
    InvalidMessageType(u8),
    InvalidString,
    InvalidTypeName(String),
    ParseError(String),
    IncompatibleFlags(Vec<u8>),
}
```

## Message Types

### Data Messages

Data messages contain the actual logged data and are accessed through subscriptions:

```rust
// Subscribe to a message type
let subscription = parser.subscriptions().get(&msg_id)?;

// Access the data
for data_point in &subscription.data {
    // Process data...
}
```

### Parameter Messages

The parser maintains both initial parameters and parameter changes:

```rust
// Access initial parameters
for (key, param) in parser.initial_params() {
    println!("{}: {:?}", key, param.value);
}

// Access default parameters
for (key, param) in parser.default_params() {
    println!("{}: {:?}", key, param.value);
}
```

### Logged Messages

Both regular and tagged logged messages are supported:

```rust
// Regular logged messages
for msg in parser.logged_messages() {
    println!("[{}] {}", msg.timestamp, msg.message);
}

// Tagged logged messages
for (tag, messages) in parser.logged_messages_tagged() {
    for msg in messages {
        println!("[{}][{}] {}", tag, msg.timestamp, msg.message);
    }
}
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

[Insert your chosen license here]