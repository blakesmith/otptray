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

use objc::{msg_send, sel};
use objc::declare::ClassDecl;
use objc::runtime::{Object, Sel, Class};

pub extern "C" fn menu_selected(_this: &Object, _sel: Sel, target: id) {
    let tag: i64 = unsafe { msg_send![target, tag] };
    log::info!("Selected menu item: {}", tag);
}

lazy_static! {
    static ref EVENT_RESPONDER: &'static Class = {
        let superclass = Class::get("NSObject").unwrap();
        let mut class_decl = ClassDecl::new("EventResponder", superclass).unwrap();
        unsafe { class_decl.add_method(sel!(menu_selected:), menu_selected as extern "C" fn(&Object, Sel, id)); }
        class_decl.register()
    };
}

fn build_menu(app_state: Arc<AppState>) -> (AppState, id) {
    let new_app_state = app_state.menu_reset();
    let responder: id = unsafe { msg_send![*EVENT_RESPONDER, new] };
    unsafe {
        let menu = NSMenu::new(nil).autorelease();

        for entry in &app_state.otp_entries {
            let action = sel!(menu_selected:);
            let otp_value = entry.get_otp_value();
            let entry_label = NSString::alloc(nil).init_str(&otp_value.formatted_menu_display()).autorelease();
            let entry_item = NSMenuItem::alloc(nil)
                .initWithTitle_action_keyEquivalent_(entry_label, action, NSString::alloc(nil).init_str("").autorelease())
                .autorelease();
            NSMenuItem::setTarget_(entry_item, responder);
            let _: () = msg_send![entry_item, setTag: 123]; // TODO: Pick a unique ID here.
            menu.addItem_(entry_item);
        }

        let quit_prefix = NSString::alloc(nil).init_str("Quit ").autorelease();
        let quit_title =
            quit_prefix.stringByAppendingString_(NSProcessInfo::processInfo(nil).processName());
        let quit_action = selector("terminate:");
        let quit_key = NSString::alloc(nil).init_str("q").autorelease();
        let quit_item = NSMenuItem::alloc(nil)
            .initWithTitle_action_keyEquivalent_(quit_title, quit_action, quit_key)
            .autorelease();
        menu.addItem_(quit_item);
        responder.autorelease();

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
        status_button.setTitle_(NSString::alloc(nil).init_str("otp").autorelease());

        // TODO: Move to TotpRefresh UIEvent
        let (app_state, menu) = build_menu(global_app_state.load());
        status_item.setMenu_(menu);

        app.run();
    }
}
