//! `_mm`: engine helpers for the bundled Python modules (docs/spec.md
//! §12). The stdlib builds what CPython builds with `exec` (namedtuple's
//! `__new__`, dataclasses' `__init__`) from these, without compiling
//! source at run time.

use crate::types::{T, ty};
use crate::vm::{CallArgs, CodeObj, GSLOT_PRELUDE, Raised, Vm};
use alloc::{format, string::String, sync::Arc, vec::Vec};
use mm_compile::bytecode::{Code, ParamSpec, op};
use mm_core::Value;

/// Signature codes one session may synthesize (each is outside the arena).
const MAX_SYNTH: usize = 4096;

/// Most parameters a synthesized signature takes (registers are bytes).
const MAX_PARAMS: usize = 126;

/// `_mm.signature(name, qualname, params, kwonly, defaults, kwdefaults,
/// impl)`: a Python function named `name`/`qualname` whose parameters are
/// `params` (positional) and `kwonly` (keyword-only), with `defaults` (a
/// tuple or `None`) and `kwdefaults` (a dict or `None`); a call binds its
/// arguments as CPython would (and reports errors the same way) and
/// returns `impl(*params, *kwonly)`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_signature(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (name, qualname) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    let (defaults, kwdefaults, imp) = (vm.arg_o(&a, 4), vm.arg_o(&a, 5), vm.arg_o(&a, 6));
    let mut names: Vec<String> = Vec::new();
    for i in [2, 3] {
        for &v in &vm.collect(vm.arg_o(&a, i))? {
            if !Vm::is_str(v) {
                return Err(vm.err(T::TypeError, "parameter names must be str"));
            }
            names.push(String::from(vm.str_ref_o(v)));
        }
    }
    let kwonly = vm.collect(vm.arg_o(&a, 3))?.len();
    let argc = names.len() - kwonly;
    if !Vm::is_str(name) || !Vm::is_str(qualname) {
        return Err(vm.err(T::TypeError, "names must be str"));
    }
    if defaults != Value::NONE && !defaults.is_obj_kind(ty::TUPLE as u8) {
        return Err(vm.err(T::TypeError, "defaults must be a tuple or None"));
    }
    if kwdefaults != Value::NONE && !kwdefaults.is_obj_kind(ty::DICT as u8) {
        return Err(vm.err(T::TypeError, "kwdefaults must be a dict or None"));
    }
    if names.len() > MAX_PARAMS {
        let msg = format!("micro-monty does not support more than {MAX_PARAMS} parameters here");
        return Err(vm.err(T::ValueError, &msg));
    }
    let found = vm.synth.binary_search_by(|&ci| {
        let c = &vm.codes[ci as usize].code;
        (c.params.argcount as usize).cmp(&argc).then_with(|| {
            c.varnames[..c.varnames.len() - 1]
                .iter()
                .map(|s| &**s)
                .cmp(names.iter().map(String::as_str))
        })
    });
    let ci = match found {
        Ok(i) => vm.synth[i],
        Err(i) => {
            if vm.synth.len() >= MAX_SYNTH {
                return Err(vm.err(T::MemoryError, ""));
            }
            if vm.synth.len() == vm.synth.capacity() {
                let extra = vm.synth.len().max(4);
                vm.reserve_code(extra as u64 * size_of::<u32>() as u64)?;
                let result = vm.synth.try_reserve_exact(extra);
                vm.reserve_code(0)?;
                result.map_err(|_| vm.err(T::MemoryError, ""))?;
            }
            let ci = vm.add_code(signature_code(&names, argc))?;
            vm.synth.insert(i, ci);
            ci
        }
    };
    let cell = vm.new_cell(imp)?;
    let closure = vm.alloc_tuple(&[cell])?;
    let r = {
        let res = vm.heap.alloc_init(
            ty::FUNC,
            0,
            &[
                name,
                qualname,
                defaults,
                kwdefaults,
                closure,
                Value::NONE,
                Value::NONE,
            ],
            &[u64::from(ci)],
        );
        vm.hx_o(res)
    }?;
    Ok(Value::from_ref(r, ty::FUNC as u8))
}

/// The code of a signature function: parameters in registers `0..n`, the
/// implementation's cell in `n`, then `impl(*params)`.
#[cfg_attr(mm_shipped, optimize(size))]
fn signature_code(names: &[String], argc: usize) -> Code {
    let n = names.len();
    // `n <= MAX_PARAMS`, so every register fits a byte.
    let reg = |i: usize| u8::try_from(i).unwrap_or(u8::MAX);
    let word = |b: [u8; 4]| u32::from_le_bytes(b);
    let mut bc = Vec::with_capacity(n + 5);
    bc.push(word([op::LDA_DEREF, reg(n), 0, 0]));
    bc.push(word([op::STAR, reg(n + 1), 0, 0]));
    // Arguments from `R[n+3]`: `R[n+2]` is the call's link slot.
    for i in 0..n {
        bc.push(word([op::MOV, reg(i), reg(n + 3 + i), 0]));
    }
    // `CALL r n ic`: the IC operand is the second word.
    bc.extend_from_slice(&[word([op::CALL, reg(n + 1), reg(n), 0]), 0]);
    bc.push(u32::from(op::RETURN));
    let mut varnames: Vec<Arc<str>> = names.iter().map(|s| Arc::from(s.as_str())).collect();
    varnames.push(Arc::from("impl"));
    Code {
        name: Arc::from("<signature>"),
        qualname: Arc::from("<signature>"),
        bytecode: bc,
        nregs: u32::try_from(2 * n + 3).unwrap_or(u32::MAX),
        varnames,
        params: ParamSpec {
            argcount: u32::try_from(argc).unwrap_or(0),
            posonly: 0,
            kwonly: u32::try_from(n - argc).unwrap_or(0),
            varargs: false,
            varkw: false,
        },
        freevars: alloc::vec![reg(n)],
        freenames: alloc::vec![Arc::from("impl")],
        n_ics: 1,
        ..Code::default()
    }
}

impl Vm {
    /// Adds an engine code object (hidden from tracebacks, no globals) to
    /// the code table; returns its index.
    #[cfg_attr(mm_shipped, optimize(size))]
    pub(crate) fn add_code(&mut self, code: Code) -> Result<u32, Raised> {
        let Ok(ci) = u32::try_from(self.codes.len()) else {
            return Err(self.err(T::MemoryError, ""));
        };
        self.reserve_code(CodeObj::load_bytes(&code))?;
        if Arc::make_mut(&mut self.codes).try_reserve_exact(1).is_err()
            || self.ics.try_reserve_exact(code.n_ics as usize).is_err()
        {
            self.reserve_code(0)?;
            return Err(self.err(T::MemoryError, ""));
        }
        let ic_base = u32::try_from(self.ics.len()).unwrap_or(u32::MAX);
        self.ics.extend(core::iter::repeat_n(
            crate::vm::Ic::Empty,
            code.n_ics as usize,
        ));
        Arc::make_mut(&mut self.codes).push(Arc::new(CodeObj {
            code: code.into(),
            consts: Vec::new(),
            names: Vec::new(),
            ic_base,
            module_base: ci,
            source: u32::MAX,
            prelude: true,
            gslot: GSLOT_PRELUDE,
            reclaim_live: false,
            reclaim_index: 0,
        }));
        self.reserve_code(0)?;
        Ok(ci)
    }
}

/// `_mm.backing(obj)`: the builtin value an instance of a builtin-type
/// subclass extends (`obj` itself for others).
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_backing(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    Ok(vm.unbx(vm.arg_o(&a, 0)))
}

/// `types.MethodType(func, obj)` (`type(obj.method)` called): `func` bound
/// to `obj`.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_method(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let (f, obj) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !crate::builtins::is_callable(vm, f) {
        return Err(vm.err(T::TypeError, "first argument must be callable"));
    }
    if obj == Value::NONE {
        return Err(vm.err(T::TypeError, "instance must not be None"));
    }
    vm.new_method(f, obj)
}

/// `_mm.gc_collect()`: asks for a collection that starts after this call,
/// run a step per safepoint (`gc.collect` waits for it with `gc_wait`).
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_gc_collect(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    vm.collect_in_steps(true);
    Ok(Value::NONE)
}

/// `_mm.gc_wait()`: whether the collection `gc_collect` asked for is still
/// running (the next safepoint then runs its next step).
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
pub(crate) fn mm_gc_wait(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    Ok(Value::from_bool(vm.gc_waiting()))
}

/// `_mm.caller(stacklevel, skip_file_prefixes)`: the frame CPython's
/// `_warnings.warn` attributes a warning to, as `(filename, lineno,
/// globals)`, or `None` when `stacklevel` runs off the stack. Level 1 is
/// the caller of the Python function that calls this; each further level
/// steps to the next frame whose filename starts with none of the
/// prefixes. Prelude frames are skipped: CPython runs that code in C.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_caller(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let level = vm.int_arg(vm.arg_o(&a, 0), "")?;
    let mut prefixes: Vec<String> = Vec::new();
    for &p in &vm.collect(vm.arg_o(&a, 1))? {
        if Vm::is_str(p) {
            prefixes.push(String::from(vm.str_ref_o(p)));
        }
    }
    let frames =
        crate::interp::walk_frames(&vm.stack, &vm.specials, &vm.codes, vm.frame_base, vm.depth)
            .unwrap_or_default();
    vm.burn(frames.len() as u64);
    let file = |ci: u32| vm.source_of(ci).map_or("", |s| s.name.as_str());
    let mut user = frames.iter().filter(|f| {
        vm.codes
            .get(f.code as usize)
            .is_some_and(|c| c.gslot != GSLOT_PRELUDE)
    });
    let mut found = user.next();
    for _ in 1..level {
        if found.is_none() {
            break;
        }
        found = user.by_ref().find(|f| {
            !prefixes
                .iter()
                .any(|p| file(f.code).starts_with(p.as_str()))
        });
    }
    let Some(f) = found.copied() else {
        return Ok(Value::NONE);
    };
    let Some(c) = vm.codes.get(f.code as usize).cloned() else {
        return Ok(Value::NONE);
    };
    let line = c.code.position(f.call_pc).map_or(0, |p| p.pos.line);
    let name = String::from(file(f.code));
    let parts = [
        vm.new_str(&name)?,
        vm.new_int(i64::from(line))?,
        vm.globals_of(&c),
    ];
    vm.alloc_tuple(&parts)
}

/// `_mm.code_kind(f)`: 0 for anything but a Python function, else 1, or
/// 2 for a generator function, 3 for a coroutine function, 4 for an async
/// generator function (`types.coroutine` reads it).
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_code_kind(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let f = vm.arg_o(&a, 0);
    let mut kind = 0;
    if f.is_obj_kind(ty::FUNC as u8) {
        let ci = vm.vslot_raw_of(f, crate::vm::fn_slot::CODE);
        if let Some(c) = usize::try_from(ci).ok().and_then(|i| vm.codes.get(i)) {
            kind = match (c.code.is_generator, c.code.is_coroutine) {
                (true, true) => 4,
                (true, false) => 2,
                (false, true) => 3,
                (false, false) => 1,
            };
        }
    }
    Ok(Value::int(kind).unwrap_or(Value::NONE))
}

/// `_mm.clock()`: the host's wall clock, seconds since the epoch.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_clock(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    match &vm.wall_clock {
        Some(f) => {
            let t = f();
            Ok(Value::float(t))
        }
        None => Err(vm.err(
            T::RuntimeError,
            "this sandbox has no clock: the host has not provided one",
        )),
    }
}

/// `_mm.local_zone()`: the host's local time zone as TZif bytes, or None
/// (UTC).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_local_zone(vm: &mut Vm, _a: CallArgs) -> Result<Value, Raised> {
    match vm.local_zone.take() {
        Some(z) => {
            let r = vm.new_bytes(&z);
            vm.local_zone = Some(z);
            r
        }
        None => Ok(Value::NONE),
    }
}

/// `_mm.tzif(key)`: zone `key`'s TZif bytes from the host's zone source,
/// `None` if it has no such zone, `False` if there is no source.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_tzif(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let key = vm.arg_o(&a, 0);
    if !Vm::is_str(key) {
        return Err(vm.err(T::TypeError, "zone key must be str"));
    }
    let key = String::from(vm.str_ref_o(key));
    let Some(source) = vm.zone_source.as_mut() else {
        return Ok(Value::FALSE);
    };
    // `zoneinfo` validates keys first, but guests can import `_mm`: the
    // host sees only relative, normalized keys (docs/user-guide.md).
    if key.contains('\0') || key.split('/').any(|p| matches!(p, "" | "." | "..")) {
        return Ok(Value::NONE);
    }
    match source(&key) {
        Some(data) => vm.new_bytes(&data),
        None => Ok(Value::NONE),
    }
}

/// `_mm.urandom(n)`: `n` bytes from the host's entropy source.
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_urandom(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let n = vm.int_arg(vm.arg_o(&a, 0), "")?;
    let Ok(n) = usize::try_from(n) else {
        return Err(vm.err(T::ValueError, "negative argument not allowed"));
    };
    if !vm.heap.would_fit(n as u64 + 64) {
        return Err(vm.err(T::MemoryError, ""));
    }
    let mut buf = alloc::vec![0u8; n];
    match &mut vm.entropy {
        Some(f) => f(&mut buf),
        None => {
            return Err(vm.err(
                T::RuntimeError,
                "this sandbox has no source of randomness: the host has not provided one",
            ));
        }
    }
    vm.burn(n as u64 / 64);
    vm.new_bytes(&buf)
}

/// `_mm.os_function(name)`: the host function the embedder registered to
/// handle OS operation `name` (`os.getenv`, `Path.read_text`, ...), or
/// `None` (docs/spec.md §10: OS operations are host calls).
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_os_function(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let name = vm.arg_o(&a, 0);
    if !Vm::is_str(name) {
        return Err(vm.err(T::TypeError, "name must be str"));
    }
    let k = vm.intern(&String::from(vm.str_ref_o(name)))?;
    Ok(vm.dict_get_str(vm.builtins, k).unwrap_or(Value::NONE))
}

/// `_mm.exc_same_meta(a, b)`: CPython's `is_same_exception_metadata` for
/// `except*`: the same cause and context, and the same traceback (entries
/// here, as tracebacks are per-exception lists). CPython also compares a
/// `notes` slot that `add_note` never sets, so notes do not count.
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_exc_same_meta(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    use crate::vm::exc_slot;
    let (x, y) = (vm.arg_o(&a, 0), vm.arg_o(&a, 1));
    if !x.is_obj_kind(ty::EXC as u8) || !y.is_obj_kind(ty::EXC as u8) {
        return Ok(Value::FALSE);
    }
    let tb = |vm: &Vm, e: Value| {
        let t = vm.vslot_o(e, exc_slot::TRACEBACK);
        if t.is_obj_kind(ty::LIST as u8) {
            vm.list_items(t)
        } else {
            Vec::new()
        }
    };
    let same = vm.vslot_o(x, exc_slot::CAUSE) == vm.vslot_o(y, exc_slot::CAUSE)
        && vm.vslot_o(x, exc_slot::CONTEXT) == vm.vslot_o(y, exc_slot::CONTEXT)
        && tb(vm, x) == tb(vm, y);
    Ok(Value::from_bool(same))
}

/// `_mm.exc_tb_here(e)`: `e`'s traceback becomes the innermost user
/// frame alone, at the instruction running there (CPython's
/// `_PyTraceBack_FromFrame` for the group `except*` wraps a naked
/// exception in).
#[allow(clippy::unnecessary_wraps, reason = "the one builtin signature")]
#[cfg_attr(mm_shipped, optimize(size))]
pub(crate) fn mm_exc_tb_here(vm: &mut Vm, a: CallArgs) -> Result<Value, Raised> {
    let e = vm.arg_o(&a, 0);
    let frames =
        crate::interp::walk_frames(&vm.stack, &vm.specials, &vm.codes, vm.frame_base, vm.depth)
            .unwrap_or_default();
    vm.burn(frames.len() as u64);
    let found = frames.iter().find(|f| {
        vm.codes
            .get(f.code as usize)
            .is_some_and(|c| c.gslot != GSLOT_PRELUDE)
    });
    if let (Some(f), Some(r)) = (found.copied(), e.as_obj())
        && e.is_obj_kind(ty::EXC as u8)
    {
        vm.set_slot_o(r, crate::vm::exc_slot::TRACEBACK, Value::NONE);
        vm.add_tb(e, f.code, f.call_pc);
    }
    Ok(Value::NONE)
}
