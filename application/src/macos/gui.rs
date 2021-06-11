use atomic_immut::AtomicImmut;
use log;
use std::sync::Arc;

use crate::common::*;

use cocoa::base::{selector, nil};
use cocoa::foundation::{NSAutoreleasePool, NSString, NSProcessInfo};
use cocoa::appkit::{NSApp, NSApplication, NSButton, NSStatusBar, NSStatusItem, NSSquareStatusItemLength, NSMenu, NSMenuItem, NSApplicationActivationPolicyRegular};

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

        let menu = NSMenu::new(nil).autorelease();
        status_item.setMenu_(menu);

        let quit_prefix = NSString::alloc(nil).init_str("Quit ");
        let quit_title =
            quit_prefix.stringByAppendingString_(NSProcessInfo::processInfo(nil).processName());
        let quit_action = selector("terminate:");
        let quit_key = NSString::alloc(nil).init_str("q");
        let quit_item = NSMenuItem::alloc(nil)
            .initWithTitle_action_keyEquivalent_(quit_title, quit_action, quit_key)
            .autorelease();
        menu.addItem_(quit_item);
        app.run();
    }
}
