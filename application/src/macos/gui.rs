use atomic_immut::AtomicImmut;
use log;
use std::sync::Arc;

use crate::common::*;

pub fn ui_main(global_app_state: Arc<AtomicImmut<AppState>>) {
    log::info!("Staring macOS ui main");
}
