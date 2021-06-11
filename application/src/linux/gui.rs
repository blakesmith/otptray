use gtk::prelude::*;

pub fn periodic_seconds_timer<F>(seconds: u32, mut f: F)
where
    F: FnMut() -> bool + 'static,
{
    glib::timeout_add_seconds_local(seconds, move || Continue(f()));
}
