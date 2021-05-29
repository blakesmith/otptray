use std::env;
use std::path::Path;

use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};

struct OtpEntry {
    name: String,
    otp: String,
}

fn build_menu(otp_entries: &[OtpEntry]) -> gtk::Menu {
    let menu = gtk::Menu::new();

    for entry in otp_entries {
        let display = format!("{}: {}", entry.name, entry.otp);
        let menu_item = gtk::CheckMenuItem::with_label(&display);
        menu.append(&menu_item);
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
