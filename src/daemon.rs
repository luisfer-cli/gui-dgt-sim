//! A non-blocking client for the `dgt-sim` control socket.
//!
//! The egui UI must never block on I/O, so every request is handed to a
//! background thread that performs the blocking Unix-socket round-trip and
//! posts the answer back. The UI drains the answers once per frame.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crate::protocol::{Request, Response};

/// What a response should be applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    /// Daemon status.
    Info,
    /// A freshly created board.
    Create,
    /// The full state of the tab with this id.
    Snapshot(u64),
    /// The message log of the tab with this id.
    Messages(u64),
    /// A mutation whose reply is the resulting `Snapshot`.
    Mutate(u64),
    /// A command whose success or failure is all that matters.
    Plain(u64),
}

/// A completed request.
pub struct Event {
    pub tag: Tag,
    pub result: Result<Response, String>,
}

struct Job {
    request: Request,
    tag: Tag,
}

/// Handle to the background control-socket worker.
pub struct Daemon {
    socket: PathBuf,
    jobs: Sender<Job>,
    events: Receiver<Event>,
}

impl Daemon {
    /// Spawn the worker thread. `ctx` is used to wake the UI when an answer
    /// arrives.
    pub fn connect(ctx: &egui::Context, socket: PathBuf) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (event_tx, event_rx) = mpsc::channel::<Event>();
        let worker_socket = socket.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                let result = call(&worker_socket, &job.request);
                if event_tx
                    .send(Event {
                        tag: job.tag,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self {
            socket,
            jobs: job_tx,
            events: event_rx,
        }
    }

    /// The socket this client talks to.
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Queue a request. The answer arrives from [`Daemon::poll`].
    pub fn send(&self, request: Request, tag: Tag) {
        let _ = self.jobs.send(Job { request, tag });
    }

    /// Take every answer that has arrived since the last call.
    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Stop the given boards synchronously, on the calling thread. Used on exit,
    /// where the background worker may not get a chance to run.
    pub fn stop_boards_blocking(&self, boards: &[u32]) {
        for &board in boards {
            let _ = call(&self.socket, &Request::Stop { board });
        }
    }
}

/// One blocking round-trip: connect, write one JSON line, read one back.
fn call(socket: &Path, request: &Request) -> Result<Response, String> {
    let stream = UnixStream::connect(socket).map_err(|err| {
        format!(
            "no se pudo conectar a {}: {err} (¿está el daemon en marcha?)",
            socket.display()
        )
    })?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let mut payload = serde_json::to_vec(request).map_err(|err| err.to_string())?;
    payload.push(b'\n');

    let mut writer = stream.try_clone().map_err(|err| err.to_string())?;
    writer.write_all(&payload).map_err(|err| err.to_string())?;
    writer.flush().map_err(|err| err.to_string())?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|err| err.to_string())?;
    if line.trim().is_empty() {
        return Err("el daemon cerró la conexión sin responder".into());
    }
    serde_json::from_str(&line).map_err(|err| format!("respuesta inválida: {err}"))
}
