use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use totp_lite::{totp_custom, Sha1, Sha256, Sha512};

static VALID_HASH_FNS: &'static [&str] = &["sha1", "sha256", "sha512"];

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OtpEntry {
    pub name: String,
    pub step: u64,
    pub secret_hash: String,
    pub hash_fn: String,
    pub digit_count: u32,
}

impl OtpEntry {
    pub fn input_validate(
        name: String,
        step: String,
        secret_hash: String,
        hash_fn: String,
        digit_count: String,
    ) -> Result<Self, ValidationError> {
        if name.is_empty() {
            return Err(ValidationError::Empty { field: "name" });
        }
        if name.len() > 255 {
            return Err(ValidationError::Length {
                field: "name",
                upper_bound: 255,
                length: name.len(),
            });
        }
        if secret_hash.is_empty() {
            return Err(ValidationError::Empty { field: "secret" });
        }
        if VALID_HASH_FNS
            .iter()
            .find(|valid_hash| **valid_hash == hash_fn)
            .is_none()
        {
            return Err(ValidationError::InvalidSelection {
                field: "hash function",
                candidate: hash_fn,
                valid_selections: VALID_HASH_FNS,
            });
        }
        let step_parsed = step.parse::<u64>()?;
        let digit_count_parsed = digit_count.parse::<u8>()?;
        Ok(OtpEntry {
            name,
            step: step_parsed,
            secret_hash,
            hash_fn,
            digit_count: digit_count_parsed as u32,
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub otp_entries: Vec<OtpEntry>,
    pub otp_codes: HashMap<u64, String>,
}

#[derive(Clone, Copy, Debug)]
pub enum EntryAction {
    Add,
    Edit(usize),
}

impl EntryAction {
    pub fn window_title(&self) -> &'static str {
        match self {
            EntryAction::Add => "Add Entry",
            EntryAction::Edit(_) => "Edit Entry",
        }
    }
}

impl Default for OtpEntry {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            secret_hash: "".to_string(),
            hash_fn: "sha1".to_string(), // Google Authenticator defaults
            step: 30,                    // Google Authenticator defaults
            digit_count: 6,              // Google Authenticator defaults
        }
    }
}

impl OtpEntry {
    pub fn get_otp_value(&self) -> OtpValue {
        let unix_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let secret = base32::decode(
            base32::Alphabet::RFC4648 { padding: false },
            &self.secret_hash,
        )
        .unwrap_or_default(); // TODO: Proper error handling.
        let otp = match &self.hash_fn[..] {
            "sha1" => totp_custom::<Sha1>(self.step, self.digit_count, &secret, unix_epoch),
            "sha256" => totp_custom::<Sha256>(self.step, self.digit_count, &secret, unix_epoch),
            "sha512" => totp_custom::<Sha512>(self.step, self.digit_count, &secret, unix_epoch),
            other => panic!("Unknown hash function: {}", other),
        };
        OtpValue {
            name: self.name.clone(),
            otp,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            otp_entries: Vec::new(),
            otp_codes: HashMap::new(),
        }
    }
}

impl AppState {
    pub fn config_path() -> Result<PathBuf, Error> {
        let config_dir = dirs::config_dir().ok_or(Error::NoUserConfigDir)?;
        Ok(config_dir.join("otptray.yaml"))
    }

    pub fn load_from_config() -> Result<AppState, Error> {
        match OpenOptions::new().read(true).open(Self::config_path()?) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Default::default()),
            Err(err) => Err(err.into()),
            Ok(file) => {
                let config: OtpTrayConfig = serde_yaml::from_reader(&file)?;
                Ok(AppState {
                    otp_entries: config.entries,
                    ..Default::default()
                })
            }
        }
    }

    pub fn save_to_config(&self) -> Result<(), Error> {
        #[cfg(target_family = "unix")]
        use std::os::unix::fs::OpenOptionsExt;

        let mut base_options = OpenOptions::new();
        base_options
            .write(true)
            .create(true)
            .truncate(true)
            .read(true);

        if cfg!(unix) {
            base_options.mode(0o600);
        }

        base_options
            .open(Self::config_path()?)
            .map_err(|err| err.into())
            .and_then(|file| {
                let config = OtpTrayConfig {
                    entries: self.otp_entries.clone(),
                };
                serde_yaml::to_writer(&file, &config).map_err(|err| err.into())
            })
    }

    pub fn add_otp_value<T: Hash>(&mut self, entry: &T, otp_code: String) -> u64 {
        let mut hasher = DefaultHasher::new();
        entry.hash(&mut hasher);
        let key = hasher.finish();
        self.otp_codes.insert(key, otp_code);
        key
    }

    // TODO: Deprecate!
    pub fn get_otp_value_by_id(&self, id: u64) -> Option<&String> {
        self.otp_codes.get(&id)
    }

    pub fn get_otp_value_at_index(&self, index: usize) -> Option<OtpValue> {
        self.otp_entries
            .get(index)
            .map(|entry| entry.get_otp_value())
    }

    pub fn save_entry(&self, otp_entry: OtpEntry, entry_action: EntryAction) -> AppState {
        let mut entries = self.otp_entries.clone();
        let new_otp_entries = match entry_action {
            EntryAction::Add => {
                entries.push(otp_entry);
                entries
            }
            EntryAction::Edit(index) => {
                entries[index] = otp_entry;
                entries
            }
        };

        Self {
            otp_entries: new_otp_entries,
            ..Default::default()
        }
    }

    pub fn remove_entry_index(&self, index: usize) -> AppState {
        let mut new_otp_entries = self.otp_entries.clone();
        new_otp_entries.remove(index);
        Self {
            otp_entries: new_otp_entries,
            ..Default::default()
        }
    }

    pub fn menu_reset(&self) -> Self {
        Self {
            otp_entries: self.otp_entries.clone(),
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub enum UiEvent {
    TotpRefresh,
    OpenSetup,
    OpenEntry(EntryAction),
    SaveEntry(OtpEntry, EntryAction),
    RemoveEntry(usize),
    CopyToClipboard(u64),
    Quit,
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    Empty {
        field: &'static str,
    },
    IntegerFormat(std::num::ParseIntError),
    Length {
        field: &'static str,
        upper_bound: usize,
        length: usize,
    },
    InvalidSelection {
        field: &'static str,
        candidate: String,
        valid_selections: &'static [&'static str],
    },
}

impl From<std::num::ParseIntError> for ValidationError {
    fn from(err: std::num::ParseIntError) -> Self {
        ValidationError::IntegerFormat(err)
    }
}

#[derive(Debug)]
pub enum Error {
    NoUserConfigDir,
    YAML(serde_yaml::Error),
    Io(std::io::Error),
}

impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Error {
        Error::YAML(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
    }
}

pub struct OtpValue {
    pub name: String,
    pub otp: String,
}

impl OtpValue {
    pub fn formatted_menu_display(&self) -> String {
        format!("{}: {}", self.name, self.otp)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OtpTrayConfig {
    entries: Vec<OtpEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationPolicy {
    Foreground,
    Background,
}
