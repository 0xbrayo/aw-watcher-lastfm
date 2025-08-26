use std::fs::{create_dir_all, metadata, remove_file, rename};
use std::path::PathBuf;

use fern::colors::{Color, ColoredLevelConfig};

pub fn get_config_dir() -> Option<PathBuf> {
    let mut dir = dirs::config_dir()?;
    dir.push("activitywatch");
    dir.push("aw-watcher-lastfm");
    create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn get_log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let dir = {
        let mut d = dirs::data_local_dir()?;
        d.push("activitywatch");
        d.push("activitywatch");
        d.push("aw-watcher-lastfm");
        d
    };

    #[cfg(target_os = "macos")]
    let dir = {
        let mut d = dirs::home_dir()?;
        d.push("Library");
        d.push("Logs");
        d.push("activitywatch");
        d.push("aw-watcher-lastfm");
        d
    };

    #[cfg(target_os = "linux")]
    let dir = {
        let mut d = dirs::cache_dir()?;
        d.push("activitywatch");
        d.push("log");
        d.push("aw-watcher-lastfm");
        d
    };

    create_dir_all(&dir).ok()?;
    Some(dir)
}

pub fn get_config_path() -> Option<PathBuf> {
    get_config_dir().map(|mut path| {
        path.push("config.yaml");
        path
    })
}

const MAX_LOG_SIZE: u64 = 32 * 1024 * 1024; // 32MB

fn rotate_log_if_needed(log_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if !log_path.exists() {
        return Ok(());
    }

    let metadata = metadata(log_path)?;
    if metadata.len() > MAX_LOG_SIZE {
        let old_log_path = log_path.with_file_name(
            log_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace(".log", "-old.log"),
        );

        // If old log exists, remove it
        if old_log_path.exists() {
            remove_file(&old_log_path)?;
        }

        // Move current log to old log
        rename(log_path, &old_log_path)?;
    }

    Ok(())
}

pub fn setup_logger(module: &str, testing: bool, verbose: bool) -> Result<(), fern::InitError> {
    let log_dir = get_log_dir().expect("Unable to get log dir to store logs in");
    let filename = if !testing {
        format!("{}.log", module)
    } else {
        format!("{}-testing.log", module)
    };

    let logfile_path = log_dir.join(filename);

    // Rotate log if needed
    rotate_log_if_needed(&logfile_path).expect("Failed to rotate log file");

    let colors = ColoredLevelConfig::new()
        .debug(Color::White)
        .info(Color::Green)
        .warn(Color::Yellow)
        .error(Color::Red);

    let default_log_level = if testing || verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let log_level = std::env::var("LOG_LEVEL").map_or(default_log_level, |level| {
        match level.to_lowercase().as_str() {
            "trace" => log::LevelFilter::Trace,
            "debug" => log::LevelFilter::Debug,
            "info" => log::LevelFilter::Info,
            "warn" => log::LevelFilter::Warn,
            "error" => log::LevelFilter::Error,
            _ => default_log_level,
        }
    });

    // Use non-colored formatter for file output
    let file_formatter = |out: fern::FormatCallback,
                          message: &std::fmt::Arguments,
                          record: &log::Record| {
        out.finish(format_args!(
            "[{}][{}][{}]: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.target(),
            message,
        ))
    };

    // Compute crate target ("aw-watcher-lastfm" -> "aw_watcher_lastfm")
    let crate_target = env!("CARGO_PKG_NAME").replace('-', "_");

    let dispatch = fern::Dispatch::new()
        .level(log::LevelFilter::Trace) // Always capture everything for file logging
        // Formatting (console remains colored)
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{}][{}][{}]: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                colors.color(record.level()),
                record.target(),
                message,
            ))
        })
        // Console output with configurable log level
        .chain(
            fern::Dispatch::new()
                .level(log_level) // Respect user's log level for console
                .chain(std::io::stdout()),
        )
        // File output only for our module at TRACE level
        .chain(
            fern::Dispatch::new()
                .level(log::LevelFilter::Off) // Disable all logs by default
                .level_for(crate_target.to_owned(), log::LevelFilter::Trace) // Only our module
                .format(file_formatter)
                .chain(fern::log_file(logfile_path)?),
        );

    dispatch.apply()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_dirs() {
        get_config_dir().unwrap();
        get_log_dir().unwrap();
        get_config_path().unwrap();
    }

    #[ignore]
    #[test]
    fn test_setup_logger() {
        setup_logger("aw-watcher-lastfm", true, true).unwrap();
    }
}
