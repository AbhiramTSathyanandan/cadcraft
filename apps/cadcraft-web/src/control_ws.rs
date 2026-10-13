//! The control channel over a WebSocket (`?control=ws://localhost:PORT`), so an agent can drive
//! the browser build the way `cadcraft --control PORT` lets it drive the desktop app.
//!
//! A browser can't listen on a port, so the page connects *out* to a small relay on the user's
//! machine; the relay exposes the desktop's JSON-lines TCP protocol to `cadcraft-cli mcp --connect`
//! and forwards every line here as a text frame. Requests reach the app through the same
//! [`ControlRequest`] queue the desktop uses ([`CadApp::with_control`]); replies go back on the
//! socket with the request's `id`, like `apps/cadcraft/src/control_server.rs`.
//!
//! Only loopback targets are accepted (`ws://localhost`, `ws://127.0.0.1`, `ws://[::1]`): the
//! channel exists to let a program on the user's own machine draw in their tab, nothing else.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Sender, channel};

use cadcraft_ui_egui::{CadApp, ControlRequest, PendingReply};
use serde_json::{Value, json};
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;

pub use crate::control_target::target;

/// A request the app is working on, waiting to be answered on the socket.
struct Pending {
    id: Value,
    reply: PendingReply,
}

/// The browser end of the channel: the socket and the replies still owed on it.
pub struct Link {
    socket: web_sys::WebSocket,
    tx: Sender<ControlRequest>,
    pending: Rc<RefCell<Vec<Pending>>>,
    /// Lines that arrived while the socket was being set up or between frames.
    inbox: Rc<RefCell<Vec<String>>>,
    ctx: egui::Context,
}

impl Link {
    /// Connect and hand the app the request queue. The app drains it every frame, exactly as it
    /// drains the desktop control server's queue.
    pub fn open(url: &str, app: CadApp, ctx: egui::Context) -> Result<(Self, CadApp), String> {
        let socket = web_sys::WebSocket::new(url).map_err(|e| format!("control: can't open {url}: {e:?}"))?;
        let (tx, rx) = channel::<ControlRequest>();
        let app = app.with_control(rx);
        let inbox: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

        let on_message = {
            let inbox = inbox.clone();
            let ctx = ctx.clone();
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
                if let Some(text) = event.data().as_string() {
                    inbox.borrow_mut().extend(text.lines().map(str::to_string));
                    ctx.request_repaint();
                }
            })
        };
        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        let on_open = {
            let url = url.to_string();
            Closure::<dyn FnMut(web_sys::Event)>::new(move |_| log::info!("control: connected to {url}"))
        };
        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();

        let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(|e: web_sys::CloseEvent| {
            log::warn!("control: socket closed (code {}): {}", e.code(), e.reason());
        });
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();

        let on_error = Closure::<dyn FnMut(web_sys::Event)>::new(|_| log::error!("control: socket error (is the relay running?)"));
        socket.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();

        Ok((Self { socket, tx, pending: Rc::new(RefCell::new(Vec::new())), inbox, ctx }, app))
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
                        self.send(&json!({"id": id, "ok": false, "error": "the app is gone"}));
                        continue;
                    }
                    self.pending.borrow_mut().push(Pending { id, reply });
                }
                Err(e) => self.send(&json!({"ok": false, "error": format!("bad JSON: {e}")})),
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
                    self.send(&r);
                }
                None => i += 1,
            }
        }
        if !pending.is_empty() {
            self.ctx.request_repaint();
        }
    }

    fn send(&self, reply: &Value) {
        if self.socket.ready_state() == web_sys::WebSocket::OPEN
            && let Err(e) = self.socket.send_with_str(&reply.to_string())
        {
            log::error!("control: send failed: {e:?}");
        }
    }
}
