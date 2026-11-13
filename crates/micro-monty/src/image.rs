//! The facade's part of the snapshot format: the header around the
//! engine's sections and the checksum after them. Its code is part of the
//! [`build_id`] (the build script hashes this file, comments and blank
//! space aside), so whatever writes or reads the header belongs here: a
//! change to it then refuses older images as
//! [`SnapshotError::Incompatible`] instead of misreading them.

use crate::{
    Engine, ParkReason, Progress, Session, SnapshotError, build_id, host_defs, test_hooks,
};
use mm_engine::{FutureStatus, Stopped, Vm};

/// The first bytes of every snapshot.
const SNAPSHOT_MAGIC: [u8; 8] = *b"MMSNAP\0\x01";

/// The snapshot format version this build writes and restores (bytes 8–9
/// of an image, little-endian; [`SnapshotError::Incompatible`] reports an
/// image's). Part of the [`build_id`].
pub const SNAPSHOT_FORMAT: u16 = 1;

/// The build id a snapshot image carries (its bytes 10..42, after the
/// 8-byte magic and the 2-byte format version; this layout is permanent),
/// or `None` if `image` is not a snapshot. A host that keeps several
/// builds restores the image on the one whose [`build_id`] this is
/// ([Upgrading: drain old builds](crate#upgrading-drain-old-builds)).
#[must_use]
pub fn image_build_id(image: &[u8]) -> Option<[u8; 32]> {
    if image.get(..8)? != SNAPSHOT_MAGIC.as_slice() {
        return None;
    }
    image.get(10..42)?.try_into().ok()
}

/// What identifies a host module's generated source `src`: two independent
/// 64-bit hashes of it. Images store it for each host module whose code the
/// Engine shares, and a restore takes or rebuilds that code by it (with the
/// code hash the image also stores, `HostProgram::code`).
pub(crate) fn host_key(src: &str) -> [u8; 16] {
    let b = src.as_bytes();
    let mut k = [0; 16];
    k[..8].copy_from_slice(&mm_core::hash_bytes(b).to_le_bytes());
    k[8..].copy_from_slice(&mm_core::xxh64(b, b.len() as u64).to_le_bytes());
    k
}

/// A session as an image holds it: the engine's state, and what the
/// header keeps of the facade's.
pub(crate) struct Image {
    pub(crate) vm: Vm,
    pub(crate) returns_last_expr: bool,
    pub(crate) want_value: bool,
    pub(crate) stop: Vec<u8>,
    pub(crate) text_limit: usize,
    pub(crate) fuel_total: Option<u64>,
}

/// Session `s` as an image of at most `max_bytes`, the checksum included
/// ([`Session::snapshot_with_limit`]).
pub(crate) fn write(s: &mut Session, max_bytes: usize) -> Result<Vec<u8>, SnapshotError> {
    let mut header = Vec::new();
    header.extend_from_slice(&SNAPSHOT_MAGIC);
    header.extend_from_slice(&SNAPSHOT_FORMAT.to_le_bytes());
    header.extend_from_slice(build_id());
    header.extend_from_slice(&[u8::from(s.returns_last_expr), u8::from(s.want_value)]);
    if s.stop.is_empty() {
        header.push(0);
    } else {
        header.extend_from_slice(&s.stop);
    }
    header.extend_from_slice(&(s.text_limit as u64).to_le_bytes());
    match s.fuel_total {
        None => header.push(0),
        Some(total) => {
            header.push(1);
            header.extend_from_slice(&total.to_le_bytes());
        }
    }
    // One buffer of the image's exact size, the checksum included.
    let mut out =
        s.vm.write_snapshot_sized(&header, 8, max_bytes)
            .map_err(|_| SnapshotError::Busy)?
            .ok_or(SnapshotError::TooLarge)?;
    test_hooks::point("snapshot");
    let sum = mm_core::xxh64(&out, 0);
    out.extend_from_slice(&sum.to_le_bytes());
    Ok(out)
}

/// The session image `bytes` holds, read for `engine` ([`Engine::restore`]):
/// refused as incompatible unless a build with this build id wrote it.
pub(crate) fn read(engine: &Engine, bytes: &[u8]) -> Result<Image, SnapshotError> {
    let corrupt = |what: &str| SnapshotError::Corrupt {
        what: what.to_owned(),
    };
    let Some((body, trailer)) = bytes.split_last_chunk::<8>() else {
        return Err(corrupt("truncated"));
    };
    let mut r = mm_core::Reader::new(body);
    if r.take(8) != Some(SNAPSHOT_MAGIC.as_slice()) {
        return Err(corrupt("not a micro-monty snapshot"));
    }
    let format = r
        .take(2)
        .and_then(|v| v.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| corrupt("truncated"))?;
    let id = image_build_id(bytes).ok_or_else(|| corrupt("truncated"))?;
    if format != SNAPSHOT_FORMAT {
        return Err(SnapshotError::Incompatible {
            format,
            build_id: id,
        });
    }
    if mm_core::xxh64(body, 0) != u64::from_le_bytes(*trailer) {
        return Err(corrupt("checksum"));
    }
    if r.take(32) != Some(build_id().as_slice()) {
        return Err(SnapshotError::Incompatible {
            format,
            build_id: id,
        });
    }
    let returns_last_expr = r.bool().ok_or_else(|| corrupt("session"))?;
    let want_value = r.bool().ok_or_else(|| corrupt("session"))?;
    // The stop, as the cell reported it (`Session::pending`); its host
    // values are the host's, capped at the memory limit as snapshot
    // output is.
    let mut budget = engine.config.limits.memory;
    let header = *r.rest_mut();
    let pending = read_stop(&mut r, &mut budget)?;
    let stop = match pending {
        Some(_) => header[..header.len() - r.remaining()].to_vec(),
        None => Vec::new(),
    };
    // A limit written on a wider target clamps to this one's.
    let text_limit = r
        .u64()
        .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
        .ok_or_else(|| corrupt("session"))?;
    let fuel_total = match r.u8() {
        Some(0) => None,
        Some(1) => Some(r.u64().ok_or_else(|| corrupt("session"))?),
        _ => return Err(corrupt("session")),
    };
    let cache = engine.module_cache();
    let vm = Vm::read_snapshot(
        &mut r,
        mm_stdlib::MODULES,
        cache.clone(),
        &mut |name, spec, recipe, _| {
            host_defs::rebuild_host_program(cache.as_ref(), name, spec, recipe)
        },
        engine.config.limits.memory,
    )
    .map_err(|e| match e {
        mm_engine::SnapError::Corrupt(what) => corrupt(what),
        mm_engine::SnapError::TooLarge => SnapshotError::TooLarge,
    })?;
    // The header's stop is what the session reports and which answer it
    // takes, so it must be what the VM is stopped at. A waiting cell's
    // futures may have been cancelled since (the host's answer to one is
    // then dropped), but the VM still holds each.
    let consistent = match (&pending, vm.stopped()) {
        (None, Stopped::Idle) | (Some(Progress::Parked { .. }), Stopped::Parked) => true,
        (Some(Progress::HostCall { call, .. }), Stopped::HostCall(id)) => *call == id,
        (Some(Progress::Waiting { calls }), Stopped::Waiting) => {
            !calls.is_empty()
                && calls
                    .iter()
                    .all(|&c| vm.future_status(c) != FutureStatus::Unknown)
        }
        _ => false,
    };
    if !consistent {
        return Err(corrupt("stop"));
    }
    Ok(Image {
        vm,
        returns_last_expr,
        want_value,
        stop,
        text_limit,
        fuel_total,
    })
}

/// Appends stop `p` ([`Session::pending`]) as the snapshot header stores
/// it: the phase byte (1 host call, 2 waiting, 3 parked; 0 is idle), then
/// the payload as the cell reported it.
pub(crate) fn write_stop(out: &mut Vec<u8>, p: &Progress) {
    let len = |out: &mut Vec<u8>, n: usize| {
        out.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
    };
    match p {
        Progress::HostCall {
            call,
            function,
            name,
            args,
            kwargs,
        } => {
            out.push(1);
            out.extend_from_slice(&call.to_le_bytes());
            out.extend_from_slice(&function.to_le_bytes());
            len(out, name.len());
            out.extend_from_slice(name.as_bytes());
            len(out, args.len());
            for a in args {
                mm_engine::put_host_value(out, a);
            }
            len(out, kwargs.len());
            for (k, v) in kwargs {
                len(out, k.len());
                out.extend_from_slice(k.as_bytes());
                mm_engine::put_host_value(out, v);
            }
        }
        Progress::Waiting { calls } => {
            out.push(2);
            len(out, calls.len());
            for c in calls {
                out.extend_from_slice(&c.to_le_bytes());
            }
        }
        Progress::Parked { reason } => out.extend_from_slice(&[
            3,
            match reason {
                ParkReason::Fuel => 0,
                ParkReason::Requested => 1,
                ParkReason::Deadline => 2,
            },
        ]),
        _ => {}
    }
}

/// The inverse of [`write_stop`]: corrupt if the header is malformed or
/// its host values take more than `budget` bytes.
pub(crate) fn read_stop(
    r: &mut mm_core::Reader<'_>,
    budget: &mut u64,
) -> Result<Option<Progress>, SnapshotError> {
    let corrupt = || SnapshotError::Corrupt {
        what: "session".to_owned(),
    };
    match r.u8().ok_or_else(corrupt)? {
        0 => Ok(None),
        phase => read_stopped(phase, r, budget).map(Some).ok_or_else(corrupt),
    }
}

/// A stop of phase `phase` (not idle) as [`write_stop`] wrote it.
fn read_stopped(phase: u8, r: &mut mm_core::Reader<'_>, budget: &mut u64) -> Option<Progress> {
    Some(match phase {
        1 => {
            let call = r.u64()?;
            let function = r.u32()?;
            let name = r.str()?.to_owned();
            let n = r.count(1)?;
            let mut args = Vec::with_capacity(n);
            for _ in 0..n {
                args.push(mm_engine::read_host_value(r, budget)?);
            }
            let n = r.count(5)?;
            let mut kwargs = Vec::with_capacity(n);
            for _ in 0..n {
                let k = r.str()?.to_owned();
                kwargs.push((k, mm_engine::read_host_value(r, budget)?));
            }
            Progress::HostCall {
                call,
                function,
                name,
                args,
                kwargs,
            }
        }
        2 => {
            let n = r.count(8)?;
            Progress::Waiting {
                calls: (0..n).map(|_| r.u64()).collect::<Option<_>>()?,
            }
        }
        3 => Progress::Parked {
            reason: match r.u8()? {
                0 => ParkReason::Fuel,
                1 => ParkReason::Requested,
                2 => ParkReason::Deadline,
                _ => return None,
            },
        },
        _ => return None,
    })
}
