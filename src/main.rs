#[macro_use]
extern crate lazy_static;

use atomic_immut::AtomicImmut;

use serde::{Deserialize, Serialize};
use simple_logger::SimpleLogger;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::env;
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
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

#[derive(Debug)]
enum UiEvent {
    TotpRefresh,
    OpenSetup,
    OpenEntry(EntryAction),
    SaveEntry(OtpEntry, EntryAction),
    RemoveEntry(usize),
    CopyToClipboard(u64),
    Quit,
}

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

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    Edit(usize),
}

impl EntryAction {
    fn window_title(&self) -> &'static str {
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

#[derive(Debug, Deserialize, Serialize)]
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
    fn config_path() -> Result<PathBuf, Error> {
        let config_dir = dirs::config_dir().ok_or(Error::NoUserConfigDir)?;
        Ok(config_dir.join("otptray.yaml"))
    }

    fn load_from_config() -> Result<AppState, Error> {
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

    fn save_to_config(&self) -> Result<(), Error> {
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

    fn add_otp_value<T: Hash>(&mut self, entry: &T, otp_code: String) -> u64 {
        let mut hasher = DefaultHasher::new();
        entry.hash(&mut hasher);
        let key = hasher.finish();
        self.otp_codes.insert(key, otp_code);
        key
    }

    fn get_otp_value_by_id(&self, id: u64) -> Option<&String> {
        self.otp_codes.get(&id)
    }

    fn save_entry(&self, otp_entry: OtpEntry, entry_action: EntryAction) -> AppState {
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

    fn remove_entry_index(&self, index: usize) -> AppState {
        let mut new_otp_entries = self.otp_entries.clone();
        new_otp_entries.remove(index);
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

fn otp_entry_window(otp_entry: &OtpEntry, entry_action: EntryAction, tx: glib::Sender<UiEvent>) {
    let window = gtk::WindowBuilder::new().build();

    let page_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let form_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let name_entry = gtk::EntryBuilder::new()
        .buffer(&gtk::EntryBuffer::new(Some(&otp_entry.name)))
        .build();
    let name_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .margin_start(5)
        .margin_end(5)
        .margin_bottom(10)
        .build();
    name_box.add(&gtk::LabelBuilder::new().label("Name").build());
    name_box.add(&name_entry);

    let secret_entry = gtk::EntryBuilder::new()
        .buffer(&gtk::EntryBuffer::new(Some(&otp_entry.secret_hash)))
        .build();
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
                let _ = tx.send(UiEvent::SaveEntry(entry, entry_action));
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

    window.connect_key_press_event(move |_, key_event| {
        match key_event.get_keyval().name() {
            Some(key_name) if key_name == "Return" => {
                save_button.clicked();
            }
            Some(key_name) if key_name == "Escape" => {
                cancel_button.clicked();
            }
            _ => {}
        }

        Inhibit(false)
    });
    window.add(&page_box);
    window.set_default_size(350, 350);
    window.set_title(entry_action.window_title());
    window.set_position(gtk::WindowPosition::Center);
    window.show_all();
}

fn build_otp_list(otp_list: &mut gtk::ListBox, otp_entries: &[OtpEntry]) {
    otp_list.foreach(|c| otp_list.remove(c));

    for (i, entry) in otp_entries.iter().enumerate() {
        let row = gtk::ListBoxRowBuilder::new()
            .child(&gtk::LabelBuilder::new().label(&entry.name).build())
            .build();
        otp_list.add(&row);
        if i == 0 {
            otp_list.select_row(Some(&row));
        }
    }

    otp_list.show_all();
}

fn otp_configuration(otp_entries: &[OtpEntry]) -> (gtk::Frame, gtk::ListBox) {
    let mut otp_list = gtk::ListBoxBuilder::new()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    build_otp_list(&mut otp_list, otp_entries);
    let viewport = gtk::ViewportBuilder::new().child(&otp_list).build();
    let window = gtk::ScrolledWindowBuilder::new()
        .hexpand(true)
        .vexpand(true)
        .child(&viewport)
        .build();
    let frame = gtk::FrameBuilder::new()
        .label("One-Time Password Setup")
        .margin(5)
        .child(&window)
        .build();
    (frame, otp_list)
}

fn setup_page(app_state: &AppState, tx: glib::Sender<UiEvent>) -> (gtk::Box, gtk::ListBox) {
    let page_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Vertical)
        .build();
    let (frame, otp_list) = otp_configuration(&app_state.otp_entries);
    let button_box = gtk::BoxBuilder::new()
        .orientation(gtk::Orientation::Horizontal)
        .margin(5)
        .build();
    let add_button = gtk::ButtonBuilder::new().margin_end(3).label("Add").build();

    let add_tx = tx.clone();
    add_button.connect_clicked(move |_| {
        let _ = add_tx.send(UiEvent::OpenEntry(EntryAction::Add));
    });
    let edit_button = gtk::ButtonBuilder::new()
        .margin_end(3)
        .label("Edit")
        .build();

    let edit_otp_list = otp_list.clone();
    let edit_tx = tx.clone();
    edit_button.connect_clicked(move |_| {
        if let Some(selected_row) = edit_otp_list
            .get_selected_row()
            .map(|row| row.get_index() as usize)
        {
            let _ = edit_tx.send(UiEvent::OpenEntry(EntryAction::Edit(selected_row)));
        }
    });
    let remove_button = gtk::ButtonBuilder::new()
        .margin_end(3)
        .label("Remove")
        .build();
    let delete_otp_list = otp_list.clone();
    let remove_tx = tx.clone();
    remove_button.connect_clicked(move |_| {
        if let Some(selected_row) = delete_otp_list
            .get_selected_row()
            .map(|row| row.get_index() as usize)
        {
            let _ = remove_tx.send(UiEvent::RemoveEntry(selected_row));
        }
    });
    button_box.add(&add_button);
    button_box.add(&edit_button);
    button_box.add(&remove_button);
    page_box.add(&frame);
    page_box.add(&button_box);
    (page_box, otp_list)
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

fn setup_window(app_state: Arc<AppState>, tx: glib::Sender<UiEvent>) -> gtk::ListBox {
    let page_stack = gtk::StackBuilder::new().build();

    let (setup_box, otp_list) = setup_page(&app_state, tx);
    page_stack.add_titled(&setup_box, "Setup", "Setup");
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
    window.connect_key_press_event(move |w, key_event| {
        match key_event.get_keyval().name() {
            Some(key_name) if key_name == "Escape" => {
                w.close();
            }
            _ => {}
        }

        Inhibit(false)
    });
    window.add(&page_box);
    window.set_title("OTPTray Setup");
    window.set_titlebar(Some(&header_bar));
    window.set_position(gtk::WindowPosition::Center);
    window.set_default_size(250, 200);
    window.show_all();
    otp_list
}

fn build_menu(app_state: Arc<AppState>, tx: glib::Sender<UiEvent>) -> (AppState, gtk::Menu) {
    let menu = gtk::Menu::new();

    let mut new_app_state = app_state.menu_reset();
    if !app_state.otp_entries.is_empty() {
        for entry in &app_state.otp_entries {
            let otp_value = entry.get_otp_value();
            let display = format!("{}: {}", otp_value.name, otp_value.otp);
            let otp_item = gtk::MenuItem::with_label(&display);
            let menu_item_id = new_app_state.add_otp_value(&otp_item, otp_value.otp.clone());
            let copy_tx = tx.clone();
            otp_item.connect_activate(move |_| {
                let _ = copy_tx.send(UiEvent::CopyToClipboard(menu_item_id));
            });
            menu.append(&otp_item);
        }
    } else {
        menu.append(&gtk::MenuItem::with_label(
            "No OTP entries. Start with setup",
        ));
    }

    menu.append(&gtk::SeparatorMenuItem::new());

    let setup_item = gtk::MenuItem::with_label("Setup");
    let setup_tx = tx.clone();
    setup_item.connect_activate(move |_| {
        let _ = setup_tx.send(UiEvent::OpenSetup);
    });
    let quit_item = gtk::MenuItem::with_label("Quit");
    let quit_tx = tx.clone();
    quit_item.connect_activate(move |_| {
        let _ = quit_tx.send(UiEvent::Quit);
    });
    menu.append(&setup_item);
    menu.append(&quit_item);

    (new_app_state, menu)
}

fn main() {
    SimpleLogger::new().init().unwrap();
    gtk::init().unwrap();

    let (tx, rx): (glib::Sender<UiEvent>, glib::Receiver<UiEvent>) =
        glib::MainContext::channel(glib::PRIORITY_DEFAULT);

    let app_state = AppState::load_from_config().expect("Cannot load OTPTrap config!");
    APP_STATE.store(app_state);

    let mut indicator = AppIndicator::new("OTP Tray", "");
    indicator.set_status(AppIndicatorStatus::Active);
    let icon_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    indicator.set_icon_theme_path(icon_path.to_str().unwrap());
    indicator.set_icon_full("rust-logo-64x64-white", "icon");

    let periodic_tx = tx.clone();
    glib::timeout_add_seconds_local(10, move || {
        let _ = periodic_tx.send(UiEvent::TotpRefresh);
        Continue(true)
    });

    let mut otp_setup_list: Option<gtk::ListBox> = None;

    let event_tx = tx.clone();
    rx.attach(None, move |event| {
        log::debug!("Got UI event: {:?}", event);
        match event {
            UiEvent::TotpRefresh => {
                let app_state = APP_STATE.load();
                let (new_app_state, mut menu) = build_menu(app_state, event_tx.clone());
                APP_STATE.store(new_app_state);
                indicator.set_menu(&mut menu);
                menu.show_all();
            }
            UiEvent::CopyToClipboard(menu_item_id) => {
                let app_state = APP_STATE.load();
                if let Some(code) = app_state.get_otp_value_by_id(menu_item_id) {
                    let atom = gdk::Atom::intern("CLIPBOARD");
                    let clipboard = gtk::Clipboard::get(&atom);
                    clipboard.set_text(code);
                }
            }
            UiEvent::OpenSetup => {
                let otp_list = setup_window(APP_STATE.load(), event_tx.clone());
                otp_setup_list = Some(otp_list);
            }
            UiEvent::OpenEntry(entry_action) => match entry_action {
                EntryAction::Add => {
                    otp_entry_window(&Default::default(), entry_action, event_tx.clone())
                }
                EntryAction::Edit(selected_row) => otp_entry_window(
                    &APP_STATE.load().otp_entries[selected_row],
                    entry_action,
                    event_tx.clone(),
                ),
            },
            UiEvent::SaveEntry(entry, entry_action) => {
                log::info!("Saving: {:?}", entry);
                let app_state = APP_STATE.load().save_entry(entry, entry_action);
                if let Some(ref mut otp_list) = otp_setup_list {
                    build_otp_list(otp_list, &app_state.otp_entries);
                }
                if let Err(err) = app_state.save_to_config() {
                    log::error!("Failed to save configuration file: {:?}", err);
                }
                APP_STATE.store(app_state);
                let _ = event_tx.send(UiEvent::TotpRefresh);
            }
            UiEvent::RemoveEntry(selected_row) => {
                log::info!("Removing entry at index: {}", selected_row);
                let app_state = APP_STATE.load().remove_entry_index(selected_row);
                if let Some(ref mut otp_list) = otp_setup_list {
                    build_otp_list(otp_list, &app_state.otp_entries);
                }
                if let Err(err) = app_state.save_to_config() {
                    log::error!("Failed to save configuration file: {:?}", err);
                }
                APP_STATE.store(app_state);
                let _ = event_tx.send(UiEvent::TotpRefresh);
            }
            UiEvent::Quit => {
                gtk::main_quit();
            }
        };

        Continue(true)
    });

    let _ = tx.send(UiEvent::TotpRefresh);
    gtk::main();
}
