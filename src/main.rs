//! A native GUI for `dgt-sim`: one tab per simulated DGT board, with a
//! draggable chessboard that drives the real control plane.
//!
//! The socket path is taken from the first argument, then `$DGT_SIM_SOCKET`,
//! then the same default the daemon uses.

mod app;
mod board_widget;
mod daemon;
mod fonts;
mod model;
mod protocol;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let socket = socket_from_args();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 840.0])
            .with_min_inner_size([860.0, 620.0])
            .with_title("dgt-sim GUI"),
        ..Default::default()
    };
    eframe::run_native(
        "dgt-sim GUI",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, socket)) as Box<dyn eframe::App>)),
    )
}

/// Resolve the control socket to connect to.
fn socket_from_args() -> PathBuf {
    if let Some(arg) = std::env::args().nth(1) {
        return PathBuf::from(arg);
    }
    if let Some(path) = std::env::var_os("DGT_SIM_SOCKET") {
        return PathBuf::from(path);
    }
    default_socket_path()
}

/// `$XDG_RUNTIME_DIR/dgt-sim.sock`, else `/tmp/dgt-sim-<uid>.sock`, matching
/// `dgt-sim`'s own default.
fn default_socket_path() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return dir.join("dgt-sim.sock");
        }
    }
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/dgt-sim-{uid}.sock"))
}
