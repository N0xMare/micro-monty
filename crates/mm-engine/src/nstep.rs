//! Native operations in steps.
//!
//! Two kinds share one state object, `NSTATE` (value slots, then raw
//! words), so a park, collection or snapshot between two steps sees only
//! heap values, and the Rust stack never holds a second dispatch loop. A
//! step trusts nothing it reads from the state: a mismatch is a
//! `SystemError`, never a panic.
//!
//! - Native continuations: a
//!   builtin whose loop calls Python code (a sort's `key=`) returns
//!   [`Next::Invoke`] with a [`Cont::Native`] to be resumed with the call's
//!   result, or [`Next::Acc`] with the operation's value.
//! - Chunked natives (bounded native steps, docs/spec.md §5): a native
//!   whose work grows with its data does at most [`STEP_WORK`] fuel units
//!   of it per step, then saves its progress ([`Vm::chunk_rest`]); the
//!   prelude's `_steps` calls the
//!   next step (`_more(state)`) until it gives the value, each call a
//!   safepoint. Raw word 0 of such a state is its operation. To add one:
//!   an [`op`] constant, its [`STEPS`] layout, an arm in
//!   [`Vm::chunk_step`] calling a step function next to the native, and a
//!   row in `crates/micro-monty/tests/steps.rs`.

use crate::builtins::Sorted;
use crate::cont::{Args, Cont, Next};
use crate::ops::OrdErr;
use crate::types::{T, ty};
use crate::vm::{Raised, Vm};
use mm_core::Value;

/// The operations, indexes into [`STEPS`].
pub(crate) mod op {
    /// A `key=` result arrived for a sort.
    pub const SORT_KEYS: u8 = 0;
    /// The prelude sorted by keys that compare in Python.
    pub const SORT_DONE: u8 = 1;
    /// Chunked: `x in s` (`not in`) over a long list or tuple.
    pub const SEQ_IN: u8 = 2;
    /// Chunked: a set or dict operation (`set::phase` programs: set
    /// algebra, `dict(...)`, `update`, copies, views).
    pub const BULK: u8 = 3;
    /// Chunked: a list or tuple built or rebuilt from slices of others
    /// (`+`, `*`, `extend`, slices, slice assignment, `insert`, `del`).
    pub const SEQ_BUILD: u8 = 4;
    /// Chunked: `count`, `index` or `remove` over a long list or tuple.
    pub const SEQ_SCAN: u8 = 5;
    /// Chunked: a str built from pieces, each a byte range of a str,
    /// repeated to its length (`*`, padding, `format`, `%`, f-strings;
    /// `strmeth.rs`).
    pub const STR_FILL: u8 = 6;
    /// Chunked: `sep.join(items)`.
    pub const STR_JOIN: u8 = 7;
    /// Chunked: `str.replace`, counted, then built.
    pub const STR_REPLACE: u8 = 8;
    /// Chunked: `split`, `rsplit`, `splitlines`.
    pub const STR_SPLIT: u8 = 9;
    /// Chunked: case maps, `translate`, `expandtabs`, extended slices,
    /// `repr` and `ascii`: sized, then built.
    pub const STR_MAP: u8 = 10;
    /// Chunked: `count` and the `is*` predicates.
    pub const STR_SCAN: u8 = 11;
    /// Chunked: `int()`, `float()` or `complex()` of a long text, then an
    /// `int()`'s digits (`complex.rs`, `intops.rs`); a big int's
    /// power-of-two digits (`builtins::DIGITS`).
    pub const NUM_TEXT: u8 = 12;
    /// Chunked: `math.fsum`, `prod`, `sumprod`, `dist` and `hypot` over a
    /// long sequence (`math.rs`).
    pub const MATH_FOLD: u8 = 13;
    /// Chunked: bytes, bytearray, `binascii` and `hashlib` work over long
    /// data (`crate::bytes::Job`; its kind is raw word 1).
    pub const BYTES: u8 = 14;
    /// Chunked: `json.loads` of a long document (`json.rs`).
    #[cfg_attr(not(feature = "json"), allow(dead_code, reason = "used by json.rs"))]
    pub const JSON_SCAN: u8 = 15;
    /// Chunked: `json.dumps` of a long value (`json.rs`).
    #[cfg_attr(not(feature = "json"), allow(dead_code, reason = "used by json.rs"))]
    pub const JSON_DUMP: u8 = 16;
    /// Chunked: `unicodedata.normalize` or `is_normalized` of a long str
    /// (`unicodedata.rs`).
    #[cfg_attr(
        not(feature = "unicodedata"),
        allow(dead_code, reason = "used by unicodedata.rs")
    )]
    pub const UNI_NORM: u8 = 17;
    /// Chunked: a long `str` or `bytes` `%` format, parsed a window per
    /// step (`percent.rs`).
    pub const STR_PCT: u8 = 18;
    /// Chunked: `any`, `all`, `min`, `max` or `sum` over an iterator
    /// (`builtins::fold`).
    pub const FOLD: u8 = 19;
    /// Chunked: `x in d.values()` (`not in`) over a long dict (`ops.rs`).
    pub const DV_IN: u8 = 20;
    /// A finished chunked native's state ([`super::Vm::chunk_step`]): no
    /// step runs it, and restore checks nothing in it.
    pub const DONE: u8 = u8::MAX;
}

/// The fuel units of work a chunked native's step does at most (about a
/// millisecond and a half; a unit is about 10 ns of work).
pub(crate) const STEP_WORK: u64 = 1 << 17;

/// What a chunked native's step came to.
pub(crate) enum Step {
    /// The operation's value; `UNDEFINED` with a request when the rest is
    /// handed to Python code (the request's value is the operation's).
    Done(Value),
    /// More steps to go.
    More,
}

/// Keys a sort computes between two safepoints (about 1 ms with `key=len`).
const KEY_CHUNK: usize = 1 << 12;

/// Each operation's name and state layout (value slots, raw words), in
/// [`op`] order; part of the build fingerprint.
pub(crate) const STEPS: [(&str, u32, u32); 21] = [
    ("sort_keys", 4, 1),
    ("sort_done", 4, 1),
    ("seq_in", 2, 3),
    ("bulk", 8, 7),
    ("seq_build", 6, 23),
    ("seq_scan", 2, 6),
    ("str_fill", 4, 8),
    ("str_join", 4, 8),
    ("str_replace", 4, 8),
    ("str_split", 4, 8),
    ("str_map", 4, 8),
    ("str_scan", 4, 8),
    ("num_text", 2, 10),
    ("math_fold", 3, 11),
    ("bytes", 4, 2 + crate::bytes::JOB_WORDS as u32),
    ("json_scan", 5, 16),
    ("json_dump", 6, 6),
    ("uni_norm", 3, 10),
    ("str_pct", 5, 6),
    ("fold", 3, 7),
    ("dv_in", 2, 3),
];

/// A state's value slots and raw words.
type State<const N: usize, const M: usize> = ([Value; N], [u64; M]);

impl Vm {
    /// A new state object for `op`: its value slots, then its raw words.
    pub(crate) fn nstate(&mut self, op: u8, vals: &[Value], raw: &[u64]) -> Result<Value, Raised> {
        if STEPS.get(op as usize).map(|s| (s.1, s.2)) != Some((vals.len() as u32, raw.len() as u32))
        {
            return Err(self.bad_step());
        }
        let res = self.heap.alloc_init(ty::NSTATE, 0, vals, raw);
        let r = self.hx_o(res)?;
        Ok(Value::from_ref(r, ty::NSTATE as u8))
    }

    /// The value slots and raw words of `state`, checked against `op`'s
    /// layout.
    #[inline]
    pub(crate) fn nstate_get<const N: usize, const M: usize>(
        &self,
        op: u8,
        state: Value,
    ) -> Option<State<N, M>> {
        let w = self.nstate_words(op, state, N, M)?;
        let mut vals = [Value::NONE; N];
        for (o, &x) in vals.iter_mut().zip(w) {
            *o = Value::from_bits(x)?;
        }
        let mut raw = [0; M];
        raw.copy_from_slice(w.get(N..N + M)?);
        Some((vals, raw))
    }

    /// The words of `state` if it has `op`'s layout of `n` value slots and
    /// `m` raw words. Out of line: every step reads its state through it,
    /// and inlined it cost about 160 B of wasm a call site [M].
    #[inline(never)]
    fn nstate_words(&self, op: u8, state: Value, n: usize, m: usize) -> Option<&[u64]> {
        let &(_, nv, nw) = STEPS.get(op as usize)?;
        if !state.is_obj_kind(ty::NSTATE as u8) || nv as usize != n || nw as usize != m {
            return None;
        }
        let (h, w) = self.heap.object_words(state.as_obj()?).ok()?;
        if h.nvals() != nv || h.len() != nv + nw {
            return None;
        }
        Some(w)
    }

    /// Sets raw word `i` of state `state` (a chunked native's progress).
    #[inline(never)]
    pub(crate) fn nstate_set_raw(&mut self, state: Value, i: u32, w: u64) {
        self.nstate_put(state, i, &[w]);
    }

    /// Sets raw words `i..` of state `state` to `ws`, one call for a
    /// step's whole progress.
    #[inline(never)]
    pub(crate) fn nstate_put(&mut self, state: Value, i: u32, ws: &[u64]) {
        if let Some(r) = state.as_obj()
            && let Ok(h) = self.heap.object_words(r).map(|(h, _)| h)
        {
            let at = h.nvals() + i;
            for (k, &w) in (at..).zip(ws) {
                self.set_raw_o(r, k, w);
            }
        }
    }

    /// The rest of chunked native `op`, whose first step (the native
    /// itself, allocating nothing while its work is short) ran out of
    /// [`STEP_WORK`]: a state of value slots `vals` and raw words `raw`
    /// (after the operation word), and a request to `_steps`, which runs
    /// the next steps a call each.
    pub(crate) fn chunk_rest(
        &mut self,
        op: u8,
        vals: &[Value],
        raw: &[u64],
    ) -> Result<Value, Raised> {
        let mut words = alloc::vec![u64::from(op)];
        words.extend_from_slice(raw);
        let state = self.nstate(op, vals, &words)?;
        self.prelude_request("_steps", alloc::vec![state], Cont::None)
    }

    /// The next step of the chunked native in `state` (`_more(state)`): its
    /// value, or `state` itself while more steps remain.
    pub(crate) fn chunk_step(&mut self, state: Value) -> Result<Value, Raised> {
        let op = state
            .as_obj()
            .filter(|_| state.is_obj_kind(ty::NSTATE as u8))
            .and_then(|r| {
                let (h, w) = self.heap.object_words(r).ok()?;
                w.get(h.nvals() as usize).copied()
            })
            .and_then(|w| u8::try_from(w).ok());
        let Some(op) = op else {
            return Err(self.bad_step());
        };
        Ok(match self.chunk_op(op, state)? {
            // The last step changed its data in place without saving its
            // words (a pending `σ` written as `ς`, a set table filled),
            // and `_steps` holds the state until its fuel is charged,
            // which may park: marked finished, it is never stepped again
            // or checked against that data on restore. A JSON dump goes on
            // after its keys (a list) are sorted.
            Step::Done(v) => {
                if !(op == op::JSON_DUMP && v.is_obj_kind(ty::LIST as u8))
                    && let Some(r) = state.as_obj()
                    && let Ok(h) = self.heap.header(r)
                {
                    let res = self.heap.set_raw(r, h.nvals(), u64::from(op::DONE));
                    self.hx(res)?;
                }
                v
            }
            Step::More => state,
        })
    }

    /// One step of chunked native `op`.
    fn chunk_op(&mut self, op: u8, state: Value) -> Result<Step, Raised> {
        match op {
            op::SEQ_IN => self.seq_in_step(state),
            op::BULK => self.bulk_step(state),
            op::SEQ_BUILD => self.seq_build_step(state),
            op::SEQ_SCAN => self.seq_scan_step(state),
            op::STR_FILL..=op::STR_SCAN => self.str_step(op, state),
            op::NUM_TEXT => self.text_step(state),
            op::MATH_FOLD => self.fold_step(state),
            op::BYTES => self.job_step(state),
            #[cfg(feature = "json")]
            op::JSON_SCAN => self.json_scan_step(state),
            #[cfg(feature = "json")]
            op::JSON_DUMP => self.json_dump_step(state),
            #[cfg(feature = "unicodedata")]
            op::UNI_NORM => self.uni_norm_step(state),
            op::STR_PCT => self.pct_step(state),
            op::FOLD => crate::builtins::fold_step(self, state),
            op::DV_IN => self.dv_in_step(state),
            _ => Err(self.bad_step()),
        }
    }

    #[cold]
    pub(crate) fn bad_step(&mut self) -> Raised {
        self.err(T::SystemError, "micro-monty: bad native continuation")
    }

    /// Starts a keyed sort of `xs` (a list the operation owns): calls `key`
    /// on each item, then sorts by the keys; with `key` `None` the items
    /// are the keys (a long sort without `key=`). `target` is the list
    /// `list.sort` sorts, its items already detached, or `None`.
    pub(crate) fn sort_start(
        &mut self,
        xs: Value,
        key: Value,
        rev: bool,
        target: Value,
    ) -> Result<Value, Raised> {
        let keys = if key == Value::NONE {
            xs
        } else {
            // Room for every key: growing by doubling would copy them all
            // in one step.
            let n = self.list_len(xs);
            self.list_with_room(0, n)?
        };
        let state = self.nstate(op::SORT_KEYS, &[xs, keys, key, target], &[u64::from(rev)])?;
        let n = self.native_step(op::SORT_KEYS, state, Value::UNDEFINED)?;
        self.next_request(n)
    }

    /// The callee a step invoked returned `v` (`UNDEFINED` to start): the
    /// operation's next step.
    pub(crate) fn native_step(&mut self, op: u8, state: Value, v: Value) -> Result<Next, Raised> {
        let Some(([xs, keys, key, target], [rev])) = self.nstate_get::<4, 1>(op, state) else {
            return Err(self.bad_step());
        };
        // The sort's lists, and the list `list.sort` gets its items back
        // in (or `None`): its buffer and length are written there. A
        // state has no operation word to mark it finished by, so a forged
        // one is refused here, whatever restore saw.
        if !xs.is_obj_kind(ty::LIST as u8)
            || !keys.is_obj_kind(ty::LIST as u8)
            || !(target == Value::NONE || target.is_obj_kind(ty::LIST as u8))
        {
            return Err(self.bad_step());
        }
        let rev = rev != 0;
        let sorted = match op {
            op::SORT_KEYS => {
                if v != Value::UNDEFINED {
                    self.list_push(keys, v)?;
                }
                let i = self.list_len(keys);
                if let Some(&x) = self.seq_words(xs).get(i) {
                    let x = Value::from_bits(x).unwrap_or(Value::NONE);
                    let cont = Cont::Native { op, state };
                    let chunk = i % KEY_CHUNK == KEY_CHUNK - 1;
                    if chunk || self.long_work {
                        // A builtin key returns without a safepoint: a chunk
                        // of keys is charged, and its last call goes through
                        // a prelude frame, whose call checks; so does the
                        // call after a key that did long work (`str` of a
                        // big int).
                        if chunk {
                            self.burn(KEY_CHUNK as u64);
                        }
                        let f = self.prelude_fn("_invoke")?;
                        let a = self.alloc_tuple(&[x])?;
                        let args = Args::of(&[key, a, Value::NONE]);
                        return Ok(Next::Invoke(f, args, Value::NONE, cont));
                    }
                    return Ok(Next::Invoke(key, Args::of(&[x]), Value::NONE, cont));
                }
                if i != self.list_len(xs) {
                    return Err(self.bad_step());
                }
                let sorted = self.sort_by_keys(xs, keys, rev);
                if sorted.is_err() {
                    // Out of memory for the arrays: the list gets its items
                    // back, as when a later step fails.
                    self.sort_reattach(xs, target);
                }
                match sorted? {
                    Sorted::Done(v) => v,
                    Sorted::More(st) => {
                        // A long sort: the rest a slice per loop step.
                        let f = self.prelude_fn("_sort_rest")?;
                        let cont = Cont::Native {
                            op: op::SORT_DONE,
                            state,
                        };
                        let args = Args::of(&[xs, keys, Value::from_bool(rev), st]);
                        return Ok(Next::Invoke(f, args, Value::NONE, cont));
                    }
                    Sorted::Fail(OrdErr::Py) => {
                        // Keys that compare in Python: the prelude sorts.
                        let f = self.prelude_fn("_sort_keys")?;
                        let cont = Cont::Native {
                            op: op::SORT_DONE,
                            state,
                        };
                        let args = Args::of(&[xs, keys, Value::from_bool(rev)]);
                        return Ok(Next::Invoke(f, args, Value::NONE, cont));
                    }
                    Sorted::Fail(e) => {
                        self.sort_reattach(xs, target);
                        return Err(self.order_error(e, "<"));
                    }
                }
            }
            op::SORT_DONE if v.is_obj_kind(ty::LIST as u8) => v,
            _ => return Err(self.bad_step()),
        };
        if target == Value::NONE {
            return Ok(Next::Acc(sorted));
        }
        // `list.sort`: the list must still be empty (CPython detaches its
        // items while it sorts); the sorted items go back either way.
        let modified = self.list_len(target) != 0;
        self.list_move(target, sorted)?;
        if modified {
            return Err(self.err(T::ValueError, "list modified during sort"));
        }
        Ok(Next::Acc(Value::NONE))
    }

    /// The callee a step invoked raised: restore what the operation must,
    /// then keep unwinding.
    pub(crate) fn native_unwind(&mut self, op: u8, state: Value) -> Next {
        if let Some(([xs, _, _, target], _)) = self.nstate_get::<4, 1>(op, state) {
            self.sort_reattach(xs, target);
        }
        Next::Pass
    }

    /// `list.sort` failed: its list gets its items back, unsorted.
    fn sort_reattach(&mut self, xs: Value, target: Value) {
        if target.is_obj_kind(ty::LIST as u8) && xs.is_obj_kind(ty::LIST as u8) {
            let _ = self.list_move(target, xs);
        }
    }
}

impl Vm {
    /// `v` as a native that may hand the rest of a chunked native to
    /// `_steps` gave it (`UNDEFINED` with that request), for a caller that
    /// needs the value itself (`math.prod` of a `bytes` repeat): the
    /// remaining steps run here, in one long step.
    pub(crate) fn steps_done(&mut self, v: Value) -> Result<Value, Raised> {
        if v != Value::UNDEFINED {
            return Ok(v);
        }
        let steps = self.prelude_fn("_steps")?;
        let state = match &self.request {
            Some(crate::vm::Request::Call { func, args, .. }) if *func == steps => {
                args.first().copied()
            }
            _ => None,
        };
        let Some(state) = state else { return Ok(v) };
        self.request = None;
        loop {
            let r = self.chunk_step(state)?;
            if r != state {
                return Ok(r);
            }
        }
    }
}
