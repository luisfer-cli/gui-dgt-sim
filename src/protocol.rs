//! The JSON control-plane protocol spoken with a running `dgt-sim` daemon.
//!
//! One request per line, one response per line, over the Unix socket. These
//! types mirror the daemon's `control` module; only the requests this GUI sends
//! are modelled, and every response field is optional so a newer daemon does
//! not break the client.
//!
//! The mirror is intentionally complete: some variants and fields document the
//! full protocol even though this GUI does not use them yet.

#![allow(dead_code)]

use serde::Deserialize;

/// A request to the daemon. Serialised with an internal `cmd` tag, exactly like
/// the daemon's own `Request`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    /// Daemon status.
    Info,
    /// All boards, one summary each.
    List,
    /// Create one more board, returning its summary.
    Create {
        #[serde(skip_serializing_if = "Option::is_none")]
        transport: Option<String>,
    },
    /// One board's full state.
    Show { board: u32 },
    /// Recent outbound protocol messages, as hex.
    Messages { board: u32 },
    /// Play a move, geometrically (no legality check).
    Move { board: u32, uci: String },
    /// Lift the piece off a square.
    Lift { board: u32, square: String },
    /// Put a piece on a square.
    Place {
        board: u32,
        square: String,
        piece: String,
    },
    /// Empty a square.
    Clear { board: u32, square: String },
    /// Load a FEN placement.
    SetFen {
        board: u32,
        fen: String,
        quiet: bool,
    },
    /// Back to the start position and IDLE mode.
    Reset { board: u32, emit: bool },
    /// Force a transfer mode.
    SetMode { board: u32, mode: String },
    /// Drop the current client.
    Disconnect { board: u32 },
    /// Accept clients again.
    Reconnect { board: u32 },
    /// Terminate one board.
    Stop { board: u32 },
    /// Manipulate the emulated clock.
    Clock { board: u32, clock: ClockRequest },
    /// Replace the fault-injection configuration.
    Fault { board: u32, faults: Faults },
}

/// Payload of a `clock` request; every field is optional.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ClockRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected: Option<bool>,
}

/// Fault-injection configuration, mirroring the daemon's `Faults`.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Faults {
    pub latency_ms: u64,
    pub drop_rate: f64,
    pub fragment_max: usize,
    pub truncate: bool,
    pub seed: u64,
    pub disconnect_rate: f64,
}

/// The daemon's answer.
#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<String>,
}

impl Response {
    /// Turn the response into a typed payload, surfacing the daemon's error
    /// message when the request failed.
    pub fn into_data<T: serde::de::DeserializeOwned>(self) -> Result<T, String> {
        if !self.ok {
            return Err(self.error.unwrap_or_else(|| "error desconocido".into()));
        }
        let data = self.data.unwrap_or(serde_json::Value::Null);
        serde_json::from_value(data).map_err(|err| format!("respuesta inesperada: {err}"))
    }
}

/// Daemon status reported by `info`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Info {
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub socket: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub boards: usize,
    #[serde(default)]
    pub uptime_ms: u64,
}

/// One row of `list`, and the payload of `create`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct BoardSummary {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub fen: String,
    #[serde(default)]
    pub messages_sent: u64,
}

/// Full board state reported by `show`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Snapshot {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub transport: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub peer: Option<String>,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub serial: String,
    #[serde(default)]
    pub long_serial: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub trademark: String,
    #[serde(default)]
    pub fen: String,
    #[serde(default)]
    pub clock: ClockView,
    #[serde(default)]
    pub faults: FaultsView,
    #[serde(default)]
    pub eeprom_len: usize,
    #[serde(default)]
    pub fields_changed: u64,
    #[serde(default)]
    pub messages_sent: u64,
    #[serde(default)]
    pub last_emitted: usize,
}

/// The emulated clock, as reported by `show`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClockView {
    #[serde(default)]
    pub connected: bool,
    #[serde(default)]
    pub running: bool,
    #[serde(default)]
    pub to_move: String,
    #[serde(default)]
    pub battery_low: bool,
    #[serde(default)]
    pub left_ms: u64,
    #[serde(default)]
    pub right_ms: u64,
    #[serde(default)]
    pub left: String,
    #[serde(default)]
    pub right: String,
}

/// Fault configuration, as reported by `show`.
#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct FaultsView {
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub drop_rate: f64,
    #[serde(default)]
    pub fragment_max: usize,
    #[serde(default)]
    pub truncate: bool,
    #[serde(default)]
    pub disconnect_rate: f64,
}

impl FaultsView {
    /// Are any faults active?
    pub fn is_clean(&self) -> bool {
        self.latency_ms == 0
            && self.drop_rate <= 0.0
            && self.fragment_max == 0
            && !self.truncate
            && self.disconnect_rate <= 0.0
    }
}

/// One entry of the recent-message log.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MessageLogEntry {
    #[serde(default)]
    pub at_ms: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub hex: String,
    #[serde(default)]
    pub len: usize,
    #[serde(default)]
    pub delivered: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn encoded(request: &Request) -> serde_json::Value {
        serde_json::to_value(request).unwrap()
    }

    #[test]
    fn requests_use_the_cmd_tag() {
        assert_eq!(encoded(&Request::Info), json!({"cmd": "info"}));
        assert_eq!(
            encoded(&Request::Move {
                board: 3,
                uci: "e2e4".into()
            }),
            json!({"cmd": "move", "board": 3, "uci": "e2e4"})
        );
        assert_eq!(
            encoded(&Request::Place {
                board: 1,
                square: "e4".into(),
                piece: "N".into()
            }),
            json!({"cmd": "place", "board": 1, "square": "e4", "piece": "N"})
        );
        assert_eq!(
            encoded(&Request::SetMode {
                board: 2,
                mode: "update-board".into()
            }),
            json!({"cmd": "set_mode", "board": 2, "mode": "update-board"})
        );
        assert_eq!(
            encoded(&Request::Reset {
                board: 1,
                emit: true
            }),
            json!({"cmd": "reset", "board": 1, "emit": true})
        );
    }

    #[test]
    fn create_omits_the_transport_when_default() {
        assert_eq!(
            encoded(&Request::Create { transport: None }),
            json!({"cmd": "create"})
        );
        assert_eq!(
            encoded(&Request::Create {
                transport: Some("tcp".into())
            }),
            json!({"cmd": "create", "transport": "tcp"})
        );
    }

    #[test]
    fn clock_request_only_sends_what_is_set() {
        let clock = ClockRequest {
            left_ms: Some(300_000),
            running: Some(true),
            ..Default::default()
        };
        assert_eq!(
            encoded(&Request::Clock { board: 1, clock }),
            json!({"cmd": "clock", "board": 1, "clock": {"left_ms": 300000, "running": true}})
        );
    }

    #[test]
    fn snapshot_tolerates_a_minimal_payload() {
        let response: Response =
            serde_json::from_str(r#"{"ok":true,"data":{"id":4,"fen":"8/8/8/8/8/8/8/8"}}"#).unwrap();
        let snapshot: Snapshot = response.into_data().unwrap();
        assert_eq!(snapshot.id, 4);
        assert_eq!(snapshot.fen, "8/8/8/8/8/8/8/8");
        assert!(!snapshot.connected);
        assert!(snapshot.clock.left.is_empty());
    }

    #[test]
    fn errors_are_surfaced() {
        let response: Response =
            serde_json::from_str(r#"{"ok":false,"error":"board 9 not found"}"#).unwrap();
        assert_eq!(
            response.into_data::<Snapshot>().unwrap_err(),
            "board 9 not found"
        );
    }
}
