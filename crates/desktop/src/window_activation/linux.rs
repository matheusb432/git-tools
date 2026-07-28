//! Sends an EWMH activation message on Linux.
//!
//! The OS-bound primitive is an EWMH
//! `_NET_ACTIVE_WINDOW` client message sent to the root window -- the only way
//! to pull a window to the foreground over a *focused fullscreen* window under
//! mutter/GNOME, whose focus-stealing-prevention silently demotes a plain
//! `gtk_window_present` to a taskbar flash. Sending with source indication `2`
//! (pager) is the documented bypass that `wmctrl -a` and `xdotool windowactivate`
//! rely on.
use std::ptr;

use x11_dl::xlib;

/// Source indication for `_NET_ACTIVE_WINDOW`: `2` = pager. Mutter trusts pager
/// requests unconditionally, bypassing focus-stealing-prevention; `1` (app) is
/// subject to it and gets demoted to "demands attention" over a fullscreen peer.
const SOURCE_PAGER: std::os::raw::c_long = 2;

/// Raises and focuses the X11 window `xid`, even over a focused fullscreen window.
///
/// Best-effort: opens its own short-lived display connection (so it is free of
/// GTK thread-affinity and safe to call from a worker thread) and swallows every
/// failure -- a missing libX11, a Wayland session, or a `0` id is a silent no-op.
/// The window must already be mapped; callers that just un-hid it should defer
/// this slightly so the map request has been processed by the WM.
pub(super) fn activate(xid: u64) {
    if xid == 0 {
        return;
    }
    let Ok(lib) = xlib::Xlib::open() else {
        return;
    };
    // SAFETY: standard Xlib FFI. Every pointer is checked before use and the
    // display is closed on every return path.
    unsafe {
        let display = (lib.XOpenDisplay)(ptr::null());
        if display.is_null() {
            return;
        }
        let screen = (lib.XDefaultScreen)(display);
        let root = (lib.XRootWindow)(display, screen);
        let net_active_window =
            (lib.XInternAtom)(display, c"_NET_ACTIVE_WINDOW".as_ptr(), xlib::False);

        let mut event: xlib::XEvent = std::mem::zeroed();
        let msg = &mut event.client_message;
        msg.type_ = xlib::ClientMessage;
        msg.send_event = xlib::True;
        msg.display = display;
        msg.window = xid as xlib::Window;
        msg.message_type = net_active_window;
        msg.format = 32;
        msg.data.set_long(0, SOURCE_PAGER);
        msg.data.set_long(1, 0); // timestamp: CurrentTime
        msg.data.set_long(2, 0); // requestor's currently active window: none

        let mask = xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask;
        (lib.XSendEvent)(display, root, xlib::False, mask, &raw mut event);
        (lib.XFlush)(display);
        (lib.XCloseDisplay)(display);
    }
}
