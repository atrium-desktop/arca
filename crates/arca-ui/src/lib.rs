//! UI layer for Arca, built on the optics stack (`optics-ui` widgets in an
//! `optics-window` window, driven by the `optics-app` application shell). The
//! business logic lives in `arca-engine`; this crate renders [`AppState`] and
//! forwards input events to it.

mod app;
pub mod chooser;
mod content;
mod device;
mod icons;
pub mod ipc;
mod menus;
mod preview;
mod sidebar;
mod statusbar;
mod tabs;
mod theme;
mod thumbs;
mod toolbar;

#[cfg(test)]
pub(crate) mod test_support;

pub use app::UiApp;
pub use chooser::run_chooser;

use arca_engine::state::AppState;

/// Open the Arca window and run the event loop until it closes.
pub fn run(state: AppState) -> Result<(), optics_app::RunError> {
    // The socket must outlive the event loop and be removed on every exit
    // path (window close, Ctrl+W on the last tab, Ctrl+Q). Holding it in
    // `run`'s frame does exactly that: `IpcServer::drop` unlinks the socket
    // before `run` returns.
    let ipc_server = ipc::IpcServer::start();
    if ipc_server.is_none() {
        log::warn!(target: "arca_ui", "single-instance socket unavailable; paths will not be forwarded here");
    }
    let app = std::rc::Rc::new(std::cell::RefCell::new(
        UiApp::with_ipc(state, ipc_server).surface_alerts(),
    ));
    let app_start = app.clone();
    let app_build = app.clone();
    let app_stop = app.clone();
    let config = optics_app::Config::new("Arca")?
        .app_id("io.arca.Arca")?
        .size(1040, 700);
    let result = optics_app::Application::run_with_lifecycle(
        config,
        // Borrow the optics-window-owned device for thumbnail uploads and
        // register the icon set on the window's ui context.
        Some(move |host: optics_app::StartHost| {
            device::set_device(host.optics_gfx_device().as_raw());
            app_start.borrow_mut().start(&mut host.ui());
            log::info!(target: "arca_ui", "window started");
            true
        }),
        // Cleanly release GPU textures and icons before the shell tears down
        // the device and ui context (ADR-0045).
        Some(move |host: optics_app::StopHost| {
            app_stop.borrow_mut().shutdown(&mut host.ui());
            device::clear_device();
        }),
        move |frame, input| app_build.borrow_mut().build(frame, input),
        // Arca is pure optics-ui chrome: it records no host canvas content, so the
        // optional recorder stays empty and the frame has no host display
        // list to composite under the widgets (Optics ADR-0127).
        None::<fn(optics_app::RecordHost)>,
    );
    match &result {
        Ok(()) => log::info!(target: "arca_ui", "window closed"),
        Err(error) => log::error!(target: "arca_ui", "window failed: {error}"),
    }
    result
}
