//! The browser end of the control channel, independent of how lines arrive.
//!
//! A control request is one JSON line (`{"id", "method", "params"}`, see
//! `crates/ui-egui/src/control.rs`); a reply is one JSON line carrying the same `id`. Two
//! transports feed this: a WebSocket to a loopback relay (`control_ws.rs`, for
//! `cadcraft-cli mcp --connect`) and the `?host=parent` postMessage bridge (`host.rs`, for the
//! page that embeds CADCraft). Both push lines in and hand a `send` closure for the replies;
//! both share the one request queue the app drains every frame ([`CadApp::with_control`]).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use cadcraft_ui_egui::{ControlRequest, PendingReply};
use serde_json::{Value, json};

/// A request the app is working on, waiting to be answered on its transport.
struct Pending {
    id: Value,
    reply: PendingReply,
}

#[derive(Clone)]
pub struct Link {
    tx: Sender<ControlRequest>,
    pending: Rc<RefCell<Vec<Pending>>>,
    /// Lines that arrived between frames.
    inbox: Rc<RefCell<Vec<String>>>,
    ctx: egui::Context,
    send: Rc<dyn Fn(&Value)>,
}

impl Link {
    /// `tx` is the app's request queue (clone it for every transport); `send` delivers a reply.
    pub fn new(tx: Sender<ControlRequest>, ctx: egui::Context, send: Rc<dyn Fn(&Value)>) -> Self {
        Self { tx, pending: Rc::new(RefCell::new(Vec::new())), inbox: Rc::new(RefCell::new(Vec::new())), ctx, send }
    }

    /// A line (or several, newline-separated) from the transport; queued on the next frame.
    pub fn push_text(&self, text: &str) {
        self.inbox.borrow_mut().extend(text.lines().map(str::to_string));
        self.ctx.request_repaint();
    }

    /// Before the app's frame logic: queue every line that arrived since the last frame.
    pub fn queue(&self) {
        let lines: Vec<String> = std::mem::take(&mut *self.inbox.borrow_mut());
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(&line) {
                Ok(msg) => {
                    let id = msg.get("id").cloned().unwrap_or(Value::Null);
                    let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_string();
                    let params = msg.get("params").cloned().unwrap_or(json!({}));
                    let (req, reply) = ControlRequest::new(method, params);
                    if self.tx.send(req).is_err() {
                        (self.send)(&json!({"id": id, "ok": false, "error": "the app is gone"}));
                        continue;
                    }
                    self.pending.borrow_mut().push(Pending { id, reply });
                }
                Err(e) => (self.send)(&json!({"ok": false, "error": format!("bad JSON: {e}")})),
            }
        }
        if !self.pending.borrow().is_empty() {
            self.ctx.request_repaint();
        }
    }

    /// After each of the app's passes: answer every request it has finished. While a reply is
    /// still owed, keep the frames coming — egui idles between frames, and a reply that lands
    /// after the last pass would otherwise wait for the next pointer movement.
    pub fn flush(&self) {
        let mut pending = self.pending.borrow_mut();
        let mut i = 0;
        while i < pending.len() {
            match pending[i].reply.try_take() {
                Some(mut r) => {
                    let done = pending.remove(i);
                    if let Some(o) = r.as_object_mut() {
                        o.insert("id".into(), done.id);
                    }
                    (self.send)(&r);
                }
                None => i += 1,
            }
        }
        if !pending.is_empty() {
            self.ctx.request_repaint();
        }
    }
}
