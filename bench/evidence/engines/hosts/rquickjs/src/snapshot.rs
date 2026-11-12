// dquickts heap snapshots (included as `mod snapshot` with the `snapshots`
// feature). The bridge and `hsync` are registered natives; the prelude's
// `{settle, run, stalled}` object is the snapshot's one root.

use super::*;
use rquickjs::snapshot::{SnapshotHost, SnapshotKind};
use std::sync::OnceLock;

pub fn host() -> SnapshotHost {
    static HOST: OnceLock<SnapshotHost> = OnceLock::new();
    HOST.get_or_init(|| {
        SnapshotHost::new()
            .function("bridge", |ctx| make_bridge(ctx.clone()))
            .function("hsync", |ctx| make_hsync(ctx.clone()))
    })
    .clone()
}

/// A full heap snapshot of `s` and its open calls (kept host-side).
pub fn write(s: &S) -> Option<(Vec<u8>, Vec<(u64, String, Json)>)> {
    let bytes = s.ctx.with(|ctx| -> Result<Vec<u8>, String> {
        let api = s
            .api
            .clone()
            .ok_or("no prelude")?
            .restore(&ctx)
            .map_err(|e| e.to_string())?;
        ctx.write_snapshot(&host(), SnapshotKind::Full, &[api.into_value()])
            .map_err(|e| exception(&ctx, e))
    });
    let bytes = bytes.map_err(|e| eprintln!("snapshot: {e}")).ok()?;
    let pending = bridge(|b| {
        b.calls
            .iter()
            .map(|(id, n, a)| (*id as u64, n.clone(), serde(a)))
            .collect()
    });
    Some((bytes, pending))
}

/// A session rebuilt from `bytes` in a new runtime, with the host-side
/// settings (limits, interrupt handler) applied again.
pub fn read(bytes: &[u8]) -> Option<S> {
    let rt = Runtime::new().ok()?;
    let restored = rt
        .read_snapshot(&host(), bytes, None)
        .map_err(|e| eprintln!("restore: {e}"))
        .ok()?;
    rt.set_memory_limit(MEMORY_LIMIT);
    rt.set_max_stack_size(MAX_STACK);
    let polls = Arc::new(AtomicU64::new(0));
    let stop = Stop::default();
    let (p, st) = (Arc::clone(&polls), stop.clone());
    rt.set_interrupt_handler(Some(Box::new(move || {
        p.fetch_add(1, Ordering::Relaxed);
        st.requested()
    })));
    let ctx = restored.contexts.first()?.clone();
    let root = restored.roots.into_iter().next()?;
    let api = ctx.with(|ctx| -> Option<Persistent<Object<'static>>> {
        let v = root.restore(&ctx).ok()?;
        Some(Persistent::save(&ctx, v.into_object()?))
    })?;
    Some(S {
        api: Some(api),
        ctx,
        rt,
        polls,
        stop,
    })
}
