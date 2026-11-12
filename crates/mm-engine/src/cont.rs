//! Continuations (docs/spec.md §8.2).
//!
//! A VM operation that needs Python code (a dunder, a property getter, a
//! generator, a prelude function) pushes that code's frame with a
//! continuation tag in the caller's frame record and returns to the
//! dispatch loop. When the frame returns (or yields, or raises out), the
//! loop hands the outcome to [`Vm::finish`], which runs the rest of the
//! operation: checks the result, tries the reflected operand, converts a
//! `StopIteration` into a loop exit, and so on. The Rust stack never holds
//! a second dispatch loop.

use crate::class::Dn;
use crate::types::{T, ty};
use crate::vm::{Raised, Vm, exc_slot};
use alloc::{format, string::String, vec, vec::Vec};
use mm_core::{Tracer, Value};

/// Result checks for dunders whose result type is fixed.
pub(crate) mod check {
    pub const REPR: u8 = 0;
    pub const STR: u8 = 1;
    pub const FORMAT: u8 = 2;
    pub const HASH: u8 = 3;
    pub const INDEX: u8 = 4;
    pub const INT: u8 = 5;
    pub const FLOAT: u8 = 6;
    pub const ITER: u8 = 7;
    /// `__index__` for `float()`: an int, converted.
    pub const INDEX_FLOAT: u8 = 8;
    /// `async for`'s `__anext__()`: an awaitable.
    pub const ANEXT: u8 = 9;
    /// `async with`'s `__aenter__()`: an awaitable.
    pub const AENTER: u8 = 10;
    /// `async with`'s `__aexit__()`: an awaitable.
    pub const AEXIT: u8 = 11;
    /// `async for`'s `__aiter__()`: an async iterator.
    pub const AITER_FOR: u8 = 12;
    /// `aiter()`'s `__aiter__()`: an async iterator.
    pub const AITER: u8 = 13;
    /// `await`'s `__await__()`: an iterator, which becomes a delegation.
    pub const AWAIT: u8 = 14;
}

/// Truth-test modes.
pub(crate) mod truth {
    /// `acc = bool(x)`.
    pub const BOOL: u8 = 0;
    /// `acc = not x`.
    pub const NOT: u8 = 1;
    /// Jump if true (acc keeps `x`).
    pub const IF_TRUE: u8 = 2;
    /// Jump if false (acc keeps `x`).
    pub const IF_FALSE: u8 = 3;
}

/// What finishes an operation when the code it called returns.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Cont {
    /// The result is the operation's value.
    None,
    /// The result is discarded (`acc = None`).
    Discard,
    /// `__init__` returned: it must be `None`; the value is the instance.
    Init(Value),
    /// A user `__new__` returned: `__init__` runs with the same arguments
    /// (a tuple and a dict or `None`) if the result is an instance of `cls`.
    NewInit { cls: Value, args: Value, kw: Value },
    /// Raise the result (`raise C` instantiated `C`).
    Raise,
    /// A truth test (`__bool__`, or `__len__` when `len`); `orig` is the
    /// tested value, kept in acc for conditional jumps.
    Truth {
        mode: u8,
        target: u32,
        len: bool,
        orig: Value,
    },
    /// A binary operator; `stage` indexes the candidate dunders tried.
    Bin {
        op: u8,
        stage: u8,
        a: Value,
        b: Value,
    },
    /// A rich comparison (`op` is the `TEST_*` opcode).
    Cmp {
        op: u8,
        stage: u8,
        a: Value,
        b: Value,
    },
    /// `__len__`: a non-negative int.
    Len,
    /// A result type check (see [`check`]).
    Check(u8),
    /// `__contains__`: its truth (negated for `not in`).
    Contains(bool),
    /// `FOR_ITER`: exhaustion jumps to the exit.
    ForIter(u32),
    /// `SEND` (`yield from`): exhaustion gives its value and jumps.
    Send(u32),
    /// A `throw` delegated by `yield from`: a yield re-yields at `at`.
    Delegate { exit: u32, at: u32 },
    /// `next(it[, default])`.
    Next(Value),
    /// `generator.send()`/`throw()`: a return raises `StopIteration`.
    GenSend,
    /// `coroutine.send()`: the coroutine's return raises `StopIteration`.
    CoroSend,
    /// One step of the `AWAIT` delegation (a buffer holding the iterator):
    /// a value is a yield, `StopIteration` the `await`'s result.
    AwaitStep(Value),
    /// `generator.close()`.
    Close,
    /// Awaiting an async generator's `asend()`/`athrow()`: a yield is the
    /// result, a return raises `StopAsyncIteration`.
    ASend,
    /// Awaiting an async generator's `aclose()`.
    AClose,
    /// A `iter(callable, sentinel)` step; `exit` is `u32::MAX` for `next()`.
    CallIter { it: Value, exit: u32 },
    /// The `==` of a `CallIter` step's value `v` and the sentinel ran in
    /// Python: true stops the iterator, false yields `v`.
    CallIterEq { it: Value, exit: u32, v: Value },
    /// `__exit__` with an exception: re-raise it unless the result is true.
    WithExit(Value),
    /// Unpacking through the prelude: the list's items go to registers.
    Unpack { r: u8, n: u16 },
    /// `LOAD_METHOD` fell back to an attribute that ran code: `R[r]` =
    /// value, `R[r+1]` = no self.
    StoreReg(u8),
    /// `getattr(o, name, default)`: `AttributeError` gives the default.
    AttrDefault(Value),
    /// `hasattr`.
    HasAttr,
    /// A property getter's `AttributeError` falls back to `__getattr__`.
    GetattrFallback(Value, Value),
    /// A builtin type's constructor returned: the result is an instance of
    /// this subclass extending it.
    BxWrap(Value),
    /// A native step (`nstep.rs`) resumes operation `op` with the result;
    /// `state` is its `NSTATE` object.
    Native { op: u8, state: Value },
    /// A bundled module's body returned: register `module`, then go on
    /// importing `dotted` (`top`: the result is its top-level package;
    /// `from_`: the result is `module` itself).
    Import {
        module: Value,
        dotted: Value,
        top: bool,
        from_: bool,
    },
}

impl Vm {
    /// Stores a frame's continuation; the [`crate::vm::Frame::cont`] value.
    #[inline]
    pub(crate) fn cont_put(&mut self, c: Cont) -> Result<u32, Raised> {
        if matches!(c, Cont::None) {
            return Ok(0);
        }
        if let Some(i) = self.cont_free.pop() {
            if let Some(slot) = self.conts.get_mut(i as usize) {
                *slot = Some(c);
            }
            return Ok(i + 1);
        }
        if self.conts.len() == self.conts.capacity() {
            self.grow_conts()?;
        }
        self.conts.push(Some(c));
        Ok(u32::try_from(self.conts.len()).unwrap_or(0))
    }

    #[cold]
    fn grow_conts(&mut self) -> Result<(), Raised> {
        let new = self.conts.len() + self.conts.len().max(4);
        // Unwinding must never need an allocation to return a continuation.
        self.reserve_execution(
            ((new - self.conts.capacity()) * size_of::<Option<Cont>>()
                + new.saturating_sub(self.cont_free.capacity()) * size_of::<u32>())
                as u64,
        )?;
        let result = self
            .cont_free
            .try_reserve_exact(new - self.cont_free.len())
            .and_then(|()| self.conts.try_reserve_exact(new - self.conts.len()));
        self.reserve_execution(0)?;
        result.map_err(|_| self.err(T::MemoryError, "continuation allocation failed"))
    }

    /// Takes back a frame's continuation (`Cont::None` for 0).
    #[inline]
    pub(crate) fn cont_take(&mut self, idx: u32) -> Cont {
        if idx == 0 {
            return Cont::None;
        }
        let c = self
            .conts
            .get_mut(idx as usize - 1)
            .and_then(Option::take)
            .unwrap_or(Cont::None);
        self.cont_free.push(idx - 1);
        c
    }
}

impl Cont {
    /// Traces the values this continuation holds without copying roots.
    pub(crate) fn trace(&self, out: &mut Tracer<'_>) {
        match *self {
            Cont::Init(v)
            | Cont::Truth { orig: v, .. }
            | Cont::Next(v)
            | Cont::CallIter { it: v, .. }
            | Cont::WithExit(v)
            | Cont::AttrDefault(v)
            | Cont::AwaitStep(v) => out.value(v),
            Cont::NewInit { cls, args, kw } => out.values(&[cls, args, kw]),
            Cont::Import { module, dotted, .. } => out.values(&[module, dotted]),
            Cont::BxWrap(c) | Cont::Native { state: c, .. } => out.value(c),
            Cont::Bin { a, b, .. }
            | Cont::Cmp { a, b, .. }
            | Cont::GetattrFallback(a, b)
            | Cont::CallIterEq { it: a, v: b, .. } => {
                out.values(&[a, b]);
            }
            _ => {}
        }
    }
}

/// How the called code ended.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Outcome {
    /// It returned (or a generator yielded) this value.
    Value(Value),
    /// A generator returned (`StopIteration` with this value).
    Stop(Value),
    /// It raised `self.exc`.
    Raise,
}

/// A call's positional arguments: up to four inline (every dunder call),
/// more on the heap.
pub(crate) enum Args {
    Inline(u8, [Value; 4]),
    Heap(Vec<Value>),
}

impl Args {
    pub(crate) fn of(a: &[Value]) -> Args {
        match a.len() {
            n @ 0..=4 => {
                let mut b = [Value::NONE; 4];
                b[..n].copy_from_slice(a);
                Args::Inline(n as u8, b)
            }
            _ => Args::Heap(a.to_vec()),
        }
    }
}

impl From<Vec<Value>> for Args {
    fn from(v: Vec<Value>) -> Args {
        Args::Heap(v)
    }
}

impl core::ops::Deref for Args {
    type Target = [Value];
    fn deref(&self) -> &[Value] {
        match self {
            Args::Inline(n, b) => &b[..*n as usize],
            Args::Heap(v) => v,
        }
    }
}

/// What the operation does next.
pub(crate) enum Next {
    /// Continue after the operation with this accumulator.
    Acc(Value),
    /// Continue at `pc` with this accumulator.
    Jump(u32, Value),
    /// Call (or, for a generator, send to) a function next: the function,
    /// positional arguments, keyword arguments (a dict or `None`).
    Invoke(Value, Args, Value, Cont),
    /// Throw an exception into a generator next.
    Throw(Value, Value, Cont),
    /// The exception was not handled here: keep unwinding.
    Pass,
    /// Re-raise this exception as it is (no new traceback entry).
    Reraise(Value),
    /// The iterator an `AWAIT` delegates to (in this buffer) yielded this
    /// value.
    AwaitYield(Value, Value),
}

impl Vm {
    /// Is the pending exception an instance of `t`?
    pub(crate) fn exc_is(&self, t: T) -> bool {
        self.instance_of(self.exc, self.type_obj(t))
    }

    /// `StopIteration(v)` (no arguments for `None`), raised.
    pub(crate) fn raise_stop(&mut self, v: Value) -> Raised {
        let args: Vec<Value> = if v == Value::NONE {
            Vec::new()
        } else {
            vec![v]
        };
        match self.new_exception(T::StopIteration, &args) {
            Ok(e) => {
                self.exc = e;
                Raised
            }
            Err(r) => r,
        }
    }

    /// The `value` of a `StopIteration`.
    pub(crate) fn stop_value(&self, e: Value) -> Value {
        let args = self.vslot(e, exc_slot::ARGS);
        if self.tuple_len(args) == 0 {
            Value::NONE
        } else {
            self.vslot(args, 0)
        }
    }

    /// Finishes an operation whose called code ended with `out`. Called
    /// with the caller's frame active; `Err` raises at the call.
    #[cold]
    pub(crate) fn finish(&mut self, base: usize, cont: Cont, out: Outcome) -> Result<Next, Raised> {
        use Outcome::{Raise, Stop, Value as Val};
        Ok(match (cont, out) {
            (_, Raise) => self.finish_exc(cont)?,
            // A generator that returns on `GeneratorExit` gives `close()`
            // its return value (3.13+).
            (Cont::None | Cont::ASend, Val(v)) | (Cont::Close, Stop(v)) => Next::Acc(v),
            (Cont::None | Cont::GenSend, Stop(v)) | (Cont::CoroSend, Val(v) | Stop(v)) => {
                return Err(self.raise_stop(v));
            }
            (Cont::Discard, Val(_) | Stop(_)) | (Cont::AClose, Stop(_)) => Next::Acc(Value::NONE),
            (Cont::NewInit { cls, args, kw }, Val(v) | Stop(v)) => {
                if self.instance_of(v, cls)
                    && let Some(init) = self.user_method(v, Dn::Init)
                {
                    let items = self.tuple_items(args);
                    let n = self.call_dunder(init, v, &items, Cont::Init(v))?;
                    match n {
                        Next::Invoke(f, a, _, c) => Next::Invoke(f, a, kw, c),
                        other => other,
                    }
                } else if matches!(self.class_base_t(cls), T::list | T::dict | T::set)
                    && self.instance_of(v, cls)
                    && (self.tuple_len(args) > 0 || kw != Value::NONE)
                    && let k = self.dn(Dn::Init)?
                    && let Some(init) = self.class_lookup(cls, k)
                {
                    // A builtin `__init__` fills a list, dict or set its
                    // `__new__` left empty: `class L(list)` with only `__new__`.
                    let items = self.tuple_items(args);
                    let n = self.call_dunder(init, v, &items, Cont::Init(v))?;
                    match n {
                        Next::Invoke(f, a, _, c) => Next::Invoke(f, a, kw, c),
                        other => other,
                    }
                } else {
                    if v.is_obj_kind(ty::EXC as u8)
                        && self.instance_of(v, cls)
                        && let Some(r) = v.as_obj()
                    {
                        // `BaseException.__init__` stores the arguments.
                        self.set_slot(r, exc_slot::ARGS, args);
                    }
                    Next::Acc(v)
                }
            }
            (Cont::BxWrap(cls), Val(v) | Stop(v)) => Next::Acc(self.new_bx(cls, v)?),
            (Cont::Native { op, state }, Val(v) | Stop(v)) => self.native_step(op, state, v)?,
            (
                Cont::Import {
                    module,
                    dotted,
                    top,
                    from_,
                },
                Val(_) | Stop(_),
            ) => {
                self.imported(module)?;
                if from_ {
                    Next::Acc(module)
                } else {
                    let v = self.import(dotted, top)?;
                    if v == Value::UNDEFINED {
                        match self.request.take() {
                            Some(crate::vm::Request::Call {
                                func,
                                args,
                                kw,
                                cont,
                                ..
                            }) => Next::Invoke(func, args, kw, cont),
                            _ => return Err(self.err(T::SystemError, "micro-monty: bad import")),
                        }
                    } else {
                        Next::Acc(v)
                    }
                }
            }
            (Cont::Init(inst), Val(v) | Stop(v)) => {
                if v != Value::NONE {
                    let msg = format!(
                        "__init__() should return None, not '{}'",
                        String::from(self.type_name(v))
                    );
                    return Err(self.err(T::TypeError, &msg));
                }
                Next::Acc(inst)
            }
            (Cont::Raise, Val(v) | Stop(v)) => {
                let e = self.raisable(v)?;
                self.exc = e;
                return Err(Raised);
            }
            (
                Cont::Truth {
                    mode,
                    target,
                    len,
                    orig,
                },
                Val(v) | Stop(v),
            ) => {
                let b = if len {
                    self.check_len(v)? != 0
                } else if v == Value::TRUE || v == Value::FALSE {
                    v == Value::TRUE
                } else {
                    let msg = format!(
                        "__bool__ should return bool, returned {}",
                        String::from(self.type_name(v))
                    );
                    return Err(self.err(T::TypeError, &msg));
                };
                match mode {
                    truth::BOOL => Next::Acc(Value::from_bool(b)),
                    truth::NOT => Next::Acc(Value::from_bool(!b)),
                    truth::IF_TRUE if b => Next::Jump(target, orig),
                    truth::IF_FALSE if !b => Next::Jump(target, orig),
                    _ => Next::Acc(orig),
                }
            }
            (Cont::Bin { op, stage, a, b }, Val(v) | Stop(v)) => {
                if v == Value::NOT_IMPLEMENTED {
                    self.binop_next(op, a, b, stage + 1)?
                } else {
                    Next::Acc(v)
                }
            }
            (Cont::Cmp { op, stage, a, b }, Val(v) | Stop(v)) => {
                if v == Value::NOT_IMPLEMENTED {
                    // Bit 7 marks an inverted `__eq__`, not the stage.
                    self.cmp_next(op, a, b, (stage & 0x7f) + 1)?
                } else if stage & 0x80 != 0 {
                    // `__ne__` by inverting `__eq__`.
                    Next::Acc(Value::from_bool(!self.truthy_fast(v).unwrap_or(true)))
                } else {
                    Next::Acc(v)
                }
            }
            (Cont::Len, Val(v) | Stop(v)) => {
                let n = self.check_len(v)?;
                Next::Acc(self.new_int(n)?)
            }
            (Cont::Check(k), Val(v) | Stop(v)) => Next::Acc(self.check_result(k, v)?),
            (Cont::Contains(neg), Val(v) | Stop(v)) => {
                let b = self.truthy_fast(v).unwrap_or(true);
                Next::Acc(Value::from_bool(b != neg))
            }
            (
                Cont::ForIter(_)
                | Cont::Next(_)
                | Cont::GenSend
                | Cont::Send(_)
                | Cont::StoreReg(_)
                | Cont::AttrDefault(_)
                | Cont::GetattrFallback(..),
                Val(v),
            ) => {
                if let Cont::StoreReg(r) = cont {
                    let r = base + r as usize;
                    if r + 1 < self.stack.len() {
                        self.stack[r] = v;
                        self.stack[r + 1] = Value::UNDEFINED;
                    }
                }
                Next::Acc(v)
            }
            (Cont::ForIter(exit), Stop(_)) => Next::Jump(exit, Value::NONE),
            (Cont::Send(exit) | Cont::Delegate { exit, .. }, Stop(v)) => Next::Jump(exit, v),
            (Cont::Delegate { at, .. }, Val(v)) => Next::Jump(at, v),
            (Cont::Next(d), Stop(v)) => {
                if d == Value::UNDEFINED {
                    return Err(self.raise_stop(v));
                }
                Next::Acc(d)
            }
            (Cont::Close, Val(_)) => {
                return Err(self.err(T::RuntimeError, "generator ignored GeneratorExit"));
            }
            (Cont::ASend, Stop(_)) => {
                let e = self.new_exception(T::StopAsyncIteration, &[])?;
                self.exc = e;
                return Err(Raised);
            }
            (Cont::AClose, Val(_)) => {
                return Err(self.err(T::RuntimeError, "async generator ignored GeneratorExit"));
            }
            (Cont::CallIter { it, exit }, Val(v) | Stop(v)) => {
                let sentinel = self.vslot(it, 1);
                let mut left = crate::dict::WALK;
                let eq = if v == sentinel {
                    Ok(true)
                } else {
                    // CPython compares `sentinel == v`, the sentinel first.
                    self.eq_hard(sentinel, v, 0, &mut left)
                };
                self.walked(left);
                match eq {
                    Ok(true) => self.calliter_stop(it, exit)?,
                    Ok(false) => Next::Acc(v),
                    // A user `__eq__`: the prelude compares, then
                    // `CallIterEq` decides.
                    Err(crate::dict::Hard::Py) => {
                        let f = self.prelude_fn("_eq_bool")?;
                        let cont = Cont::CallIterEq { it, exit, v };
                        Next::Invoke(f, Args::of(&[sentinel, v]), Value::NONE, cont)
                    }
                    Err(crate::dict::Hard::Deep) => {
                        return Err(self.err(
                            T::RecursionError,
                            "maximum recursion depth exceeded in comparison",
                        ));
                    }
                }
            }
            (Cont::CallIterEq { it, exit, v }, Val(r) | Stop(r)) => {
                if r == Value::TRUE {
                    self.calliter_stop(it, exit)?
                } else {
                    Next::Acc(v)
                }
            }
            (Cont::WithExit(exc), Val(v) | Stop(v)) => {
                if self.truthy_fast(v).unwrap_or(true) {
                    Next::Acc(Value::NONE)
                } else {
                    Next::Reraise(exc)
                }
            }
            (Cont::Unpack { r, n }, Val(v) | Stop(v)) => {
                let items = self.seq_items(v).unwrap_or_default();
                let r = base + r as usize;
                for (i, x) in items.iter().take(n as usize).enumerate() {
                    if let Some(slot) = self.stack.get_mut(r + i) {
                        *slot = *x;
                    }
                }
                Next::Acc(Value::NONE)
            }
            (Cont::HasAttr, Val(_) | Stop(_)) => Next::Acc(Value::TRUE),
            // A value is the iterator's yield; its return ends the `await`.
            (Cont::AwaitStep(w), Val(v) | Stop(v)) => match out {
                Val(_) => Next::AwaitYield(v, w),
                _ => Next::Acc(v),
            },
            (Cont::StoreReg(_) | Cont::AttrDefault(_) | Cont::GetattrFallback(..), Stop(v)) => {
                Next::Acc(v)
            }
        })
    }

    /// The exception path of [`Vm::finish`]: `self.exc` left the called
    /// code.
    fn finish_exc(&mut self, cont: Cont) -> Result<Next, Raised> {
        let stop = || T::StopIteration;
        Ok(match cont {
            Cont::ForIter(exit) if self.exc_is(stop()) => Next::Jump(exit, Value::NONE),
            Cont::Send(exit) | Cont::Delegate { exit, .. } if self.exc_is(stop()) => {
                let v = self.stop_value(self.exc);
                Next::Jump(exit, v)
            }
            Cont::Next(d) if d != Value::UNDEFINED && self.exc_is(stop()) => Next::Acc(d),
            Cont::AwaitStep(_) if self.exc_is(stop()) => Next::Acc(self.stop_value(self.exc)),
            Cont::Close if self.exc_is(T::GeneratorExit) || self.exc_is(stop()) => {
                Next::Acc(Value::NONE)
            }
            Cont::AClose if self.exc_is(T::GeneratorExit) || self.exc_is(T::StopAsyncIteration) => {
                Next::Acc(Value::NONE)
            }
            Cont::CallIter { it, exit } if self.exc_is(stop()) => self.calliter_stop(it, exit)?,
            Cont::AttrDefault(d) if self.exc_is(T::AttributeError) => Next::Acc(d),
            Cont::HasAttr if self.exc_is(T::AttributeError) => Next::Acc(Value::FALSE),
            Cont::GetattrFallback(obj, name) if self.exc_is(T::AttributeError) => {
                let c = self.class_of(obj);
                let key = self.dn(Dn::Getattr)?;
                match self.class_lookup(c, key) {
                    Some(g) => self.call_dunder(g, obj, &[name], Cont::None)?,
                    None => Next::Pass,
                }
            }
            Cont::Native { op, state } => self.native_unwind(op, state),
            Cont::Import { module, .. } => {
                // A module whose body raised leaves `sys.modules`.
                let k = self.vslot(module, crate::modules::module_slot::NAME);
                let mods = self.modules;
                if self.dict_get(mods, k)? == Some(module) {
                    self.dict_del(mods, k)?;
                }
                Next::Pass
            }
            _ => Next::Pass,
        })
    }

    /// A `iter(callable, sentinel)` iterator is exhausted: it stays so, and
    /// the loop exits (or `next()` raises `StopIteration`).
    fn calliter_stop(&mut self, it: Value, exit: u32) -> Result<Next, Raised> {
        if let Some(r) = it.as_obj() {
            self.set_slot(r, 1, Value::UNDEFINED);
        }
        if exit == u32::MAX {
            return Err(self.raise_stop(Value::NONE));
        }
        Ok(Next::Jump(exit, Value::NONE))
    }

    /// The result of `__len__` (or `__len__` in a truth test).
    fn check_len(&mut self, v: Value) -> Result<i64, Raised> {
        let Some(n) = self.int_of(v) else {
            let msg = format!(
                "'{}' object cannot be interpreted as an integer",
                String::from(self.type_name(v))
            );
            return Err(self.err(T::TypeError, &msg));
        };
        if n < 0 {
            return Err(self.err(T::ValueError, "__len__() should return >= 0"));
        }
        Ok(n)
    }

    fn check_result(&mut self, k: u8, v: Value) -> Result<Value, Raised> {
        let tn = String::from(self.type_name(v));
        if k == check::AWAIT {
            if v.is_obj_kind(ty::CORO as u8) {
                return Err(self.err(T::TypeError, "__await__() returned a coroutine"));
            }
            if !self.is_iterator(v) {
                let msg = format!("__await__() returned non-iterator of type '{tn}'");
                return Err(self.err(T::TypeError, &msg));
            }
            return self.new_delegation(v);
        }
        let ok = match k {
            check::REPR | check::STR | check::FORMAT => Vm::is_str(self.unbx(v)),
            check::HASH => self.int_of(v).is_some(),
            // int subclasses give the int they extend (CPython warns).
            check::INDEX | check::INT | check::INDEX_FLOAT => Vm::is_intlike(self.unbx(v)),
            check::FLOAT => v.is_float(),
            check::ANEXT | check::AENTER | check::AEXIT => Vm::is_awaitable(v) || self.has_await(v),
            check::AITER_FOR | check::AITER => self.is_async_iterator(v),
            _ => self.is_iterator(v),
        };
        if ok {
            return Ok(match k {
                check::HASH => {
                    let h = self.int_of(v).unwrap_or(0);
                    self.new_int(if h == -1 { -2 } else { h })?
                }
                check::INDEX | check::INT if v.as_special().is_some() => {
                    self.new_int(self.int_of(v).unwrap_or(0))?
                }
                check::INDEX | check::INT | check::REPR | check::STR => self.unbx(v),
                // An int always converts (OverflowError when too large).
                check::INDEX_FLOAT => Value::float(self.num_to_f64(self.unbx(v))?.unwrap_or(0.0)),
                // `check::FORMAT` among them: `format()` gives what
                // `__format__` returned, a str subclass included, as
                // CPython's `PyObject_Format`.
                _ => v,
            });
        }
        let msg = match k {
            check::REPR => format!("__repr__ returned non-string (type {tn})"),
            check::STR => format!("__str__ returned non-string (type {tn})"),
            check::FORMAT => format!("__format__ must return a str, not {tn}"),
            check::HASH => String::from("__hash__ method should return an integer"),
            check::INDEX | check::INDEX_FLOAT => format!("__index__ returned non-int (type {tn})"),
            check::INT => format!("__int__ returned non-int (type {tn})"),
            check::FLOAT => format!("__float__ returned non-float (type {tn})"),
            check::ANEXT => format!("'async for' received an invalid object from __anext__: {tn}"),
            check::AENTER | check::AEXIT => format!(
                "'async with' received an object from __a{}__ that does not implement __await__: {tn}",
                if k == check::AENTER { "enter" } else { "exit" }
            ),
            check::AITER_FOR => format!(
                "'async for' received an object from __aiter__ that does not implement __anext__: {tn}"
            ),
            check::AITER => format!("aiter() returned not an async iterator of type '{tn}'"),
            _ => format!("iter() returned non-iterator of type '{tn}'"),
        };
        Err(self.err(T::TypeError, &msg))
    }

    /// Can `v` be awaited?
    pub(crate) fn is_awaitable(v: Value) -> bool {
        matches!(
            v.obj_kind().map(u16::from),
            Some(ty::CORO | ty::TASK | ty::FUTURE | ty::GATHER | ty::SLEEP | ty::AGENOP)
        )
    }

    /// Does `v`'s class define `__await__`?
    pub(crate) fn has_await(&mut self, v: Value) -> bool {
        let Ok(key) = self.intern("__await__") else {
            return false;
        };
        let cls = self.class_of(v);
        self.class_lookup(cls, key).is_some()
    }

    /// Does `v` support `anext()`?
    pub(crate) fn is_async_iterator(&mut self, v: Value) -> bool {
        v.is_obj_kind(ty::AGEN as u8) || self.user_method(v, Dn::Anext).is_some()
    }

    /// Does `v` support `next()`?
    pub(crate) fn is_iterator(&self, v: Value) -> bool {
        matches!(
            v.obj_kind().map(u16::from),
            Some(ty::ITER | ty::ENUMERATE | ty::ZIP | ty::REVERSED | ty::GEN | ty::CALLITER)
        ) || self.uflags(v) & crate::class::f::NEXT != 0
    }
}
