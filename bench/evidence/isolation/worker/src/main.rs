//! Child-process mode, in `mm-cell-worker`'s shape: frames (`u32 LE` length,
//! then an `iso_service` message) on stdin and stdout, the session on a
//! thread with its own stack, and a reader thread that turns a `Park` frame
//! into a park request while a cell computes.
//!
//! `ISO_STACK_KIB` sets the session thread's stack (default 16 MiB, as
//! `mm-cell-worker`).

use std::{
    io::{self, Read, Write},
    sync::mpsc,
    thread,
};

use iso_service::{Request, Service, decode, encode};

fn read_frame(input: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match input.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let mut body = vec![0u8; u32::from_le_bytes(len) as usize];
    input.read_exact(&mut body)?;
    Ok(Some(body))
}

fn serve() {
    let mut service = Service::new();
    let park = service.park_slot();
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut stdin = io::stdin().lock();
        while let Ok(Some(body)) = read_frame(&mut stdin) {
            if matches!(decode::<Request>(&body), Ok((Request::Park, _))) {
                if let Ok(slot) = park.lock() {
                    if let Some(handle) = slot.as_ref() {
                        handle.request();
                    }
                }
                continue;
            }
            if tx.send(body).is_err() {
                break;
            }
        }
    });
    let mut stdout = io::stdout().lock();
    for body in rx {
        let (response, raw) = match decode::<Request>(&body) {
            Ok((request, raw)) => service.handle(&request, raw),
            Err(message) => (iso_service::Response::Failed { message }, Vec::new()),
        };
        let message = encode(&response, &raw);
        let len = u32::try_from(message.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes();
        if stdout.write_all(&len).is_err()
            || stdout.write_all(&message).is_err()
            || stdout.flush().is_err()
        {
            break;
        }
    }
}

fn main() {
    let kib: usize = std::env::var("ISO_STACK_KIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16 * 1024);
    let worker = thread::Builder::new()
        .name("iso-worker".into())
        .stack_size(kib * 1024)
        .spawn(serve);
    if let Ok(worker) = worker {
        let _ = worker.join();
    }
}
