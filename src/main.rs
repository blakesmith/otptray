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

static VALID_HASH_FNS: &'static [&str] = &["sha1", "sha256", "sha512"];

#[derive(Debug, Clone)]
enum ValidationError {
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

impl OtpEntry {
    fn input_validate(
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

#[derive(Clone, Copy, Debug)]
enum EntryAction {
    Add,
    Edit,
}

impl EntryAction {
    fn window_title(&self) -> &'static str {
        match self {
            EntryAction::Add => "Add Entry",
            EntryAction::Edit => "Edit Entry",
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

    fn save_entry(&self, otp_entry: OtpEntry, entry_action: EntryAction) -> AppState {
        let new_otp_entries = match entry_action {
            EntryAction::Add => {
                let mut entries = self.otp_entries.clone();
                entries.push(otp_entry);
                entries
            }
            EntryAction::Edit => self.otp_entries.clone(), // TODO: Base the edit off the combo box position?
        };

        Self {
            otp_entries: new_otp_entries,
            ..Default::default()
        }
    }

    fn menu_reset(&self) -> Self {
        Self {
            otp_entries: self.otp_entries.clone(),
            ..Default::default()
        }
    }
}

fn otp_entry_window(otp_entry: &OtpEntry, entry_action: EntryAction) {
    let window = gtk::WindowBuilder::new().build();

    let page_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let form_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let name_entry = gtk::EntryBuilder::new().build();
    let name_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(5)
        .margin_end(5)
        .margin_bottom(10)
        .build();
    name_box.add(&gtk::LabelBuilder::new().label("Name").build());
    name_box.add(&name_entry);

    let secret_entry = gtk::EntryBuilder::new().build();
    let secret_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(5)
        .margin_end(5)
        .margin_bottom(10)
        .build();
    secret_box.add(&gtk::LabelBuilder::new().label("Secret").build());
    secret_box.add(&secret_entry);

    let hash_fn_combo = gtk::ComboBoxTextBuilder::new().build();
    hash_fn_combo.append(Some("sha1"), "sha1");
    hash_fn_combo.append(Some("sha256"), "sha256");
    hash_fn_combo.append(Some("sha512"), "sha512");
    hash_fn_combo.set_active_id(Some("sha1"));
    let hash_fn_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(5)
        .margin_end(5)
        .margin_bottom(10)
        .build();
    hash_fn_box.add(&gtk::LabelBuilder::new().label("Hash Function").build());
    hash_fn_box.add(&hash_fn_combo);

    let step_entry = gtk::EntryBuilder::new()
        .buffer(&gtk::EntryBuffer::new(Some(&otp_entry.step.to_string())))
        .build();
    let step_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(5)
        .margin_end(5)
        .margin_bottom(10)
        .build();
    step_box.add(&gtk::LabelBuilder::new().label("Step in Seconds").build());
    step_box.add(&step_entry);

    let digit_entry = gtk::EntryBuilder::new()
        .buffer(&gtk::EntryBuffer::new(Some(
            &otp_entry.digit_count.to_string(),
        )))
        .build();
    let digit_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(5)
        .margin_end(5)
        .margin_bottom(10)
        .build();
    digit_box.add(
        &gtk::LabelBuilder::new()
            .label("Password Digit Length")
            .build(),
    );
    digit_box.add(&digit_entry);

    form_box.add(&name_box);
    form_box.add(&secret_box);
    form_box.add(&hash_fn_box);
    form_box.add(&step_box);
    form_box.add(&digit_box);

    let form_frame = gtk::FrameBuilder::new()
        .label(entry_action.window_title())
        .child(&form_box)
        .vexpand(true)
        .margin(5)
        .build();

    let button_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Horizontal)
        .margin(5)
        .build();
    let save_button = gtk::ButtonBuilder::new()
        .margin_end(3)
        .label("Save")
        .build();
    let cancel_button = gtk::ButtonBuilder::new()
        .margin_end(3)
        .label("Cancel")
        .build();
    let save_window = window.clone();

    save_button.connect_clicked(move |_| {
        let new_otp_entry = OtpEntry::input_validate(
            name_entry.get_buffer().get_text(),
            step_entry.get_buffer().get_text(),
            secret_entry.get_buffer().get_text(),
            hash_fn_combo.get_active_id().unwrap().as_str().to_string(), // Our combo box should always have a value
            digit_entry.get_buffer().get_text(),
        );
        match new_otp_entry {
            Ok(entry) => {
                log::info!("Saving: {:?}", entry);
                APP_STATE.store(APP_STATE.load().save_entry(entry, entry_action));
            }
            Err(err) => log::info!("Invalid entry input: {:?}", err), // TODO: Pop up some error window
        }
        save_window.close();
    });
    let cancel_window = window.clone();
    cancel_button.connect_clicked(move |_| {
        cancel_window.close();
    });
    button_box.add(&save_button);
    button_box.add(&cancel_button);

    page_box.add(&form_frame);
    page_box.add(&button_box);

    window.add(&page_box);
    window.set_default_size(350, 350);
    window.set_title(entry_action.window_title());
    window.set_position(gtk::WindowPosition::Center);
    window.show_all();
}

fn setup_page(app_state: &AppState) -> gtk::Box {
    let page_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let button_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Horizontal)
        .margin(5)
        .build();
    let add_button = gtk::ButtonBuilder::new().margin_end(3).label("Add").build();
    add_button.connect_clicked(|_| {
        otp_entry_window(&Default::default(), EntryAction::Add);
    });
    let edit_button = gtk::ButtonBuilder::new()
        .margin_end(3)
        .label("Edit")
        .build();
    let remove_button = gtk::ButtonBuilder::new()
        .margin_end(3)
        .label("Remove")
        .build();
    button_box.add(&add_button);
    button_box.add(&edit_button);
    button_box.add(&remove_button);
    page_box.add(&otp_configuration(&app_state.otp_entries));
    page_box.add(&button_box);
    page_box
}

fn about_page() -> gtk::Box {
    let gtk_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Horizontal)
        .halign(gtk::Align::Center)
        .build();
    let label = gtk::LabelBuilder::new().label("About OTPTray").build();
    gtk_box.add(&label);
    gtk_box
}

fn otp_configuration(otp_entries: &[OtpEntry]) -> gtk::Frame {
    let otp_list = gtk::ListBoxBuilder::new()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    for entry in otp_entries {
        let row = gtk::ListBoxRowBuilder::new()
            .child(&gtk::LabelBuilder::new().label(&entry.name).build())
            .build();
        otp_list.add(&row);
    }
    let viewport = gtk::ViewportBuilder::new().child(&otp_list).build();
    let window = gtk::ScrolledWindowBuilder::new()
        .hexpand(true)
        .vexpand(true)
        .child(&viewport)
        .build();
    gtk::FrameBuilder::new()
        .label("One-Time Password Setup")
        .margin(5)
        .child(&window)
        .build()
}

fn setup_window() {
    let page_stack = gtk::StackBuilder::new().build();
    let app_state = APP_STATE.load();

    page_stack.add_titled(&setup_page(&app_state), "Setup", "Setup");
    page_stack.add_titled(&about_page(), "About", "About");

    let page_switcher = gtk::StackSwitcherBuilder::new().stack(&page_stack).build();

    let header_bar = gtk::HeaderBarBuilder::new()
        .show_close_button(true)
        .custom_title(&page_switcher)
        .build();

    let page_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .build();

    page_box.add(&page_stack);

    let window = gtk::WindowBuilder::new().resizable(true).build();
    window.add(&page_box);
    window.set_title("OTPTray Setup");
    window.set_titlebar(Some(&header_bar));
    window.set_position(gtk::WindowPosition::Center);
    window.set_default_size(250, 200);
    window.show_all();
}

fn build_menu() -> gtk::Menu {
    let menu = gtk::Menu::new();

    let app_state = APP_STATE.load();
    let mut new_app_state = app_state.menu_reset();
    if !app_state.otp_entries.is_empty() {
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
    setup_item.connect_activate(|_| {
        setup_window();
    });
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
