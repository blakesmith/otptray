use atomic_immut::AtomicImmut;
use log;
use std::sync::Arc;

use crate::common::*;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyRegular, NSButton, NSMenu, NSMenuItem,
    NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
};
use cocoa::base::{nil, selector, id};
use cocoa::foundation::{NSAutoreleasePool, NSProcessInfo, NSString};

use objc::sel;
use objc::runtime::{Object, Sel};

pub extern fn menu_selected(_menu_item: &Object, _sel: Sel) {
    log::info!("Selected menu item");
}

fn build_menu(app_state: Arc<AppState>) -> (AppState, id) {
    let new_app_state = app_state.menu_reset();
    let menu_selector = sel!(menu_selected);
    unsafe {
        let menu = NSMenu::new(nil).autorelease();

        for entry in &app_state.otp_entries {
            let otp_value = entry.get_otp_value();
            let entry_label = NSString::alloc(nil).init_str(&otp_value.formatted_menu_display());
            let entry_item = NSMenuItem::alloc(nil)
                .initWithTitle_action_keyEquivalent_(entry_label, menu_selector, NSString::alloc(nil).init_str(""))
                .autorelease();
            menu.addItem_(entry_item);
        }

        let quit_prefix = NSString::alloc(nil).init_str("Quit ");
        let quit_title =
            quit_prefix.stringByAppendingString_(NSProcessInfo::processInfo(nil).processName());
        let quit_action = selector("terminate:");
        let quit_key = NSString::alloc(nil).init_str("q");
        let quit_item = NSMenuItem::alloc(nil)
            .initWithTitle_action_keyEquivalent_(quit_title, quit_action, quit_key)
            .autorelease();
        menu.addItem_(quit_item);

        (new_app_state, menu)
    }
}

pub fn ui_main(global_app_state: Arc<AtomicImmut<AppState>>) {
    log::info!("Staring macOS ui main");

    unsafe {
        let _pool = NSAutoreleasePool::new(nil);
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyRegular);

        let status_bar = NSStatusBar::systemStatusBar(nil);
        let status_item = status_bar.statusItemWithLength_(NSSquareStatusItemLength);
        let status_button = status_item.button();
        status_button.setTitle_(NSString::alloc(nil).init_str("otp"));

        // TODO: Move to TotpRefresh UIEvent
        let (app_state, menu) = build_menu(global_app_state.load());
        status_item.setMenu_(menu);

        app.run();
    }
}
