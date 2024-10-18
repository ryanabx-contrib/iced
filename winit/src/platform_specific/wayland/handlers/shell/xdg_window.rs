use crate::platform_specific::wayland::event_loop::state::SctkState;
use sctk::{
    delegate_xdg_shell, delegate_xdg_window, shell::xdg::window::WindowHandler,
};

impl WindowHandler for SctkState {
    fn request_close(
        &mut self,
        _conn: &sctk::reexports::client::Connection,
        _qh: &sctk::reexports::client::QueueHandle<Self>,
        _window: &sctk::shell::xdg::window::Window,
    ) {
    }

    fn configure(
        &mut self,
        _conn: &sctk::reexports::client::Connection,
        _qh: &sctk::reexports::client::QueueHandle<Self>,
        _window: &sctk::shell::xdg::window::Window,
        _configure: sctk::shell::xdg::window::WindowConfigure,
        _serial: u32,
    ) {
    }
}

delegate_xdg_window!(SctkState);
delegate_xdg_shell!(SctkState);