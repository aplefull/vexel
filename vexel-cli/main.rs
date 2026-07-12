mod gui;
mod writer;

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;
use glob::glob;
#[cfg(unix)]
use libc::{rlimit, setrlimit, RLIMIT_AS};
use vexel::{LogLevel, Vexel};
use writer::Writer;

const AFTER_HELP: &str = "\
The PATH argument supports glob patterns (e.g. \"*.png\") to process multiple files at once. When no
output format is specified, files are written as JXL next to the source with the same name. Use
--output-dir to specify an output directory, or --output to specify an exact path for a single file.

Examples:
  vexel image.png
  vexel -f webp image.png
  vexel image.png -O output.webp
  vexel \"*.png\" -f webp -o ./converted
  vexel --frames -f webp image.gif
  vexel --info image.png
  vexel --gui image.png
  vexel --gui ./images\
";

#[derive(Parser, Debug)]
#[clap(name = "vexel", after_help = AFTER_HELP)]
struct Cli {
    #[arg(required = true)]
    path: String,

    #[arg(short, long, value_parser = ["ppm", "pam", "webp", "jxl"], help = "Output format [default: jxl]")]
    format: Option<String>,

    #[arg(short = 'o', long = "output-dir", help = "Output directory for converted files")]
    output_dir: Option<String>,

    #[arg(short = 'O', long = "output", help = "Output file path (single file only)", conflicts_with = "output_dir")]
    output: Option<String>,

    #[arg(long, help = "Display the image")]
    gui: bool,

    #[arg(long, help = "Print format-specific metadata for the image")]
    info: bool,

    #[arg(long, help = "Decode the image without writing to a file")]
    void: bool,

    #[arg(long, help = "Write each frame as a separate file")]
    frames: bool,

    #[arg(long, value_parser = ["error", "warn", "debug", "none"], default_value = "error", help = "Minimum log level to display")]
    log_level: String,

    #[arg(
        long,
        default_value = "4096",
        help = "Maximum memory usage in MiB (0 = unlimited). Decoder will abort if this limit is exceeded."
    )]
    memory_limit: u64,

    #[arg(long, help = "Disable all decode limits (pixel-data allocation cap and pixel-count cap)")]
    no_limits: bool,
}

fn build_limits(cli: &Cli) -> vexel::Limits {
    if cli.no_limits {
        vexel::Limits::no_limits()
    } else {
        vexel::Limits::default()
    }
}

fn get_files(path: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".."));
    let absolute_pattern = if Path::new(path).is_relative() {
        base_dir.join(path).to_string_lossy().into_owned()
    } else {
        path.to_string()
    };

    for entry in glob(&absolute_pattern).expect("Failed to read glob pattern") {
        match entry {
            Ok(path) => {
                if !path.is_file() {
                    continue;
                }

                files.push(path);
            }
            Err(e) => println!("{:?}", e),
        }
    }

    files
}

fn get_output_path(file: &Path, output_dir: Option<&str>, format: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let file_stem = file.file_stem().ok_or("Invalid file name")?.to_str().ok_or("Invalid file stem")?;

    let output_path = if let Some(dir) = output_dir {
        let output_dir = Path::new(dir);

        if !output_dir.exists() {
            fs::create_dir_all(output_dir)?;
        }

        let output_dir =
            if output_dir.is_relative() { std::env::current_dir()?.join(output_dir) } else { output_dir.to_path_buf() };

        output_dir.join(format!("{}.{}", file_stem, format))
    } else {
        file.parent().unwrap_or_else(|| Path::new("..")).join(format!("{}.{}", file_stem, format))
    };

    Ok(output_path)
}

fn process_file(file: &Path, cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    println!("File: {}", file.display());

    let mut decoder = Vexel::open(file)?;
    decoder.set_limits(build_limits(cli));

    if cli.void {
        let _ = decoder.decode();
        return Ok(());
    }

    if cli.info {
        let _ = decoder.decode();
        let info = decoder.get_info();
        println!("{}", info);
        return Ok(());
    }

    let image = decoder.decode()?;

    let format = cli.format.as_deref().unwrap_or("jxl");
    let output_path = if let Some(output) = cli.output.as_deref() {
        let p = Path::new(output);
        if p.is_relative() {
            std::env::current_dir()?.join(p)
        } else {
            p.to_path_buf()
        }
    } else {
        get_output_path(file, cli.output_dir.as_deref(), format)?
    };

    if let Some(parent) = output_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
        }
    }

    println!("Writing to: {}", output_path.display());
    if cli.frames {
        Writer::write_frames(&output_path, format, image)?;
    } else {
        Writer::write_image(&image, format, &output_path)?;
    }

    Ok(())
}

fn apply_memory_limit(limit_mib: u64) {
    #[cfg(unix)]
    {
        if limit_mib == 0 {
            return;
        }

        let limit_bytes = limit_mib.saturating_mul(1024 * 1024);
        let rl = rlimit { rlim_cur: limit_bytes, rlim_max: limit_bytes };

        unsafe {
            if setrlimit(RLIMIT_AS, &rl) != 0 {
                eprintln!("Warning: failed to set memory limit");
            }
        }
    }

    #[cfg(not(unix))]
    let _ = limit_mib;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    apply_memory_limit(cli.memory_limit);

    let log_level = match cli.log_level.as_str() {
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warning,
        "none" => LogLevel::None,
        _ => LogLevel::Error,
    };
    vexel::set_log_level(log_level);

    if cli.gui {
        let (files, start_index) = gui::collect_gui_files(&cli.path);
        if files.is_empty() {
            eprintln!("No files found matching path: {}", cli.path);
            return Ok(());
        }

        gui::display_image(files, start_index, build_limits(&cli))?;

        return Ok(());
    }

    let files = get_files(&cli.path);

    if files.is_empty() {
        eprintln!("No files found matching pattern: {}", cli.path);
        return Ok(());
    }

    if cli.output.is_some() && files.len() > 1 {
        eprintln!("--output cannot be used with multiple files; use --output-dir instead");
        return Ok(());
    }

    for file in files {
        if let Err(err) = process_file(&file, &cli) {
            eprintln!("Error processing file: {:?}", err);
            continue;
        }
    }

    Ok(())
}
