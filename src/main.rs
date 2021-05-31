#[macro_use]
extern crate lazy_static;

use atomic_immut::AtomicImmut;

use serde::Deserialize;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;

use std::env;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use totp_lite::{totp_custom, Sha1, Sha256, Sha512};

use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};

lazy_static! {
    static ref APP_STATE: Arc<AtomicImmut<AppState>> =
        Arc::new(AtomicImmut::new(Default::default()));
}

#[derive(Debug)]
enum Error {
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

struct OtpValue {
    name: String,
    otp: String,
}

#[derive(Clone, Debug, Deserialize)]
struct OtpEntry {
    name: String,
    step: u64,
    secret_hash: String,
    hash_fn: String,
    digit_count: u32,
}

#[derive(Debug, Deserialize)]
struct OtpTrayConfig {
    entries: Vec<OtpEntry>,
}

impl OtpEntry {
    fn get_otp_value(&self) -> OtpValue {
        let unix_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let otp = match &self.hash_fn[..] {
            "sha1" => totp_custom::<Sha1>(
                self.step,
                self.digit_count,
                self.secret_hash.as_bytes(),
                unix_epoch,
            ),
            "sha256" => totp_custom::<Sha256>(
                self.step,
                self.digit_count,
                self.secret_hash.as_bytes(),
                unix_epoch,
            ),
            "sha512" => totp_custom::<Sha512>(
                self.step,
                self.digit_count,
                self.secret_hash.as_bytes(),
                unix_epoch,
            ),
            other => panic!("Unknown hash function: {}", other),
        };
        OtpValue {
            name: self.name.clone(),
            otp,
        }
    }
}

#[derive(Clone)]
struct AppState {
    otp_entries: Vec<OtpEntry>,
    otp_codes: HashMap<u64, String>,
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
    fn load_from_config() -> Result<AppState, Error> {
        use std::fs::OpenOptions;

        let config_dir = dirs::config_dir().ok_or(Error::NoUserConfigDir)?;
        let config_file_path = config_dir.join("otptray.yaml");
        match OpenOptions::new().read(true).open(config_file_path) {
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

    fn add_otp_value<T: Hash>(&mut self, entry: &T, otp_code: String) {
        let mut hasher = DefaultHasher::new();
        entry.hash(&mut hasher);
        let key = hasher.finish();
        self.otp_codes.insert(key, otp_code);
    }

    fn get_otp_value<T: Hash>(&self, entry: &T) -> Option<&String> {
        let mut hasher = DefaultHasher::new();
        entry.hash(&mut hasher);
        let key = hasher.finish();
        self.otp_codes.get(&key)
    }

    fn menu_reset(&self) -> Self {
        Self {
            otp_entries: self.otp_entries.clone(),
            ..Default::default()
        }
    }
}

fn build_menu() -> gtk::Menu {
    let menu = gtk::Menu::new();

    let app_state = APP_STATE.load();
    let mut new_app_state = app_state.menu_reset();
    if app_state.otp_entries.len() > 0 {
        for entry in &app_state.otp_entries {
            let otp_value = entry.get_otp_value();
            let display = format!("{}: {}", otp_value.name, otp_value.otp);
            let otp_item = gtk::MenuItem::with_label(&display);
            otp_item.connect_activate(|menu_item| {
                let atom = gdk::Atom::intern("CLIPBOARD");
                let clipboard = gtk::Clipboard::get(&atom);
                let app_state = APP_STATE.load();
                if let Some(code) = app_state.get_otp_value(&menu_item) {
                    clipboard.set_text(code);
                }
            });
            menu.append(&otp_item);
            new_app_state.add_otp_value(&otp_item, otp_value.otp.clone());
        }
    } else {
        menu.append(&gtk::MenuItem::with_label(
            "No OTP entries. Start with setup",
        ));
    }

    menu.append(&gtk::SeparatorMenuItem::new());

    let setup_item = gtk::MenuItem::with_label("Setup");
    let quit_item = gtk::MenuItem::with_label("Quit");
    quit_item.connect_activate(|_| {
        gtk::main_quit();
    });
    menu.append(&setup_item);
    menu.append(&quit_item);

    APP_STATE.store(new_app_state);
    menu
}

fn periodic_otp_task(indicator: &mut AppIndicator) {
    let mut menu = build_menu();
    indicator.set_menu(&mut menu);
    menu.show_all();
}

fn main() {
    gtk::init().unwrap();

    let app_state = AppState::load_from_config().expect("Cannot load OTPTrap config!");
    APP_STATE.store(app_state);

    let mut indicator = AppIndicator::new("OTP Tray", "");
    indicator.set_status(AppIndicatorStatus::Active);
    let icon_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    indicator.set_icon_theme_path(icon_path.to_str().unwrap());
    indicator.set_icon_full("rust-logo-64x64-white", "icon");

    periodic_otp_task(&mut indicator);
    glib::timeout_add_seconds_local(10, move || {
        periodic_otp_task(&mut indicator);
        Continue(true)
    });
    gtk::main();
}
