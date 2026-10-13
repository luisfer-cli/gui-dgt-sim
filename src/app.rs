//! The eframe application: one tab per simulated board.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Align, Color32, FontId, Layout, RichText};

use crate::board_widget::{self, BoardAction, DragState, Palette};
use crate::daemon::{Daemon, Tag};
use crate::fonts::PieceFont;
use crate::model::{self, Board, Color, Kind, Piece, Square};
use crate::protocol::{BoardSummary, Info, MessageLogEntry, Request, Snapshot};

/// How often the visible tab is refreshed.
const POLL_INTERVAL: Duration = Duration::from_millis(400);

/// Transport for newly created boards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransportChoice {
    Daemon,
    Pty,
    Tcp,
}

impl TransportChoice {
    fn label(self) -> &'static str {
        match self {
            Self::Daemon => "el del daemon",
            Self::Pty => "pty (/dev/pts)",
            Self::Tcp => "tcp (loopback)",
        }
    }

    fn wire(self) -> Option<String> {
        match self {
            Self::Daemon => None,
            Self::Pty => Some("pty".into()),
            Self::Tcp => Some("tcp".into()),
        }
    }
}

/// One board, with its own view state.
struct Tab {
    id: u64,
    board: u32,
    snapshot: Option<Snapshot>,
    position: Board,
    messages: Vec<MessageLogEntry>,
    fen_input: String,
    palette: Option<Palette>,
    drag: Option<DragState>,
    selected: Option<Square>,
    last_move: Option<(Square, Square)>,
    flipped: bool,
    promotion: char,
    clock_left: String,
    clock_right: String,
    clock_running: bool,
    clock_turn: Option<String>,
    clock_connected: bool,
    latency_ms: String,
    drop_rate: String,
    error: Option<String>,
}

impl Tab {
    fn new(id: u64, board: u32) -> Self {
        Self {
            id,
            board,
            snapshot: None,
            position: model::start_board(),
            messages: Vec::new(),
            fen_input: model::START_FEN.to_string(),
            palette: None,
            drag: None,
            selected: None,
            last_move: None,
            flipped: false,
            promotion: 'q',
            clock_left: "5:00".into(),
            clock_right: "5:00".into(),
            clock_running: false,
            clock_turn: None,
            clock_connected: true,
            latency_ms: "0".into(),
            drop_rate: "0".into(),
            error: None,
        }
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot) {
        if let Some(position) = model::parse_fen(&snapshot.fen) {
            self.position = position;
        }
        self.snapshot = Some(snapshot);
    }
}

/// The application state.
pub struct App {
    daemon: Daemon,
    tabs: Vec<Tab>,
    active: usize,
    next_tab_id: u64,
    transport: TransportChoice,
    info: Option<Info>,
    status: String,
    last_poll: Instant,
    piece_font: PieceFont,
    show_options: bool,
    cleaned_up: bool,
}

impl App {
    /// Build the app, install the chess font and ask the daemon for a first
    /// board.
    pub fn new(cc: &eframe::CreationContext<'_>, socket: PathBuf) -> Self {
        let piece_font = crate::fonts::install(&cc.egui_ctx);
        let daemon = Daemon::connect(&cc.egui_ctx, socket);
        let mut app = Self {
            daemon,
            tabs: Vec::new(),
            active: 0,
            next_tab_id: 1,
            transport: TransportChoice::Daemon,
            info: None,
            status: "Conectando con el daemon…".into(),
            last_poll: Instant::now() - POLL_INTERVAL,
            piece_font,
            show_options: false,
            cleaned_up: false,
        };
        app.daemon.send(Request::Info, Tag::Info);
        app.create_tab();
        app
    }

    /// Ask the daemon for a new board; the tab is created when it answers.
    fn create_tab(&mut self) {
        self.status = "Creando tablero…".into();
        self.daemon.send(
            Request::Create {
                transport: self.transport.wire(),
            },
            Tag::Create,
        );
    }

    /// Stop the board behind a tab and drop the tab.
    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(index);
        self.daemon
            .send(Request::Stop { board: tab.board }, Tag::Plain(tab.id));
        self.status = format!("tablero {} detenido", tab.board);
        if self.tabs.is_empty() {
            self.active = 0;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
    }

    fn refresh_active(&self) {
        if let Some(tab) = self.tabs.get(self.active) {
            self.daemon
                .send(Request::Show { board: tab.board }, Tag::Snapshot(tab.id));
            self.daemon.send(
                Request::Messages { board: tab.board },
                Tag::Messages(tab.id),
            );
        }
    }

    fn tab_mut(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    /// Apply every answer that arrived since the last frame.
    fn poll_events(&mut self) {
        for event in self.daemon.poll() {
            match event.tag {
                Tag::Info => match event.result.and_then(|r| r.into_data::<Info>()) {
                    Ok(info) => {
                        self.status = format!(
                            "daemon activo · pid {} · v{} · {} tableros",
                            info.pid, info.version, info.boards
                        );
                        self.info = Some(info);
                    }
                    Err(err) => self.status = err,
                },
                Tag::Create => match event.result.and_then(|r| r.into_data::<BoardSummary>()) {
                    Ok(summary) => {
                        let id = self.next_tab_id;
                        self.next_tab_id += 1;
                        self.tabs.push(Tab::new(id, summary.id));
                        self.active = self.tabs.len() - 1;
                        self.status = format!(
                            "tablero {} creado · {} · {}",
                            summary.id, summary.transport, summary.endpoint
                        );
                        self.refresh_active();
                    }
                    Err(err) => self.status = err,
                },
                Tag::Snapshot(tab_id) => {
                    if let Ok(snapshot) = event.result.and_then(|r| r.into_data::<Snapshot>())
                        && let Some(tab) = self.tab_mut(tab_id)
                    {
                        tab.apply_snapshot(snapshot);
                    }
                }
                Tag::Messages(tab_id) => {
                    if let Ok(entries) = event
                        .result
                        .and_then(|r| r.into_data::<Vec<MessageLogEntry>>())
                        && let Some(tab) = self.tab_mut(tab_id)
                    {
                        tab.messages = entries;
                    }
                }
                Tag::Mutate(tab_id) => match event.result.and_then(|r| r.into_data::<Snapshot>()) {
                    Ok(snapshot) => {
                        if let Some(tab) = self.tab_mut(tab_id) {
                            tab.apply_snapshot(snapshot);
                            tab.error = None;
                        }
                    }
                    Err(err) => {
                        if let Some(tab) = self.tab_mut(tab_id) {
                            tab.error = Some(err);
                        }
                    }
                },
                Tag::Plain(_) => {}
            }
        }
    }

    /// Send the requests a board action produced.
    fn play(&mut self, index: usize, action: BoardAction) {
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        let board = tab.board;
        let tab_id = tab.id;
        let request = match action {
            BoardAction::Move { from, to } => {
                let mut uci = format!("{}{}", from.name(), to.name());
                if let Some(piece) = tab.position[from.field()]
                    && piece.kind == Kind::Pawn
                    && (to.rank == 0 || to.rank == 7)
                {
                    uci.push(tab.promotion);
                }
                tab.last_move = Some((from, to));
                Request::Move { board, uci }
            }
            BoardAction::Place { square, piece } => Request::Place {
                board,
                square: square.name(),
                piece: piece.fen_char().to_string(),
            },
            BoardAction::Clear { square } => Request::Clear {
                board,
                square: square.name(),
            },
        };
        self.daemon.send(request, Tag::Mutate(tab_id));
    }

    // ---- UI ---------------------------------------------------------------

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("dgt-sim GUI");
                ui.separator();
                ui.add(
                    egui::Label::new(
                        RichText::new(self.daemon.socket().display().to_string()).monospace(),
                    )
                    .truncate(),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("⟳ Refrescar").clicked() {
                        self.daemon.send(Request::Info, Tag::Info);
                        self.refresh_active();
                    }
                    if ui
                        .selectable_label(self.show_options, "Opciones del tablero")
                        .on_hover_text("Mostrar u ocultar el panel de opciones")
                        .clicked()
                    {
                        self.show_options = !self.show_options;
                    }
                });
            });
            let color = if self.info.is_some() {
                Color32::from_rgb(0x7A, 0xD1, 0x6A)
            } else {
                Color32::from_rgb(0xE0, 0x6A, 0x6A)
            };
            ui.add(egui::Label::new(RichText::new(&self.status).color(color)).truncate());
            ui.horizontal(|ui| {
                ui.label("Transporte del nuevo tablero:");
                egui::ComboBox::from_id_salt("transport")
                    .selected_text(self.transport.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.transport,
                            TransportChoice::Daemon,
                            TransportChoice::Daemon.label(),
                        );
                        ui.selectable_value(
                            &mut self.transport,
                            TransportChoice::Pty,
                            TransportChoice::Pty.label(),
                        );
                        ui.selectable_value(
                            &mut self.transport,
                            TransportChoice::Tcp,
                            TransportChoice::Tcp.label(),
                        );
                    });
                if ui.button("＋ Nuevo tablero").clicked() {
                    self.create_tab();
                }
            });
            ui.add_space(4.0);
        });
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            return;
        }
        let mut select = None;
        let mut close = None;
        egui::Panel::top("tabs").show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, tab) in self.tabs.iter().enumerate() {
                            let selected = index == self.active;
                            let connected =
                                tab.snapshot.as_ref().is_some_and(|snapshot| snapshot.connected);
                            let label = if connected {
                                format!("● Tablero {}", tab.board)
                            } else {
                                format!("Tablero {}", tab.board)
                            };
                            if ui.selectable_label(selected, label).clicked() {
                                select = Some(index);
                            }
                            if ui
                                .small_button("✕")
                                .on_hover_text("Cerrar pestaña y detener el tablero")
                                .clicked()
                            {
                                close = Some(index);
                            }
                            ui.separator();
                        }
                    });
                });
        });
        if let Some(index) = select {
            self.active = index;
            self.refresh_active();
        }
        if let Some(index) = close {
            self.close_tab(index);
        }
    }

    fn side_panel(&mut self, ui: &mut egui::Ui) {
        // `show_collapsible` needs `&mut bool`, and the closure borrows `self`,
        // so the flag is moved out and back.
        let mut show = self.show_options;
        egui::Panel::right("side")
            .resizable(true)
            .default_size(370.0)
            .max_size(460.0)
            .show_collapsible(ui, &mut show, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.side_contents(ui));
            });
        self.show_options = show;
    }

    fn side_contents(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            ui.label("No hay tableros abiertos.");
            return;
        }
        let active = self.active.min(self.tabs.len() - 1);
        self.active = active;
        let board = self.tabs[active].board;
        let tab_id = self.tabs[active].id;

        let mut requests: Vec<Request> = Vec::new();
        let mut close = false;

        {
            let tab = &mut self.tabs[active];
            let piece_font = &self.piece_font;

            // ---- estado ---------------------------------------------------
            egui::CollapsingHeader::new("Estado")
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("status_grid")
                        .num_columns(2)
                        .spacing([12.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Tablero");
                            ui.label(board.to_string());
                            ui.end_row();
                            if let Some(snapshot) = &tab.snapshot {
                                ui.label("Transporte");
                                ui.label(&snapshot.transport);
                                ui.end_row();
                                ui.label("Cliente");
                                ui.label(if snapshot.connected {
                                    snapshot.peer.clone().unwrap_or_else(|| "conectado".into())
                                } else {
                                    "—".into()
                                });
                                ui.end_row();
                                ui.label("Modo");
                                ui.label(&snapshot.mode);
                                ui.end_row();
                                ui.label("Mensajes");
                                ui.label(snapshot.messages_sent.to_string());
                                ui.end_row();
                                ui.label("Campos");
                                ui.label(snapshot.fields_changed.to_string());
                                ui.end_row();
                                ui.label("EEPROM");
                                ui.label(format!("{} B", snapshot.eeprom_len));
                                ui.end_row();
                            }
                        });
                    // The endpoint and FEN are long: give each its own line so
                    // they truncate to the panel width instead of widening it.
                    if let Some(snapshot) = &tab.snapshot {
                        ui.add_space(4.0);
                        ui.label("Endpoint");
                        ui.add(
                            egui::Label::new(RichText::new(&snapshot.endpoint).monospace())
                                .truncate(),
                        );
                        ui.label("FEN");
                        ui.add(
                            egui::Label::new(RichText::new(&snapshot.fen).monospace()).truncate(),
                        );
                    }
                });

            if let Some(err) = &tab.error {
                ui.colored_label(Color32::from_rgb(0xE0, 0x6A, 0x6A), err);
            }

            // ---- posición -------------------------------------------------
            egui::CollapsingHeader::new("Posición")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Posición inicial").clicked() {
                            requests.push(Request::Reset { board, emit: true });
                            tab.last_move = None;
                            tab.selected = None;
                        }
                        if ui.button("Vaciar tablero").clicked() {
                            requests.push(Request::SetFen {
                                board,
                                fen: format!("{} w - - 0 1", model::EMPTY_FEN),
                                quiet: false,
                            });
                            tab.last_move = None;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("FEN:");
                        if ui.button("usar actual").clicked() {
                            let fen = tab.snapshot.as_ref().map(|snapshot| snapshot.fen.clone());
                            if let Some(fen) = fen {
                                tab.fen_input = fen;
                            }
                        }
                    });
                    ui.add(
                        egui::TextEdit::multiline(&mut tab.fen_input)
                            .desired_rows(2)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                    if ui.button("Aplicar FEN").clicked() {
                        requests.push(Request::SetFen {
                            board,
                            fen: tab.fen_input.clone(),
                            quiet: false,
                        });
                        tab.last_move = None;
                    }
                });

            // ---- piezas ---------------------------------------------------
            egui::CollapsingHeader::new("Colocar piezas")
                .default_open(true)
                .show(ui, |ui| {
                    ui.label(
                        "Elige una pieza y pincha casillas. Sin pieza activa, arrastra para mover.",
                    );
                    let mut palette = tab.palette;
                    palette_ui(ui, &mut palette, piece_font);
                    tab.palette = palette;
                    if ui.button("Quitar selección").clicked() {
                        tab.palette = None;
                    }
                });

            // ---- modo -----------------------------------------------------
            egui::CollapsingHeader::new("Modo de transferencia")
                .default_open(false)
                .show(ui, |ui| {
                    ui.label("El tablero solo emite actualizaciones en modo update.");
                    let current = tab
                        .snapshot
                        .as_ref()
                        .map(|s| s.mode.clone())
                        .unwrap_or_else(|| "idle".into());
                    ui.label(RichText::new(format!("actual: {current}")).monospace());
                    ui.horizontal_wrapped(|ui| {
                        for mode in ["idle", "update-board", "update", "update-nice"] {
                            if ui.button(mode).clicked() {
                                requests.push(Request::SetMode {
                                    board,
                                    mode: mode.to_string(),
                                });
                            }
                        }
                    });
                });

            // ---- reloj ----------------------------------------------------
            egui::CollapsingHeader::new("Reloj")
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(snapshot) = &tab.snapshot {
                        let clock = &snapshot.clock;
                        ui.label(
                            RichText::new(format!(
                                "izq {} · der {} · {} · juega {}",
                                clock.left,
                                clock.right,
                                if clock.running { "corriendo" } else { "parado" },
                                if clock.to_move.is_empty() {
                                    "—"
                                } else {
                                    &clock.to_move
                                }
                            ))
                            .monospace(),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("Izq:");
                        ui.add(egui::TextEdit::singleline(&mut tab.clock_left).desired_width(64.0));
                        ui.label("Der:");
                        ui.add(
                            egui::TextEdit::singleline(&mut tab.clock_right).desired_width(64.0),
                        );
                    });
                    ui.checkbox(&mut tab.clock_running, "corriendo");
                    ui.checkbox(&mut tab.clock_connected, "reloj conectado");
                    ui.horizontal(|ui| {
                        ui.label("Juega:");
                        ui.selectable_value(&mut tab.clock_turn, None, "—");
                        ui.selectable_value(&mut tab.clock_turn, Some("left".into()), "izq");
                        ui.selectable_value(&mut tab.clock_turn, Some("right".into()), "der");
                    });
                    if ui.button("Aplicar reloj").clicked() {
                        requests.push(Request::Clock {
                            board,
                            clock: crate::protocol::ClockRequest {
                                left_ms: parse_time_ms(&tab.clock_left),
                                right_ms: parse_time_ms(&tab.clock_right),
                                running: Some(tab.clock_running),
                                turn: tab.clock_turn.clone(),
                                connected: Some(tab.clock_connected),
                            },
                        });
                    }
                    ui.label("Formato: mm:ss, h:mm:ss, 5m, 90s o segundos.");
                });

            // ---- conexión y fallos ---------------------------------------
            egui::CollapsingHeader::new("Conexión y fallos")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Desconectar").clicked() {
                            requests.push(Request::Disconnect { board });
                        }
                        if ui.button("Reconectar").clicked() {
                            requests.push(Request::Reconnect { board });
                        }
                    });
                    ui.separator();
                    if let Some(snapshot) = &tab.snapshot {
                        if snapshot.faults.is_clean() {
                            ui.label("Sin fallos inyectados.");
                        } else {
                            ui.label(
                                RichText::new(format!(
                                    "latencia {} ms · drop {:.2} · fragmento {} · truncar {}",
                                    snapshot.faults.latency_ms,
                                    snapshot.faults.drop_rate,
                                    snapshot.faults.fragment_max,
                                    snapshot.faults.truncate
                                ))
                                .monospace(),
                            );
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.label("Latencia ms:");
                        ui.add(egui::TextEdit::singleline(&mut tab.latency_ms).desired_width(56.0));
                        ui.label("Drop:");
                        ui.add(egui::TextEdit::singleline(&mut tab.drop_rate).desired_width(56.0));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Aplicar fallos").clicked() {
                            requests.push(Request::Fault {
                                board,
                                faults: crate::protocol::Faults {
                                    latency_ms: tab.latency_ms.trim().parse().unwrap_or(0),
                                    drop_rate: tab.drop_rate.trim().parse().unwrap_or(0.0),
                                    ..Default::default()
                                },
                            });
                        }
                        if ui.button("Limpiar fallos").clicked() {
                            requests.push(Request::Fault {
                                board,
                                faults: crate::protocol::Faults::default(),
                            });
                        }
                    });
                    ui.separator();
                    if ui.button("Detener tablero").clicked() {
                        close = true;
                    }
                });

            // ---- promoción y orientación ---------------------------------
            egui::CollapsingHeader::new("Opciones de vista")
                .default_open(false)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Promoción:");
                        egui::ComboBox::from_id_salt("promotion")
                            .selected_text(promotion_label(tab.promotion))
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    ('q', "dama"),
                                    ('r', "torre"),
                                    ('b', "alfil"),
                                    ('n', "caballo"),
                                ] {
                                    ui.selectable_value(&mut tab.promotion, value, label);
                                }
                            });
                    });
                    ui.checkbox(&mut tab.flipped, "Girar tablero (negras abajo)");
                });

            // ---- mensajes -------------------------------------------------
            egui::CollapsingHeader::new("Mensajes del protocolo")
                .default_open(false)
                .show(ui, |ui| {
                    if tab.messages.is_empty() {
                        ui.label("Todavía no se ha emitido nada.");
                    } else {
                        for entry in tab.messages.iter().rev().take(40) {
                            ui.label(
                                RichText::new(format!(
                                    "{:>7}ms {:<14} {:>4}B {}",
                                    entry.at_ms, entry.name, entry.len, entry.hex
                                ))
                                .monospace(),
                            );
                        }
                    }
                });
        }

        for request in requests {
            self.daemon.send(request, Tag::Mutate(tab_id));
        }
        if close {
            let index = self.tabs.iter().position(|tab| tab.id == tab_id);
            if let Some(index) = index {
                self.close_tab(index);
            }
        }
    }

    fn board_view(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No hay tableros. Pulsa «＋ Nuevo tablero».");
            });
            return;
        }
        let active = self.active.min(self.tabs.len() - 1);
        self.active = active;

        let piece_font = self.piece_font.clone();
        let tab = &mut self.tabs[active];

        // The board is the whole point of the window, so it gets all the space
        // that is left and is centred in it. Everything else lives in the side
        // panel, which starts hidden.
        let action = ui
            .with_layout(
                egui::Layout::centered_and_justified(egui::Direction::TopDown),
                |ui| {
                    board_widget::show(
                        ui,
                        &tab.position,
                        tab.flipped,
                        tab.palette,
                        &mut tab.drag,
                        &mut tab.selected,
                        tab.last_move,
                        &piece_font,
                    )
                },
            )
            .inner;
        if let Some(action) = action {
            self.play(active, action);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Stop the boards this instance created when the window is closed, so
        // they do not accumulate in the daemon across runs.
        if !self.cleaned_up && ui.ctx().input(|input| input.viewport().close_requested()) {
            self.cleaned_up = true;
            let boards: Vec<u32> = self.tabs.iter().map(|tab| tab.board).collect();
            self.daemon.stop_boards_blocking(&boards);
        }

        self.poll_events();
        if self.last_poll.elapsed() >= POLL_INTERVAL {
            self.last_poll = Instant::now();
            self.refresh_active();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(120));

        self.top_bar(ui);
        self.tab_bar(ui);
        self.side_panel(ui);
        egui::CentralPanel::default().show(ui, |ui| self.board_view(ui));
    }
}

/// The palette grid: white pieces, black pieces and an eraser.
fn palette_ui(ui: &mut egui::Ui, palette: &mut Option<Palette>, piece_font: &PieceFont) {
    let order = [
        Kind::King,
        Kind::Queen,
        Kind::Rook,
        Kind::Bishop,
        Kind::Knight,
        Kind::Pawn,
    ];
    egui::Grid::new("palette")
        .num_columns(6)
        .spacing([4.0, 4.0])
        .show(ui, |ui| {
            for color in [Color::White, Color::Black] {
                for kind in order {
                    let piece = Piece::new(color, kind);
                    let selected = *palette == Some(Palette::Piece(piece));
                    if piece_button(ui, piece, selected, piece_font).clicked() {
                        *palette = if selected {
                            None
                        } else {
                            Some(Palette::Piece(piece))
                        };
                    }
                }
                ui.end_row();
            }
        });
    let eraser_selected = *palette == Some(Palette::Eraser);
    if ui
        .add_sized(
            [120.0, 28.0],
            egui::Button::new("Borrar casilla").selected(eraser_selected),
        )
        .clicked()
    {
        *palette = if eraser_selected {
            None
        } else {
            Some(Palette::Eraser)
        };
    }
}

/// A palette button that paints the piece with the same ink-centred glyph as
/// the board, so both line up identically.
fn piece_button(
    ui: &mut egui::Ui,
    piece: Piece,
    selected: bool,
    piece_font: &PieceFont,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(40.0, 34.0), egui::Sense::click());
    if selected || response.hovered() {
        let fill = if selected {
            ui.visuals().selection.bg_fill
        } else {
            ui.visuals().widgets.hovered.weak_bg_fill
        };
        ui.painter().rect_filled(rect, 4.0, fill);
    }
    let font = FontId::new(24.0, piece_font.family.clone());
    board_widget::paint_piece(
        ui.painter(),
        rect.center(),
        piece,
        &font,
        piece_font.glyphs,
        1.2,
    );
    response
}

fn promotion_label(value: char) -> &'static str {
    match value {
        'r' => "torre",
        'b' => "alfil",
        'n' => "caballo",
        _ => "dama",
    }
}

/// Parse `mm:ss`, `h:mm:ss`, `5m`, `90s` or a bare number of seconds.
fn parse_time_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if text.contains(':') {
        let parts: Vec<&str> = text.split(':').collect();
        let seconds = match parts.as_slice() {
            [m, s] => m.trim().parse::<u64>().ok()? * 60 + s.trim().parse::<u64>().ok()?,
            [h, m, s] => {
                h.trim().parse::<u64>().ok()? * 3600
                    + m.trim().parse::<u64>().ok()? * 60
                    + s.trim().parse::<u64>().ok()?
            }
            _ => return None,
        };
        return Some(seconds * 1000);
    }
    if let Some(value) = text.strip_suffix('m') {
        return value.trim().parse::<u64>().ok().map(|m| m * 60_000);
    }
    if let Some(value) = text.strip_suffix('s') {
        return value.trim().parse::<u64>().ok().map(|s| s * 1000);
    }
    text.parse::<u64>().ok().map(|seconds| seconds * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_parsing() {
        assert_eq!(parse_time_ms("5:00"), Some(300_000));
        assert_eq!(parse_time_ms("1:30:00"), Some(5_400_000));
        assert_eq!(parse_time_ms("5m"), Some(300_000));
        assert_eq!(parse_time_ms("90s"), Some(90_000));
        assert_eq!(parse_time_ms("42"), Some(42_000));
        assert_eq!(parse_time_ms(""), None);
        assert_eq!(parse_time_ms("abc"), None);
    }
}
