//! The control channel over a WebSocket (`?control=ws://localhost:PORT`), so an agent can drive
//! the browser build the way `cadcraft --control PORT` lets it drive the desktop app.
//!
//! A browser can't listen on a port, so the page connects *out* to a small relay on the user's
//! machine; the relay exposes the desktop's JSON-lines TCP protocol to `cadcraft-cli mcp --connect`
//! and forwards every line here as a text frame. Requests reach the app through the shared
//! [`Link`] (`control_link.rs`); replies go back on the socket with the request's `id`, like
//! `apps/cadcraft/src/control_server.rs`.
//!
//! Only loopback targets are accepted (`ws://localhost`, `ws://127.0.0.1`, `ws://[::1]`): the
//! channel exists to let a program on the user's own machine draw in their tab, nothing else.

use std::rc::Rc;
use std::sync::mpsc::Sender;

use cadcraft_ui_egui::ControlRequest;
use serde_json::Value;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::closure::Closure;

use crate::control_link::Link;
pub use crate::control_target::target;

/// Connect and return the link that feeds the app's request queue `tx`.
pub fn open(url: &str, tx: Sender<ControlRequest>, ctx: egui::Context) -> Result<Link, String> {
    let socket = web_sys::WebSocket::new(url).map_err(|e| format!("control: can't open {url}: {e:?}"))?;
    let send: Rc<dyn Fn(&Value)> = {
        let socket = socket.clone();
        Rc::new(move |reply: &Value| {
            if socket.ready_state() == web_sys::WebSocket::OPEN
                && let Err(e) = socket.send_with_str(&reply.to_string())
            {
                log::error!("control: send failed: {e:?}");
            }
        })
    };
    let link = Link::new(tx, ctx, send);

    let on_message = {
        let link = link.clone();
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if let Some(text) = event.data().as_string() {
                link.push_text(&text);
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

    Ok(link)
}
