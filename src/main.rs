use std::{fs::File, path::PathBuf};

use clap::{command, Parser};
use ulog_rs::ulog::ULog;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to ULog file
    #[arg(short, long)]
    file: PathBuf,

    /// Show message statistics
    #[arg(short, long)]
    stats: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let file = File::open(&args.file)?;
    let ulog = ULog::parse(file)?;

    println!("ULog File Analysis");
    println!("-----------------");
    println!("Duration: {:.2} seconds", ulog.duration_seconds());

    if let Some(sys_name) = ulog.system_name() {
        println!("System: {}", sys_name);
    }
    if let Some(hw_ver) = ulog.hardware_version() {
        println!("Hardware: {}", hw_ver);
    }
    if let Some(sw_ver) = ulog.software_version() {
        println!("Software: {}", sw_ver);
    }

    println!("\nDropout Statistics:");
    println!("Total dropouts: {}", ulog.dropout_count());
    println!(
        "Total dropout duration: {} ms",
        ulog.total_dropout_duration()
    );
    println!("Data loss: {:.2}%", ulog.data_loss_percentage());

    if args.stats {
        println!("\nMessage Statistics:");
        println!("{:<30} {:<10} {:<10}", "Message", "Count", "Freq (Hz)");
        println!("{:-<52}", "");

        for msg_name in ulog.available_messages() {
            if let Some(data) = ulog.get_subscription_messages(msg_name) {
                let freq = ulog.message_frequency(msg_name).unwrap_or(0.0);
                println!("{:<30} {:<10} {:.2}", msg_name, data.len(), freq);
            }
        }

        println!("\nParameter Changes:");
        for (param_name, changes) in ulog.changed_parameters.iter() {
            println!("{}: {} changes", param_name, changes.len());
        }
    }

    println!("{:#?}", ulog);

    Ok(())
}
