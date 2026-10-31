//! UI layer for Arca, built on the optics stack (`lens` widgets inside
//! an `iris` window). The business logic lives in `arca-engine`; this crate
//! renders [`AppState`] and forwards input events to it.

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
pub fn run(state: AppState) -> Result<(), iris::RunError> {
    // The socket must outlive the event loop and be removed on every exit
    // path (window close, Ctrl+W on the last tab, Ctrl+Q). Holding it in
    // `run`'s frame does exactly that: `IpcServer::drop` unlinks the socket
    // before `run` returns.
    let ipc_server = ipc::IpcServer::start();
    let app = std::rc::Rc::new(std::cell::RefCell::new(UiApp::with_ipc(state, ipc_server)));
    let app_build = app.clone();
    let app_stop = app.clone();
    let config = iris::Config::new("Arca")?
        .app_id("io.arca.Arca")?
        .size(1040, 700);
    let result = iris::Application::run_with_lifecycle(
        config,
        // Borrow iris's device for texture uploads (icon glyphs and thumbnails).
        Some(|host: iris::StartHost| {
            device::set_device(host.flux_device().as_raw());
            true
        }),
        // Cleanly release all GPU textures before iris tears down device (ADR-0045).
        Some(move |_host: iris::StopHost| {
            app_stop.borrow_mut().shutdown();
            device::clear_device();
        }),
        move |frame, input| app_build.borrow_mut().build(frame, input),
        None::<fn(iris::PaintHost)>,
    );
    result
}
