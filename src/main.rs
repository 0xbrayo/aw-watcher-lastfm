use aw_client_rust::blocking::AwClient;
use aw_models::Event;
use chrono::{DateTime, TimeDelta, Utc};
use crossbeam_channel::{self, RecvTimeoutError, TryRecvError};
use dirs::config_dir;
use env_logger::Env;
use log::{debug, info, warn};
use regex::Regex;
use reqwest;
use serde_json::{Map, Value};
use serde_yaml;
use std::env;
use std::fs::{DirBuilder, File};
use std::io::prelude::*;
use std::process::exit;
use std::time::{Duration, Instant};

fn parse_time_string(time_str: &str) -> Option<TimeDelta> {
    let re = Regex::new(r"^(\d+)([dhm])$").unwrap();
    if let Some(caps) = re.captures(time_str) {
        let amount: i64 = caps.get(1)?.as_str().parse().ok()?;
        let unit = caps.get(2)?.as_str();

        match unit {
            "d" => Some(TimeDelta::days(amount)),
            "h" => Some(TimeDelta::hours(amount)),
            "m" => Some(TimeDelta::minutes(amount)),
            _ => None,
        }
    } else {
        None
    }
}

fn sync_historical_data(
    client: &reqwest::blocking::Client,
    aw_client: &AwClient,
    username: &str,
    apikey: &str,
    from_time: TimeDelta,
) -> Result<(), Box<dyn std::error::Error>> {
    let from_timestamp = (Utc::now() - from_time).timestamp();
    let url = format!(
        "https://ws.audioscrobbler.com/2.0/?method=user.getrecenttracks&user={}&api_key={}&format=json&limit=200&from={}",
        username, apikey, from_timestamp
    );

    let response = client.get(&url).send()?.error_for_status()?;
    let v: Value = response.json()?;
    if v.get("error").is_some() {
        let msg = v
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        return Err(format!("last.fm API error: {}", msg).into());
    }
    if let Some(tracks) = v["recenttracks"]["track"].as_array() {
        debug!("Syncing {} historical tracks...", tracks.len());
        for track in tracks.iter().rev() {
            let mut event_data: Map<String, Value> = Map::new();

            event_data.insert("title".to_string(), track["name"].to_owned());
            event_data.insert("artist".to_string(), track["artist"]["#text"].to_owned());
            event_data.insert("album".to_string(), track["album"]["#text"].to_owned());

            // Get timestamp from the track
            if let Some(date) = track["date"]["uts"].as_str() {
                if let Ok(timestamp) = date.parse::<i64>() {
                    let event = Event {
                        id: None,
                        timestamp: DateTime::<Utc>::from_timestamp(timestamp, 0)
                            .expect("Invalid timestamp"),
                        duration: TimeDelta::seconds(30),
                        data: event_data,
                    };

                    aw_client
                        .insert_event("aw-watcher-lastfm", &event)
                        .unwrap_or_else(|e| {
                            warn!("Error inserting historical event: {:?}", e);
                        });
                }
            }
        }
        debug!("Historical sync completed!");
    }

    Ok(())
}

fn get_config_path() -> Option<std::path::PathBuf> {
    config_dir().map(|mut path| {
        path.push("activitywatch");
        path.push("aw-watcher-lastfm");
        path
    })
}

fn run_loop(
    client: reqwest::blocking::Client,
    url: String,
    aw_client: AwClient,
    polling_time: TimeDelta,
    polling_interval: u64,
) {
    let interval_duration = Duration::from_secs(polling_interval);
    let (tx, rx) = crossbeam_channel::unbounded();

    // Set up signal handler for graceful shutdown
    ctrlc::set_handler(move || {
        info!("Received interrupt signal, shutting down gracefully...");
        let _ = tx.send(());
    })
    .expect("Error setting Ctrl+C handler");

    loop {
        let start_time = Instant::now();

        handle_lastfm_update(&client, &url, &aw_client, polling_time, polling_interval);

        let elapsed = start_time.elapsed();

        // Only sleep if we haven't already exceeded the interval
        if elapsed < interval_duration {
            let sleep_duration = interval_duration - elapsed;

            // Use channel to wait for either timeout or shutdown signal
            match rx.recv_timeout(sleep_duration) {
                Ok(()) => {
                    // Received shutdown signal
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    // Normal timeout, continue loop
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    // Channel disconnected, should not happen but break anyway
                    break;
                }
            }
        } else {
            // Check for shutdown signal without blocking
            match rx.try_recv() {
                Ok(()) => break,
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Disconnected) => break,
            }
        }
    }

    info!("Shutdown complete.");
}

fn handle_lastfm_update(
    client: &reqwest::blocking::Client,
    url: &str,
    aw_client: &AwClient,
    polling_time: TimeDelta,
    polling_interval: u64,
) {
    let response = client.get(url).send();
    let v: Value = match response {
        Ok(response) => match response.json() {
            Ok(json) => json,
            Err(e) => {
                warn!("Error parsing json: {}", e);
                return;
            }
        },
        Err(_) => {
            warn!("Error connecting to last.fm");
            return;
        }
    };

    if v["recenttracks"]["track"][0]["@attr"]["nowplaying"].as_str() != Some("true") {
        debug!("No song is currently playing");
        return;
    }

    let mut event_data: Map<String, Value> = Map::new();
    debug!(
        "Track: {} - {}",
        v["recenttracks"]["track"][0]["name"], v["recenttracks"]["track"][0]["artist"]["#text"]
    );

    event_data.insert(
        "title".to_string(),
        v["recenttracks"]["track"][0]["name"].to_owned(),
    );
    event_data.insert(
        "artist".to_string(),
        v["recenttracks"]["track"][0]["artist"]["#text"].to_owned(),
    );
    event_data.insert(
        "album".to_string(),
        v["recenttracks"]["track"][0]["album"]["#text"].to_owned(),
    );

    let event = Event {
        id: None,
        timestamp: Utc::now(),
        duration: polling_time,
        data: event_data,
    };

    aw_client
        .heartbeat("aw-watcher-lastfm", &event, polling_interval as f64)
        .unwrap_or_else(|e| {
            warn!("Error sending heartbeat: {:?}", e);
        });
}

fn main() {
    let config_dir = get_config_path().expect("Unable to get config path");
    let config_path = config_dir.join("config.yaml");

    let args: Vec<String> = env::args().collect();
    let mut port: u16 = 5600;
    let mut sync_duration: Option<TimeDelta> = None;

    let mut idx = 1;
    while idx < args.len() {
        match args[idx].as_str() {
            "--port" => {
                if idx + 1 < args.len() {
                    port = args[idx + 1].parse().expect("Invalid port number");
                    idx += 2;
                } else {
                    panic!("--port requires a value");
                }
            }
            "--testing" => {
                port = 5699;
                idx += 1;
            }
            "--sync" => {
                if idx + 1 < args.len() {
                    sync_duration = Some(
                        parse_time_string(&args[idx + 1])
                            .expect("Invalid sync duration format. Use format: 7d, 24h, or 30m"),
                    );
                    idx += 2;
                } else {
                    panic!("--sync requires a duration value (e.g., 7d, 24h, 30m)");
                }
            }
            "--help" => {
                println!("Usage: aw-watcher-lastfm-rust [--testing] [--port PORT] [--sync DURATION] [--help]");
                println!("\nOptions:");
                println!("  --testing         Use testing port (5699)");
                println!("  --port PORT       Specify custom port");
                println!("  --sync DURATION   Sync historical data (format: 7d, 24h, 30m)");
                println!("  --help            Show this help message");
            }
            _ => {
                println!("Unknown argument: {}", args[idx]);
            }
        }
    }

    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "info")
        .write_style_or("MY_LOG_STYLE", "always");

    env_logger::init_from_env(env);

    if !config_path.exists() {
        if !config_dir.exists() {
            DirBuilder::new()
                .recursive(true)
                .create(&config_dir)
                .expect("Unable to create directory");
        }
        let mut file = File::create(&config_path).expect("Unable to create file");
        file.write_all(b"username: your_username\napikey: your-api-key\npolling_interval: 10")
            .expect("Unable to write to file");
        panic!("Please set your api key and username at {:?}", config_path);
    }

    let mut config_file = File::open(config_path.clone()).expect("Unable to open file");
    let mut contents = String::new();
    config_file
        .read_to_string(&mut contents)
        .expect("Unable to read file");

    let yaml: Value =
        serde_yaml::from_str(&contents).expect("Unable to parse yaml from config file");
    let apikey = yaml["apikey"]
        .as_str()
        .expect("Unable to get api key from config file")
        .to_string();
    let username = yaml["username"]
        .as_str()
        .expect("Unable to get username from config file")
        .to_string();
    let polling_interval = yaml["polling_interval"].as_u64().unwrap_or(10);
    if polling_interval < 3 {
        // for rate limiting, recommend at least 10 seconds but 3 will work
        panic!("Polling interval must be at least 3 seconds");
    }

    drop(config_file);

    if username == "your_username" || username == "" {
        panic!("Please set your username at {:?}", config_path);
    }

    if apikey == "your-api-key" || apikey == "" {
        panic!("Please set your api key at {:?}", config_path);
    }

    let url = format!("https://ws.audioscrobbler.com/2.0/?method=user.getrecenttracks&user={}&api_key={}&format=json&limit=1", username, apikey);

    let aw_client = AwClient::new("localhost", port, "aw-watcher-lastfm-rust").unwrap();
    
    if aw_client.wait_for_start().is_err() {
        warn!("Failed to connect to ActivityWatch Server");
        exit(1)
    }
    aw_client.create_bucket_simple("aw-watcher-lastfm", "currently-playing").expect("Failed to create a bucket");
    
    let polling_time = TimeDelta::seconds(polling_interval as i64);

    let client = reqwest::blocking::ClientBuilder::new()
        .timeout(Duration::from_secs(5))
        .user_agent(concat!(
            "aw-watcher-lastfm/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .unwrap();

    // Handle historical sync if requested
    if let Some(duration) = sync_duration {
        info!("Starting historical sync...");
        match sync_historical_data(&client, &aw_client, &username, &apikey, duration) {
            Ok(_) => info!("Historical sync completed successfully"),
            Err(e) => warn!("Error during historical sync: {:?}", e),
        }
        info!("Starting real-time tracking...");
    }

    run_loop(client, url, aw_client, polling_time, polling_interval);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_time_string() {
        // Test valid inputs
        assert_eq!(parse_time_string("7d"), Some(TimeDelta::days(7)));
        assert_eq!(parse_time_string("24h"), Some(TimeDelta::hours(24)));
        assert_eq!(parse_time_string("30m"), Some(TimeDelta::minutes(30)));

        // Test invalid inputs
        assert_eq!(parse_time_string(""), None);
        assert_eq!(parse_time_string("30s"), None); // Invalid unit
        assert_eq!(parse_time_string("abc"), None); // Invalid format
        assert_eq!(parse_time_string("-1d"), None); // Negative number
    }
}
