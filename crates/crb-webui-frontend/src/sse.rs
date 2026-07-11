use futures::channel::mpsc;
use leptos::{SignalSet, WriteSignal};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;

/// Create an EventSource and an unbounded channel pair.
fn new_event_source(
    url: &str,
) -> Result<
    (
        web_sys::EventSource,
        mpsc::UnboundedSender<String>,
        mpsc::UnboundedReceiver<String>,
    ),
    String,
> {
    let (tx, rx) = mpsc::unbounded();
    let es = web_sys::EventSource::new(url)
        .map_err(|e| format!("Failed to construct EventSource: {:?}", e))?;
    Ok((es, tx, rx))
}

/// Connect to an SSE endpoint and return a channel receiver that yields
/// event data strings as they arrive.
///
/// This basic version only sets up the `onmessage` handler — no connection
/// status tracking. For status-aware connections (onopen / onerror), see
/// [`connect_sse_with_status`].
pub async fn connect_sse(url: &str) -> Result<mpsc::UnboundedReceiver<String>, String> {
    let (es, tx, rx) = new_event_source(url)?;
    attach_onmessage(&es, tx);
    Ok(rx)
}

/// Connect to an SSE endpoint with connection-status tracking.
///
/// In addition to the basic `onmessage` handler, this sets up:
/// - `onopen`  → writes `"connected"` to `set_connected`
/// - `onerror` → writes `"disconnected"` / `"connecting"` to `set_connected`
pub async fn connect_sse_with_status(
    url: &str,
    set_connected: WriteSignal<String>,
) -> Result<mpsc::UnboundedReceiver<String>, String> {
    let (es, tx, rx) = new_event_source(url)?;

    // onopen — connection established
    let on_open = set_connected.clone();
    let open_closure = Closure::wrap(Box::new(move || {
        on_open.set("connected".into());
    }) as Box<dyn FnMut()>);
    es.set_onopen(Some(open_closure.as_ref().unchecked_ref()));
    open_closure.forget();

    // onerror — connection lost or error
    let on_err = set_connected;
    let es_for_err = es.clone();
    let err_closure = Closure::wrap(Box::new(move || {
        // EventSource auto-reconnects, but we update the status
        // readyState: 0=CONNECTING, 1=OPEN, 2=CLOSED
        if es_for_err.ready_state() == 2 {
            on_err.set("disconnected".into());
        } else {
            on_err.set("connecting".into());
        }
    }) as Box<dyn FnMut()>);
    es.set_onerror(Some(err_closure.as_ref().unchecked_ref()));
    err_closure.forget();

    // onmessage — new event received
    attach_onmessage(&es, tx);

    Ok(rx)
}

/// Set up the onmessage handler on an EventSource.
fn attach_onmessage(es: &web_sys::EventSource, tx: mpsc::UnboundedSender<String>) {
    let tx_clone = tx.clone();
    let msg_closure = Closure::wrap(Box::new(move |event: MessageEvent| {
        if let Some(text) = event.data().as_string() {
            let _ = tx_clone.unbounded_send(text); // Ignore — receiver may have disconnected
        } else {
            log::warn!("SSE message with non-string data");
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    es.set_onmessage(Some(msg_closure.as_ref().unchecked_ref()));
    msg_closure.forget();
}
