use atomic_immut::AtomicImmut;
use core::ffi::c_void;
use log;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use crate::common::*;

use cocoa::appkit::{
    NSApp, NSApplication, NSApplicationActivationPolicyRegular, NSButton, NSMenu, NSMenuItem,
    NSPasteboard, NSSquareStatusItemLength, NSStatusBar, NSStatusItem,
};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSArray, NSAutoreleasePool, NSProcessInfo, NSString};

use objc::declare::ClassDecl;
use objc::rc::StrongPtr;
use objc::runtime::{Class, Object, Sel, YES};
use objc::{class, msg_send, sel};

lazy_static! {
    static ref EVENT_RESPONDER_CLASS: &'static Class = {
        let superclass = class!(NSObject);
        let mut class_decl = ClassDecl::new("EventResponder", superclass).unwrap();
        unsafe {
            class_decl.add_ivar::<*mut c_void>("rust_responder");
            class_decl.add_method(
                sel!(menu_selected:),
                EventResponder::menu_selected as extern "C" fn(&Object, Sel, id),
            );
            class_decl.add_method(
                sel!(totp_refresh),
                EventResponder::totp_refresh as extern "C" fn(&Object, Sel),
            );
            class_decl.add_method(
                sel!(quit),
                EventResponder::quit as extern "C" fn(&Object, Sel),
            );
        }
        class_decl.register()
    };
}

struct EventResponder {
    obj_c_responder: Option<id>,
    status_item: Option<StrongPtr>,
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
        Self {
            obj_c_responder: None,
            status_item: None,
            global_app_state,
            tx,
            rx,
        }
    }

    fn instantiate_obj_c_responder(&mut self) {
        let obj_c_responder: id = unsafe { msg_send![*EVENT_RESPONDER_CLASS, new] };
        unsafe {
            let responder_ptr: *mut c_void = self as *mut _ as *mut c_void;
            (&mut *obj_c_responder).set_ivar("rust_responder", responder_ptr);
        }
        self.obj_c_responder = Some(obj_c_responder);
    }

    pub extern "C" fn menu_selected(this: &Object, _sel: Sel, target: id) {
        let menu_item_id: i64 = unsafe { msg_send![target, tag] };
        let responder = Self::rust_responder(this);
        let _ = &responder
            .tx
            .send(UiEvent::CopyToClipboard(menu_item_id as u64));

        process_events(responder);
    }

    pub extern "C" fn totp_refresh(this: &Object, _sel: Sel) {
        let responder = Self::rust_responder(this);
        let _ = &responder.tx.send(UiEvent::TotpRefresh);

        process_events(responder);
    }

    pub extern "C" fn quit(this: &Object, _sel: Sel) {
        let responder = Self::rust_responder(this);
        let _ = &responder.tx.send(UiEvent::Quit);

        process_events(responder);
    }

    fn rust_responder(this: &Object) -> &mut EventResponder {
        unsafe {
            let responder_ptr = *this.get_ivar::<*mut c_void>("rust_responder");
            if responder_ptr.is_null() {
                panic!("Got back a null rust responder pointer. This should never happen!");
            }
            &mut *(responder_ptr as *mut EventResponder)
        }
    }
}

impl Drop for EventResponder {
    fn drop(&mut self) {
        unsafe {
            if let Some(obj_c_responder) = self.obj_c_responder {
                obj_c_responder.autorelease();
            }
        }
    }
}

fn build_menu(app_state: Arc<AppState>, event_responder: &EventResponder) -> (AppState, id) {
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
            NSMenuItem::setTarget_(
                entry_item,
                event_responder
                    .obj_c_responder
                    .expect("No objective-c EventResponder instantiated!"),
            );
            let _: () = msg_send![entry_item, setTag: i];
            menu.addItem_(entry_item);
        }

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let quit_prefix = NSString::alloc(nil).init_str("Quit ").autorelease();
        let quit_title =
            quit_prefix.stringByAppendingString_(NSProcessInfo::processInfo(nil).processName());
        let quit_action = sel!(quit);
        let quit_key = NSString::alloc(nil).init_str("q").autorelease();
        let quit_item = NSMenuItem::alloc(nil)
            .initWithTitle_action_keyEquivalent_(quit_title, quit_action, quit_key)
            .autorelease();
        NSMenuItem::setTarget_(quit_item, event_responder.obj_c_responder.unwrap());
        menu.addItem_(quit_item);

        (new_app_state, menu)
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

fn process_events(event_responder: &mut EventResponder) {
    while let Ok(event) = event_responder.rx.try_recv() {
        log::debug!("Got event: {:?}", event);
        match event {
            UiEvent::CopyToClipboard(menu_id) => {
                let app_state = event_responder.global_app_state.load();
                if let Some(otp_value) = app_state.get_otp_value_at_index(menu_id as usize) {
                    copy_to_pasteboard(&otp_value.otp);
                }
            }
            UiEvent::Quit => {
                unsafe {
                    let app = NSApplication::sharedApplication(nil);
                    let _: () = msg_send![app, terminate: nil];
                }
                return;
            }
            UiEvent::TotpRefresh => unsafe {
                let status_item = match &event_responder.status_item {
                    Some(s) => **s,
                    None => {
                        let status_bar = NSStatusBar::systemStatusBar(nil);
                        let status_item = StrongPtr::retain(
                            status_bar.statusItemWithLength_(NSSquareStatusItemLength),
                        );
                        let status_button = status_item.button();
                        event_responder.status_item = Some(status_item.clone());
                        status_button.setTitle_(NSString::alloc(nil).init_str("otp").autorelease());
                        *status_item
                    }
                };
                let (app_state, menu) =
                    build_menu(event_responder.global_app_state.load(), event_responder);
                status_item.setMenu_(menu);
                event_responder.global_app_state.store(app_state);
            },
            _ => {}
        }
    }
}

fn start_timer(event_responder: &EventResponder) {
    unsafe {
        let _: () = msg_send![class!(NSTimer),
                              scheduledTimerWithTimeInterval: 5.0
                              target: event_responder.obj_c_responder.unwrap()
                              selector: sel!(totp_refresh)
                              userInfo: nil
                              repeats: YES];
    }
}

pub fn ui_main(global_app_state: Arc<AtomicImmut<AppState>>) {
    log::info!("Staring macOS ui main");
    let (tx, rx) = channel();
    let mut event_responder = EventResponder::new(global_app_state, tx.clone(), rx);
    event_responder.instantiate_obj_c_responder();

    unsafe {
        let app = NSApp();
        app.setActivationPolicy_(NSApplicationActivationPolicyRegular);
        let _ = tx.send(UiEvent::TotpRefresh);
        process_events(&mut event_responder);
        start_timer(&event_responder);
        app.run();
    }
}
