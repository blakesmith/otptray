use atomic_immut::AtomicImmut;
use core::ffi::c_void;
use log;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use crate::common::*;

use cocoa::appkit::{
    NSApp, NSApplication, NSBackingStoreType, NSButton, NSMenu, NSMenuItem, NSPasteboard,
    NSSquareStatusItemLength, NSStatusBar, NSStatusItem, NSTabView, NSTabViewItem, NSView,
    NSViewHeightSizable, NSViewWidthSizable, NSWindow, NSWindowStyleMask,
};
use cocoa::base::{id, nil, SEL};
use cocoa::foundation::{NSArray, NSAutoreleasePool, NSPoint, NSRect, NSSize, NSString};

use objc::declare::ClassDecl;
use objc::rc::StrongPtr;
use objc::runtime::{Class, Object, Sel, NO, YES};
use objc::{class, msg_send, sel};

lazy_static! {
    /// This is the event target that we use to translate
    /// high level UI events between Rust and Objective-C
    /// events.
    static ref EVENT_RESPONDER_CLASS: &'static Class = {
        let mut class_decl = ClassDecl::new("EventResponder", class!(NSObject)).unwrap();
        class_decl.add_ivar::<*mut c_void>("rust_responder");

        unsafe {
            class_decl.add_method(
                sel!(menu_selected:),
                EventResponder::menu_selected as extern "C" fn(&Object, Sel, id),
            );
            class_decl.add_method(
                sel!(totp_refresh),
                EventResponder::totp_refresh as extern "C" fn(&Object, Sel),
            );
            class_decl.add_method(
                sel!(setup),
                EventResponder::setup as extern "C" fn(&Object, Sel),
            );
            class_decl.add_method(
                sel!(open_entry:),
                EventResponder::open_entry as extern "C" fn(&Object, Sel, id),
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
    obj_c_responder: Option<StrongPtr>,
    status_item: Option<StrongPtr>,
    otp_setup_list: OtpSetupList,
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
        let otp_setup_list = OtpSetupList::new(global_app_state.load());
        Self {
            obj_c_responder: None,
            status_item: None,
            global_app_state,
            otp_setup_list,
            tx,
            rx,
        }
    }

    fn instantiate_obj_c_responder(&mut self) {
        let obj_c_responder: id = unsafe { msg_send![*EVENT_RESPONDER_CLASS, new] };
        unsafe {
            let responder_ptr: *mut c_void = self as *mut _ as *mut c_void;
            (&mut *obj_c_responder).set_ivar("rust_responder", responder_ptr);
            self.obj_c_responder = Some(StrongPtr::new(obj_c_responder));
        }
        self.otp_setup_list.instantiate_obj_c_setup_list();
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

    pub extern "C" fn open_entry(this: &Object, _sel: Sel, sender: id) {
        unsafe {
            let selected_segment: i64 = msg_send![sender, selectedSegment];
            let responder = Self::rust_responder(this);
            if let Some(event) = match selected_segment {
                0 => Some(UiEvent::OpenEntry(EntryAction::Add)),
                1 => responder
                    .otp_setup_list
                    .selected_item
                    .map(|selected| UiEvent::OpenEntry(EntryAction::Edit(selected))),
                2 => responder
                    .otp_setup_list
                    .selected_item
                    .map(|selected| UiEvent::RemoveEntry(selected)),
                _ => None,
            } {
                let _ = &responder.tx.send(event);
                process_events(responder);
            }
        }
    }

    pub extern "C" fn setup(this: &Object, _sel: Sel) {
        let responder = Self::rust_responder(this);
        let _ = &responder.tx.send(UiEvent::OpenSetup);

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

lazy_static! {
    /// This is the required NSTableViewDataSource class that we need
    /// to populate the NSTableView for the OTP list during setup / configuration.
    static ref OTP_SETUP_LIST_CLASS: &'static Class = {
        let mut class_decl = ClassDecl::new("OtpSetupList", class!(NSObject)).unwrap();
        class_decl.add_ivar::<*mut c_void>("rust_otp_setup_list");

        unsafe {
            class_decl.add_method(
                sel!(numberOfRowsInTableView:),
                OtpSetupList::number_of_rows_in as extern "C" fn(&Object, Sel, id) -> i64,
            );

            class_decl.add_method(
                sel!(tableView:objectValueForTableColumn:row:),
                OtpSetupList::table_view_object_value_for as extern "C" fn(&Object, Sel, id, id, i64) -> id,
            );

            class_decl.add_method(
                sel!(tableViewSelectionDidChange:),
                OtpSetupList::table_view_selection_did_change as extern "C" fn(&Object, Sel, id)
            );
        }
        class_decl.register()
    };
}

struct OtpSetupList {
    app_state: Arc<AppState>,
    obj_c_setup_list: Option<StrongPtr>,
    selected_item: Option<usize>,
}

impl OtpSetupList {
    fn new(app_state: Arc<AppState>) -> Self {
        Self {
            app_state,
            obj_c_setup_list: None,
            selected_item: None,
        }
    }

    fn instantiate_obj_c_setup_list(&mut self) {
        let obj_c_setup_list: id = unsafe { msg_send![*OTP_SETUP_LIST_CLASS, new] };
        unsafe {
            let otp_setup_list_ptr: *mut c_void = self as *mut _ as *mut c_void;
            (&mut *obj_c_setup_list).set_ivar("rust_otp_setup_list", otp_setup_list_ptr);
            self.obj_c_setup_list = Some(StrongPtr::new(obj_c_setup_list));
        }
    }

    fn rust_setup_list(this: &Object) -> &mut OtpSetupList {
        unsafe {
            let setup_list_ptr = *this.get_ivar::<*mut c_void>("rust_otp_setup_list");
            if setup_list_ptr.is_null() {
                panic!("Got back a null rust OTP Setup list pointer. This should never happen!");
            }
            &mut *(setup_list_ptr as *mut OtpSetupList)
        }
    }

    /// Return the row count of the table
    pub extern "C" fn number_of_rows_in(this: &Object, _sel: Sel, _table_view: id) -> i64 {
        let setup_list = Self::rust_setup_list(this);
        setup_list.app_state.otp_entries.len() as i64
    }

    pub extern "C" fn table_view_object_value_for(
        this: &Object,
        _sel: Sel,
        _table_view: id,
        _tabe_column: id,
        row: i64,
    ) -> id {
        let setup_list = Self::rust_setup_list(this);
        let otp_entry = &setup_list.app_state.otp_entries[row as usize];
        unsafe { NSString::alloc(nil).init_str(&otp_entry.name).autorelease() }
    }

    pub extern "C" fn table_view_selection_did_change(this: &Object, _sel: Sel, notification: id) {
        unsafe {
            let table_view: id = msg_send![notification, object];
            let selected_row_index: i64 = msg_send![table_view, selectedRow];
            let mut setup_list = Self::rust_setup_list(this);
            setup_list.selected_item = match selected_row_index {
                -1 => None,
                index => Some(index as usize),
            };
            log::debug!("Got selection change. Row index: {}", selected_row_index);
        }
    }
}

fn setup_page(event_responder: &mut EventResponder, frame: NSRect) -> id {
    let frame_with_margin = NSRect::new(
        NSPoint::new(0.0, 30.0),
        NSSize::new(frame.size.width, frame.size.height - 60.0),
    );
    unsafe {
        let table_box: id = msg_send![class!(NSBox), alloc];
        let _: () = msg_send![table_box, initWithFrame: frame];
        let _: () = msg_send![table_box, setTitle: NSString::alloc(nil).init_str("One-Time Password Setup").autorelease() ];
        let _: () = msg_send![table_box, setBorderType: 0]; // NSBorderType.noBorder
        table_box.autorelease();

        let scroll_view: id = msg_send![class!(NSScrollView), alloc];
        let _: () = msg_send![scroll_view, initWithFrame: frame_with_margin];
        NSView::setAutoresizingMask_(scroll_view, NSViewWidthSizable | NSViewHeightSizable);
        scroll_view.autorelease();

        let table_view: id = msg_send![class!(NSTableView), alloc];
        let _: () = msg_send![table_view, initWithFrame: frame];
        let _: () = msg_send![table_view, setHeaderView: nil];
        let _: () = msg_send![scroll_view, setDocumentView: table_view];
        let _: () = msg_send![table_box, addSubview: scroll_view];

        let otp_objc = event_responder
            .otp_setup_list
            .obj_c_setup_list
            .as_ref()
            .expect("Must have instantiated the OTP setup list by now!");
        let _: () = msg_send![table_view, setDataSource: **otp_objc];
        let _: () = msg_send![table_view, setDelegate: **otp_objc];
        table_view.autorelease();

        let column: id = msg_send![class!(NSTableColumn), alloc];
        let _: () = msg_send![column, initWithIdentifier: NSString::alloc(nil).init_str("Name").autorelease() ];
        let _: () =
            msg_send![column, setTitle: NSString::alloc(nil).init_str("Name").autorelease() ];
        let _: () = msg_send![column, setEditable: NO];
        column.autorelease();

        let _: () = msg_send![table_view, addTableColumn: column];

        let event_responder_objc = event_responder
            .obj_c_responder
            .as_ref()
            .expect("Must have instantiated the event responder by now!");

        let add_label: id = NSString::alloc(nil).init_str("Add").autorelease();
        let edit_label: id = NSString::alloc(nil).init_str("Edit").autorelease();
        let remove_label: id = NSString::alloc(nil).init_str("Remove").autorelease();
        let button_segment: id = msg_send![class!(NSSegmentedControl), alloc];
        let _: () = msg_send![button_segment, initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(frame.size.width, 10.0))];
        let _: () = msg_send![button_segment, setTarget: **event_responder_objc];
        let _: () = msg_send![button_segment, setAction: sel!(open_entry:)];
        let _: () = msg_send![button_segment, setSegmentCount: 3];
        let _: () = msg_send![button_segment, setLabel: add_label forSegment: 0 ];
        let _: () = msg_send![button_segment, setLabel: edit_label forSegment: 1 ];
        let _: () = msg_send![button_segment, setLabel: remove_label forSegment: 2 ];
        let _: () = msg_send![button_segment, sizeToFit];
        let _: () = msg_send![table_box, addSubview: button_segment];
        button_segment.autorelease();

        table_box
    }
}

fn otp_entry_window(
    otp_entry: &OtpEntry,
    entry_action: EntryAction,
    event_responder: &mut EventResponder,
) -> id {
    unsafe {
        let mut window_mask = NSWindowStyleMask::empty();
        window_mask.insert(NSWindowStyleMask::NSTitledWindowMask);
        window_mask.insert(NSWindowStyleMask::NSClosableWindowMask);
        window_mask.insert(NSWindowStyleMask::NSResizableWindowMask);
        let content_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(350.0, 300.0));
        let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            content_frame,
            window_mask,
            NSBackingStoreType::NSBackingStoreBuffered,
            NO,
        );
        window.center();
        NSWindow::setTitle_(
            window,
            NSString::alloc(nil)
                .init_str(entry_action.window_title())
                .autorelease(),
        );
        window
    }
}

fn setup_window(event_responder: &mut EventResponder) -> id {
    unsafe {
        let mut window_mask = NSWindowStyleMask::empty();
        window_mask.insert(NSWindowStyleMask::NSTitledWindowMask);
        window_mask.insert(NSWindowStyleMask::NSClosableWindowMask);
        window_mask.insert(NSWindowStyleMask::NSResizableWindowMask);
        let content_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(350.0, 300.0));
        let window = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            content_frame,
            window_mask,
            NSBackingStoreType::NSBackingStoreBuffered,
            NO,
        );
        window.center();
        NSWindow::setTitle_(
            window,
            NSString::alloc(nil).init_str("OTPTray Setup").autorelease(),
        );

        let tab_view = NSTabView::initWithFrame_(NSTabView::new(nil), content_frame).autorelease();
        let setup_item = NSTabViewItem::alloc(nil)
            .initWithIdentifier_(nil)
            .autorelease();
        setup_item.setLabel_(NSString::alloc(nil).init_str("Setup").autorelease());
        setup_item.setView_(setup_page(event_responder, content_frame));
        tab_view.addTabViewItem_(setup_item);

        let about_item = NSTabViewItem::alloc(nil)
            .initWithIdentifier_(nil)
            .autorelease();
        about_item.setLabel_(NSString::alloc(nil).init_str("About").autorelease());
        tab_view.addTabViewItem_(about_item);

        NSView::setAutoresizingMask_(tab_view, NSViewWidthSizable | NSViewHeightSizable);
        NSView::addSubview_(window.contentView(), tab_view);

        window
    }
}

fn build_menu_item(name: &str, action: SEL, target: id) -> id {
    unsafe {
        let menu_item_title = NSString::alloc(nil).init_str(name).autorelease();
        let menu_item_key = NSString::alloc(nil).init_str("").autorelease();
        let menu_item = NSMenuItem::alloc(nil)
            .initWithTitle_action_keyEquivalent_(menu_item_title, action, menu_item_key)
            .autorelease();
        NSMenuItem::setTarget_(menu_item, target);
        menu_item
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
                **event_responder
                    .obj_c_responder
                    .as_ref()
                    .expect("No objective-c EventResponder instantiated!"),
            );
            let _: () = msg_send![entry_item, setTag: i];
            menu.addItem_(entry_item);
        }

        menu.addItem_(NSMenuItem::separatorItem(nil));

        let setup_item = build_menu_item(
            "Setup",
            sel!(setup),
            **event_responder.obj_c_responder.as_ref().unwrap(),
        );
        let quit_item = build_menu_item(
            "Quit",
            sel!(quit),
            **event_responder.obj_c_responder.as_ref().unwrap(),
        );
        menu.addItem_(setup_item);
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
            UiEvent::OpenSetup => unsafe {
                let app = NSApplication::sharedApplication(nil);
                let window = setup_window(event_responder);
                NSApplication::activateIgnoringOtherApps_(app, YES);
                window.makeKeyAndOrderFront_(app);
                // Windows should automatically get released upon close
                // See: 'releaseWhenClosed' property.
            },
            UiEvent::OpenEntry(entry_action) => match entry_action {
                EntryAction::Add => unsafe {
                    let app = NSApplication::sharedApplication(nil);
                    let window =
                        otp_entry_window(&Default::default(), entry_action, event_responder);
                    window.makeKeyAndOrderFront_(app);
                },
                EntryAction::Edit(selected_row) => {}
            },
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
                        NSButton::setTitle_(
                            status_button,
                            NSString::alloc(nil).init_str("otp").autorelease(),
                        );
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
                              target: **event_responder.obj_c_responder.as_ref().unwrap()
                              selector: sel!(totp_refresh)
                              userInfo: nil
                              repeats: YES];
    }
}

pub fn ui_main(global_app_state: Arc<AtomicImmut<AppState>>, activation_policy: ActivationPolicy) {
    log::info!("Staring macOS ui main");
    let (tx, rx) = channel();
    let mut event_responder = EventResponder::new(global_app_state, tx.clone(), rx);
    event_responder.instantiate_obj_c_responder();

    unsafe {
        let app = NSApp();
        if activation_policy == ActivationPolicy::Foreground {
            app.setActivationPolicy_(cocoa::appkit::NSApplicationActivationPolicyRegular);
        }
        let _ = tx.send(UiEvent::TotpRefresh);
        process_events(&mut event_responder);
        start_timer(&event_responder);
        app.run();
    }
}
