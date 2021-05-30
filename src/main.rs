#[macro_use]
extern crate lazy_static;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;

use std::env;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};

lazy_static! {
    static ref APP: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::new()));
}

struct OtpEntry {
    name: String,
    otp: String,
}

struct AppState {
    otp_codes: HashMap<u64, String>,
}

impl AppState {
    fn new() -> Self {
        AppState {
            otp_codes: HashMap::new(),
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

    fn clear_otp_codes(&mut self) {
        self.otp_codes.clear();
    }
}

fn build_menu(otp_entries: &[OtpEntry]) -> gtk::Menu {
    let menu = gtk::Menu::new();

    let mut app = APP.lock().unwrap();
    app.clear_otp_codes();
    for entry in otp_entries {
        let display = format!("{}: {}", entry.name, entry.otp);
        let otp_item = gtk::MenuItem::with_label(&display);
        otp_item.connect_activate(|menu_item| {
            let atom = gdk::Atom::intern("CLIPBOARD");
            let clipboard = gtk::Clipboard::get(&atom);
            let app = APP.lock().unwrap();
            match app.get_otp_value(&menu_item) {
                Some(code) => clipboard.set_text(code),
                None => {}
            }
        });
        menu.append(&otp_item);
        app.add_otp_value(&otp_item, entry.otp.clone());
    }

    let mi = gtk::CheckMenuItem::with_label("Quit");
    mi.connect_activate(|_| {
        gtk::main_quit();
    });
    menu.append(&mi);
    menu
}

fn generate_otp_entries() -> Vec<OtpEntry> {
    vec![
        OtpEntry {
            name: "Amazon".to_string(),
            otp: "39480".to_string(),
        },
        OtpEntry {
            name: "Dropbox".to_string(),
            otp: "43909".to_string(),
        },
    ]
}

fn main() {
    gtk::init().unwrap();
    let mut indicator = AppIndicator::new("OTP Tray", "");
    indicator.set_status(AppIndicatorStatus::Active);
    let icon_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    indicator.set_icon_theme_path(icon_path.to_str().unwrap());
    indicator.set_icon_full("rust-logo-64x64-white", "icon");

    let mut menu = build_menu(&generate_otp_entries());
    indicator.set_menu(&mut menu);
    menu.show_all();
    gtk::main();
}
