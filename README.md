# Rust ULog Parser

A robust Rust implementation of a parser for the ULog file format, commonly used in PX4 flight stack for logging system data. This parser provides a safe, efficient way to read and process ULog files with strong type checking and error handling.

## Project Status

This project is currently in development, it basically works but I'll be making changes to the interface as I go.

- [x] Parse ULog files and extract all messages
- [ ] Make a nice interface for the parser (in progress)
- [ ] Documentation
- [ ] Add cli features for the binary (right now it just gives some summary data for a sanity check)
- [ ] Add tests (Parity tests are in progress, using pyulog for comparison. The holdup is munging the data to match the format from pyulog)
- [ ] Benchmarking

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
ulog_rs = "0.0.1"
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

The `ULog` struct provides the main interface for parsing ULog files:

```rust
use ulog_rs::ulog::ULog;
let ulog = ULog::parse_file("sample.ulg")?;

// Access header information
println!("Version: {}", ulog.version);
println!("Timestamp: {} μs", ulog.timestamp);
// Get logged messages
for message in &ulog.logged_messages() {
    println!("[{}] {}", message.timestamp, message.message);
}
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

MIT License
