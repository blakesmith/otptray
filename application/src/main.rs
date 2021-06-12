#[macro_use]
extern crate lazy_static;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use atomic_immut::AtomicImmut;
use simple_logger::SimpleLogger;
use std::sync::Arc;

mod common;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux::gui;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::gui;

use crate::common::*;

lazy_static! {
    static ref APP_STATE: Arc<AtomicImmut<AppState>> =
        Arc::new(AtomicImmut::new(Default::default()));
}

fn main() {
    SimpleLogger::new().init().unwrap();
    let app_state = AppState::load_from_config().expect("Cannot load OTPTrap config!");
    APP_STATE.store(app_state);

    gui::ui_main(APP_STATE.clone());
}
