use std::sync::{Arc, Mutex};
use std::sync::mpsc::Receiver;
use std::time::Duration;
use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, CheckButton, Orientation, Box as GtkBox, Label};
use crate::config::AppConfig;

pub fn run(config: Arc<Mutex<AppConfig>>, show_rx: Receiver<()>) {
    let app = Application::builder()
        .application_id("com.hytale.rpc.config")
        .build();

    let config_clone = config.clone();
    
    // We need to move the receiver into the closure, but timeout_add_local callback is FnMut.
    // mpsc Receiver is not Sync, but we are in local context (main thread).
    // Receiver is not Clone. We need to put it in a Rc<RefCell<...>> or similar to share?
    // Or just move it in once. But `connect_activate` can be called multiple times?
    // `Application` is a singleton mostly.
    // We can put it in a Shared state.
    
    // Use Rc<RefCell> for the receiver to be accessible in the callback
    use std::rc::Rc;
    use std::cell::RefCell;
    let rx = Rc::new(RefCell::new(show_rx));

    app.connect_activate(move |app| {
        let hold = app.hold();
        let app_clone = app.clone();
        let config_clone = config_clone.clone();
        let rx_clone = rx.clone();

        // Poll the channel every 100ms
        glib::timeout_add_local(Duration::from_millis(100), move || {
            // Keep the application alive
            let _ = &hold;

            // Try to read all pending events
            if let Ok(_) = rx_clone.borrow().try_recv() {
                // If we got a signal (or multiple), show config
                // Drain any extra signals to avoid queueing
                while rx_clone.borrow().try_recv().is_ok() {}

                if let Some(window) = app_clone.active_window() {
                    window.present();
                } else {
                    build_ui(&app_clone, &config_clone);
                }
            }
            glib::ControlFlow::Continue
        });
    });

    // Run the application
    app.run_with_args(&Vec::<String>::new());
}

fn build_ui(app: &Application, config: &Arc<Mutex<AppConfig>>) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Hytale RPC Settings")
        .default_width(300)
        .default_height(200)
        .resizable(false)
        .build();

    let vbox = GtkBox::new(Orientation::Vertical, 10);
    vbox.set_margin_top(20);
    vbox.set_margin_bottom(20);
    vbox.set_margin_start(20);
    vbox.set_margin_end(20);

    let title_label = Label::builder()
        .label("<span size='large'><b>Configuration</b></span>")
        .use_markup(true)
        .margin_bottom(10)
        .build();
    vbox.append(&title_label);

    let (show_world, show_ip) = {
        let cfg = config.lock().unwrap();
        (cfg.show_world_name, cfg.show_server_ip)
    };

    // Show World Name Checkbox
    let check_world = CheckButton::builder()
        .label("Show World Name")
        .active(show_world)
        .build();
    
    let config_world = config.clone();
    check_world.connect_toggled(move |btn| {
        if let Ok(mut cfg) = config_world.lock() {
            cfg.show_world_name = btn.is_active();
            let _ = cfg.save();
        }
    });
    vbox.append(&check_world);

    // Show Server IP Checkbox
    let check_ip = CheckButton::builder()
        .label("Show Server IP")
        .active(show_ip)
        .build();

    let config_ip = config.clone();
    check_ip.connect_toggled(move |btn| {
        if let Ok(mut cfg) = config_ip.lock() {
            cfg.show_server_ip = btn.is_active();
            let _ = cfg.save();
        }
    });
    vbox.append(&check_ip);

    // Footer
    let footer_label = Label::builder()
        .label("<small>Changes apply immediately</small>")
        .use_markup(true)
        .margin_top(20)
        .opacity(0.7)
        .build();
    vbox.append(&footer_label);

    window.set_child(Some(&vbox));
    window.present();
}
