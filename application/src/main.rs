#[macro_use]
extern crate lazy_static;

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use atomic_immut::AtomicImmut;
use clap::{App, Arg};
use simple_logger::SimpleLogger;
use std::sync::Arc;

pub mod common;

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
    let matches = App::new("OTPTrap")
        .author("Blake Smith")
        .about("Simple 2FA / OTP tray application")
        .arg(
            Arg::with_name("foreground")
                .help("Whether to launch the application in the foreground or not (OS X only)")
                .short("f"),
        )
        .get_matches();
    let activation_policy = if matches.is_present("foreground") {
        ActivationPolicy::Foreground
    } else {
        ActivationPolicy::Background
    };
    SimpleLogger::new().init().unwrap();
    let app_state = AppState::load_from_config().expect("Cannot load OTPTrap config!");
    APP_STATE.store(app_state);

    gui::ui_main(APP_STATE.clone(), activation_policy);
}
