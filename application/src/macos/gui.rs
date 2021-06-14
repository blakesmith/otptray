use atomic_immut::AtomicImmut;
use core::ffi::c_void;
use log;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use crate::common::*;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyRegular, NSButton, NSMenu, NSMenuItem,
    NSPasteboard, NSPasteboardTypeString, NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
};
use cocoa::base::{id, nil, selector};
use cocoa::foundation::{NSArray, NSAutoreleasePool, NSProcessInfo, NSString};

use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{msg_send, sel};

lazy_static! {
    static ref EVENT_RESPONDER_CLASS: &'static Class = {
        let superclass = Class::get("NSObject").unwrap();
        let mut class_decl = ClassDecl::new("EventResponder", superclass).unwrap();
        unsafe {
            class_decl.add_ivar::<*mut c_void>("rust_responder");
            class_decl.add_method(
                sel!(menu_selected:),
                EventResponder::menu_selected as extern "C" fn(&Object, Sel, id),
            );
        }
        class_decl.register()
    };
}

struct EventResponder {
    obj_c_responder: id,
    global_app_state: Arc<AtomicImmut<AppState>>,
    tx: Sender<UiEvent>,
    rx: Receiver<UiEvent>,
}

impl EventResponder {
    fn new(
        global_app_state: Arc<AtomicImmut<AppState>>,
        tx: Sender<UiEvent>,
        rx: Receiver<UiEvent>,
    ) -> Self {
        let obj_c_responder: id = unsafe { msg_send![*EVENT_RESPONDER_CLASS, new] };
        let mut responder = Self {
            obj_c_responder,
            global_app_state,
            tx,
            rx,
        };
        unsafe {
            let responder_ptr: *mut c_void = &mut responder as *mut _ as *mut c_void;
            (&mut *obj_c_responder).set_ivar("rust_responder", responder_ptr);
        }

        responder
    }

    pub extern "C" fn menu_selected(this: &Object, _sel: Sel, target: id) {
        let menu_item_id: i64 = unsafe { msg_send![target, tag] };
        log::info!("Selected menu item: {}", menu_item_id);
        let responder = Self::rust_responder(this);
        let _ = &responder
            .tx
            .send(UiEvent::CopyToClipboard(menu_item_id as u64));
        responder.drain_events();
    }

    fn rust_responder(this: &Object) -> &mut EventResponder {
        unsafe { &mut *(*this.get_ivar::<*mut c_void>("rust_responder") as *mut EventResponder) }
    }

    fn drain_events(&self) {
        while let Ok(event) = self.rx.try_recv() {
            log::debug!("Got event: {:?}", event);
            match event {
                UiEvent::CopyToClipboard(menu_id) => {
                    let app_state = self.global_app_state.load();
                    if let Some(otp_value) = app_state.get_otp_value_at_index(menu_id as usize) {
                        Self::copy_to_pasteboard(&otp_value.otp);
                    }
                }
                _ => {}
            }
        }
    }

    fn copy_to_pasteboard(contents: &str) {
        unsafe {
            let ns_contents = NSString::alloc(nil).init_str(contents).autorelease();
            let array_contents = NSArray::arrayWithObjects(nil, &[ns_contents]);
            let pasteboard = NSPasteboard::generalPasteboard(nil);
            pasteboard.clearContents();
            pasteboard.writeObjects(array_contents);
        }
    }
}

impl Drop for EventResponder {
    fn drop(&mut self) {
        unsafe {
            self.obj_c_responder.autorelease();
        }
    }
}

fn build_menu(
    app_state: Arc<AppState>,
    event_responder: &mut EventResponder,
    tx: Sender<UiEvent>,
) -> (AppState, id) {
    let new_app_state = app_state.menu_reset();
    unsafe {
        let menu = NSMenu::new(nil).autorelease();

        for (i, entry) in app_state.otp_entries.iter().enumerate() {
            let action = sel!(menu_selected:);
            let otp_value = entry.get_otp_value();
            let entry_label = NSString::alloc(nil)
                .init_str(&otp_value.formatted_menu_display())
                .autorelease();
            let entry_item = NSMenuItem::alloc(nil)
                .initWithTitle_action_keyEquivalent_(
                    entry_label,
                    action,
                    NSString::alloc(nil).init_str("").autorelease(),
                )
                .autorelease();
            NSMenuItem::setTarget_(entry_item, event_responder.obj_c_responder);
            let _: () = msg_send![entry_item, setTag: i];
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
    let (tx, rx) = channel();
    let mut event_responder = EventResponder::new(global_app_state.clone(), tx.clone(), rx);

    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyRegular);

        let status_bar = NSStatusBar::systemStatusBar(nil);
        let status_item = status_bar.statusItemWithLength_(NSSquareStatusItemLength);
        let status_button = status_item.button();
        status_button.setTitle_(NSString::alloc(nil).init_str("otp").autorelease());

        // TODO: Move to TotpRefresh UIEvent
        let (app_state, menu) =
            build_menu(global_app_state.load(), &mut event_responder, tx.clone());
        status_item.setMenu_(menu);

        app.run();
    }
}
