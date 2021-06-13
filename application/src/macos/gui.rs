use atomic_immut::AtomicImmut;
use core::ffi::c_void;
use log;
use std::sync::Arc;
use std::sync::mpsc::{channel, Receiver, Sender};

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

pub extern "C" fn menu_selected(this: &Object, _sel: Sel, target: id) {
    let tag: i64 = unsafe { msg_send![target, tag] };
    log::info!("Selected menu item: {}", tag);
    let tx = extract_tx(this);
    let _ = tx.send(UiEvent::TotpRefresh);
    drain_events(extract_rx(this));
}

fn extract_tx(this: &Object) -> &mut Sender<UiEvent> {
    unsafe { &mut *(*this.get_ivar::<*mut c_void>("tx") as *mut Sender<UiEvent>) }
}

fn extract_rx(this: &Object) -> &mut Receiver<UiEvent> {
    unsafe { &mut *(*this.get_ivar::<*mut c_void>("rx") as *mut Receiver<UiEvent>) }
}

fn drain_events(rx: &mut Receiver<UiEvent>) {
    while let Ok(event) = rx.try_recv() {
        log::debug!("Got event: {:?}", event);
    }
}

pub extern "C" fn set_sender_receiver(this: &mut Object, _sel: Sel, tx: *mut c_void, rx: *mut c_void) {
    unsafe {
        this.set_ivar("tx", tx);
        this.set_ivar("rx", rx);
    }
}

lazy_static! {
    static ref EVENT_RESPONDER_CLASS: &'static Class = {
        let superclass = Class::get("NSObject").unwrap();
        let mut class_decl = ClassDecl::new("EventResponder", superclass).unwrap();
        unsafe {
            class_decl.add_ivar::<*mut c_void>("tx");
            class_decl.add_ivar::<*mut c_void>("rx");
            class_decl.add_method(sel!(menu_selected:), menu_selected as extern "C" fn(&Object, Sel, id));
            class_decl.add_method(sel!(set_sender_receiver:rx:), set_sender_receiver as extern "C" fn(&mut Object, Sel, *mut c_void, *mut c_void));
        }
        class_decl.register()
    };
}

fn build_menu(app_state: Arc<AppState>, event_responder: id, tx: Sender<UiEvent>) -> (AppState, id) {
    let new_app_state = app_state.menu_reset();
    unsafe {
        let menu = NSMenu::new(nil).autorelease();

        for entry in &app_state.otp_entries {
            let action = sel!(menu_selected:);
            let otp_value = entry.get_otp_value();
            let entry_label = NSString::alloc(nil).init_str(&otp_value.formatted_menu_display()).autorelease();
            let entry_item = NSMenuItem::alloc(nil)
                .initWithTitle_action_keyEquivalent_(entry_label, action, NSString::alloc(nil).init_str("").autorelease())
                .autorelease();
            NSMenuItem::setTarget_(entry_item, event_responder);
            let _: () = msg_send![entry_item, setTag: 123]; // TODO: Pick a unique ID here.
            menu.addItem_(entry_item);
        }

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let quit_prefix = NSString::alloc(nil).init_str("Quit ").autorelease();
        let quit_title =
            quit_prefix.stringByAppendingString_(NSProcessInfo::processInfo(nil).processName());
        let quit_action = selector("terminate:");
        let quit_key = NSString::alloc(nil).init_str("q").autorelease();
        let quit_item = NSMenuItem::alloc(nil)
            .initWithTitle_action_keyEquivalent_(quit_title, quit_action, quit_key)
            .autorelease();
        menu.addItem_(quit_item);

        (new_app_state, menu)
    }
}

pub fn ui_main(global_app_state: Arc<AtomicImmut<AppState>>) {
    log::info!("Staring macOS ui main");
    let (mut tx, mut rx) = channel();
    let event_responder: id = unsafe { msg_send![*EVENT_RESPONDER_CLASS, new] };
    let tx_ptr: *mut c_void = &mut tx.clone() as *mut _ as *mut c_void;
    let rx_ptr: *mut c_void = &mut rx as *mut _ as *mut c_void;
    let _: () = unsafe { msg_send![event_responder, set_sender_receiver:tx_ptr rx: rx_ptr] };

    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyRegular);

        let status_bar = NSStatusBar::systemStatusBar(nil);
        let status_item = status_bar.statusItemWithLength_(NSSquareStatusItemLength);
        let status_button = status_item.button();
        status_button.setTitle_(NSString::alloc(nil).init_str("otp").autorelease());

        // TODO: Move to TotpRefresh UIEvent
        let (app_state, menu) = build_menu(global_app_state.load(), event_responder, tx.clone());
        status_item.setMenu_(menu);

        app.run();
    }
}
