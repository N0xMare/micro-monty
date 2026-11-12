//! Snapshot and restore of the whole execution state.
//!
//! Everything the loop needs to continue is plain data: `Vm::st`, the live
//! value stack (frame headers included) and the arena. Code entries and IC
//! tables are not written: the code table is rebuilt from the program (the
//! engine writes it beside the arena), ICs start empty.

use crate::heap::SEG_WORDS;
use crate::value::Value;
use crate::vm::{State, Vm};

const MAGIC: u64 = 0x4D4D_5350_4B32_0001; // "MMSPK2" v1

pub fn save(vm: &Vm) -> Vec<u64> {
    let s = vm.st;
    let top = vm.live_top();
    let h = &vm.heap;
    let mut w = vec![
        MAGIC,
        s.pc as u64,
        s.hb as u64,
        s.ci as u64,
        s.depth as u64,
        s.acc.0,
        s.ticks as u64,
        vm.period as u64,
        vm.ticks_used,
        vm.fuel.unwrap_or(u64::MAX),
        vm.limit as u64,
        h.gver,
        h.globals.0,
        h.allocated,
        h.threshold,
        h.arena.top,
        h.arena.limit,
        h.arena.segs.len() as u64,
        top as u64,
    ];
    w.extend(vm.stack[..top].iter().map(|v| v.0));
    for (i, seg) in h.arena.segs.iter().enumerate() {
        let base = i as u64 * SEG_WORDS;
        let n = h.arena.top.saturating_sub(base).min(seg.len() as u64) as usize;
        w.push(n as u64);
        w.extend_from_slice(&seg[..n]);
    }
    w
}

/// Restores `w` into `vm`, a VM freshly built from the same program.
pub fn restore(vm: &mut Vm, w: &[u64]) -> Result<(), &'static str> {
    let mut it = w.iter().copied();
    let mut next = || it.next().ok_or("truncated snapshot");
    if next()? != MAGIC {
        return Err("bad magic");
    }
    vm.st = State {
        pc: next()? as u32,
        hb: next()? as u32,
        ci: next()? as u32,
        depth: next()? as u32,
        acc: Value(next()?),
        ticks: next()? as i64,
    };
    vm.period = next()? as i64;
    vm.ticks_used = next()?;
    vm.fuel = match next()? {
        u64::MAX => None,
        f => Some(f),
    };
    vm.limit = next()? as u32;
    vm.heap.gver = next()?;
    vm.heap.globals = Value(next()?);
    vm.heap.allocated = next()?;
    vm.heap.threshold = next()?;
    vm.heap.arena.top = next()?;
    vm.heap.arena.limit = next()?;
    let nsegs = next()? as usize;
    let top = next()? as usize;
    if vm.stack.len() < top + 256 {
        vm.stack
            .resize((top + 256).next_power_of_two(), Value::UNDEF);
    }
    for i in 0..top {
        vm.stack[i] = Value(next()?);
    }
    vm.heap.arena.segs.clear();
    for _ in 0..nsegs {
        let n = next()? as usize;
        let mut seg = vec![0u64; (n as u64).max(SEG_WORDS) as usize].into_boxed_slice();
        for x in seg.iter_mut().take(n) {
            *x = next()?;
        }
        vm.heap.arena.segs.push(seg);
    }
    for ic in vm.ics.iter_mut() {
        ic.version = 0;
    }
    Ok(())
}
