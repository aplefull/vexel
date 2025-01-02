use std::fmt::Display;

#[cfg(not(target_arch = "wasm32"))]
use std::time::SystemTime;

#[cfg(target_arch = "wasm32")]
use web_sys::console;
#[cfg(target_arch = "wasm32")]
use js_sys::Date;

const RESET: &str = "\x1b[0m";
const BLUE: &str = "\x1b[34m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

#[macro_export]
macro_rules! log_info {
    ($msg:expr) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Info,
            &$msg.to_string()
        )
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Info,
            &format!($fmt, $($arg)*)
        )
    };
}

#[macro_export]
macro_rules! log_debug {
    ($msg:expr) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Debug,
            &$msg.to_string()
        )
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Debug,
            &format!($fmt, $($arg)*)
        )
    };
}

#[macro_export]
macro_rules! log_warn {
    ($msg:expr) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Warning,
            &$msg.to_string()
        )
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Warning,
            &format!($fmt, $($arg)*)
        )
    };
}

#[macro_export]
macro_rules! log_error {
    ($msg:expr) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Error,
            &$msg.to_string()
        )
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::utils::logger::Logger::log(
            $crate::utils::logger::LogLevel::Error,
            &format!($fmt, $($arg)*)
        )
    };
}

#[derive(Debug, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

pub struct Logger {}

impl Logger {
    pub fn new() -> Self {
        Self {}
    }

    fn get_timestamp() -> String {
        #[cfg(target_arch = "wasm32")]
        let (secs, millis) = {
            let now = Date::now();
            ((now / 1000.0) as u64, (now % 1000.0) as u32)
        };

        #[cfg(not(target_arch = "wasm32"))]
        let (secs, millis) = {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            (now.as_secs(), now.subsec_millis())
        };

        let hours = (secs / 3600) % 24;
        let minutes = (secs / 60) % 60;
        let seconds = secs % 60;

        format!("{:02}:{:02}:{:02}.{:03}", hours, minutes, seconds, millis)
    }

    pub fn log(level: LogLevel, message: impl Display) {
        let (level_str, color) = match level {
            LogLevel::Debug => ("DEBUG", BLUE),
            LogLevel::Info => ("INFO", GREEN),
            LogLevel::Warning => ("WARN", YELLOW),
            LogLevel::Error => ("ERROR", RED),
        };

        #[cfg(target_arch = "wasm32")]
        console::log_1(&format!("{} [{}] {}", Self::get_timestamp(), level_str, message).into());

        println!(
            "{} [{}{}{}] {}",
            Self::get_timestamp(),
            color,
            level_str,
            RESET,
            message
        );
    }
}
