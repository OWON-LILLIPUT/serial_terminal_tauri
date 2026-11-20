use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct SerialConfig {
    pub port_name: String,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub parity: String,
    pub stop_bits: u8,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port_name: String::new(),
            baud_rate: 1_500_000,
            data_bits: 8,
            parity: "none".into(),
            stop_bits: 1,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub serial: SerialConfig,
    pub adb_address: String,
    pub adb_history: Vec<String>,
    pub adb_device: String,
    pub serial_sync: bool,
    pub serial_binding: Option<(String, String)>,
    pub video_codec: String,
    pub video_bit_rate: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            serial: SerialConfig::default(),
            adb_address: "192.168.30.1:5555".into(),
            adb_history: Vec::new(),
            adb_device: String::new(),
            serial_sync: true,
            serial_binding: None,
            video_codec: "h265".into(),
            video_bit_rate: "8M".into(),
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|path| path.join("com.lt.serial-terminal-ratatui"))
}

impl Settings {
    pub fn load() -> Self {
        config_dir()
            .and_then(|path| fs::read(path.join("settings.json")).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_dir().ok_or_else(|| std::io::Error::other("找不到配置目录"))?;
        fs::create_dir_all(&path)?;
        let mut staged = tempfile::NamedTempFile::new_in(&path)?;
        serde_json::to_writer_pretty(staged.as_file_mut(), self)?;
        staged.persist(path.join("settings.json"))?;
        Ok(())
    }
}
