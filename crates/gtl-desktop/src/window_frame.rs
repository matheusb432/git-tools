use gtk::{gdk, glib::Propagation, prelude::*};

pub(super) fn configure(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let native_window = window.gtk_window()?;
    let titlebar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    // Keep GTK's frame without restoring its titlebar when Tauri calls show_all after tray hiding.
    titlebar.set_no_show_all(true);
    titlebar.hide();
    native_window.set_titlebar(Some(&titlebar));
    let content = window.default_vbox()?;
    // Tao consumes button-press-event before GTK's default frame handler runs.
    native_window.connect_event(move |window, event| resize_frame(window, &content, event));
    Ok(())
}

fn resize_frame(
    window: &gtk::ApplicationWindow,
    content: &gtk::Box,
    event: &gdk::Event,
) -> Propagation {
    if event.event_type() != gdk::EventType::ButtonPress
        || !window.is_resizable()
        || window.is_maximized()
    {
        return Propagation::Proceed;
    }
    let Some(button) = event.downcast_ref::<gdk::EventButton>() else {
        return Propagation::Proceed;
    };
    if button.button() != 1 {
        return Propagation::Proceed;
    }
    let (Some(native), Some(target), Some(device)) =
        (window.window(), event.window(), event.device())
    else {
        return Propagation::Proceed;
    };
    if native.state().contains(gdk::WindowState::FULLSCREEN)
        || !target.is_input_only()
        || target.parent().as_ref() != Some(&native)
    {
        return Propagation::Proceed;
    }
    let Some((x, y)) = content.translate_coordinates(window, 0, 0) else {
        return Propagation::Proceed;
    };
    let content = gtk::Allocation::new(x, y, content.allocated_width(), content.allocated_height());
    let (x, y, width, height) = target.geometry();
    let Some(edge) = resize_edge(content, gtk::Allocation::new(x, y, width, height)) else {
        return Propagation::Proceed;
    };
    let (x, y) = button.root();
    let root = tauri::LogicalPosition::new(x, y).cast::<i32>();
    native.begin_resize_drag_for_device(edge, &device, 1, root.x, root.y, button.time());
    Propagation::Stop
}

fn resize_edge(content: gtk::Allocation, handle: gtk::Allocation) -> Option<gdk::WindowEdge> {
    let left = handle.x() < content.x();
    let right = handle.x() + handle.width() > content.x() + content.width();
    let top = handle.y() < content.y();
    let bottom = handle.y() + handle.height() > content.y() + content.height();
    match (
        left && !right,
        right && !left,
        top && !bottom,
        bottom && !top,
    ) {
        (true, _, true, _) => Some(gdk::WindowEdge::NorthWest),
        (_, true, true, _) => Some(gdk::WindowEdge::NorthEast),
        (true, _, _, true) => Some(gdk::WindowEdge::SouthWest),
        (_, true, _, true) => Some(gdk::WindowEdge::SouthEast),
        (true, _, _, _) => Some(gdk::WindowEdge::West),
        (_, true, _, _) => Some(gdk::WindowEdge::East),
        (_, _, true, _) => Some(gdk::WindowEdge::North),
        (_, _, _, true) => Some(gdk::WindowEdge::South),
        _ => None,
    }
}
