//! The dunder protocol as VM operations use it (docs/spec.md §8.2).
//!
//! An operation on a user object finds the Python method, and instead of
//! calling it leaves a request: "call this with these arguments, then
//! finish with this continuation". The dispatch loop pushes the frame. The
//! continuation does the rest when the method returns: tries the reflected
//! operand after `NotImplemented`, checks result types, turns the result
//! into a jump. Operations on builtin values keep their native paths.

use crate::class::{BINOP_DUNDERS, Dn, f};
use crate::cont::{Args, Cont, Next, check, truth};
use crate::types::{T, ty};
use crate::vm::{Raised, Request, Vm};
use alloc::{format, string::String, vec, vec::Vec};
use mm_compile::ast::BinOp;
use mm_compile::bytecode::op;
use mm_core::Value;

impl Vm {
    /// The Python-defined method `name` of user object `v`'s class (not a
    /// builtin default, not `None`).
    pub(crate) fn user_method(&mut self, v: Value, name: Dn) -> Option<Value> {
        let m = self.dunder(v, name)?;
        (m != Value::NONE).then_some(m)
    }

    /// A call of class attribute `m` as a method of `obj`.
    pub(crate) fn call_dunder(
        &mut self,
        m: Value,
        obj: Value,
        args: &[Value],
        cont: Cont,
    ) -> Result<Next, Raised> {
        if m.is_obj_kind(ty::FUNC as u8) {
            let all = match *args {
                [] => Args::of(&[obj]),
                [a] => Args::of(&[obj, a]),
                [a, b] => Args::of(&[obj, a, b]),
                _ => {
                    let mut all = Vec::with_capacity(args.len() + 1);
                    all.push(obj);
                    all.extend_from_slice(args);
                    Args::from(all)
                }
            };
            return Ok(Next::Invoke(m, all, Value::NONE, cont));
        }
        let c = self.class_of(obj);
        let b = self.bind_attr(m, obj, c)?;
        if b == Value::UNDEFINED {
            self.request = None;
            return Err(self.err(
                T::TypeError,
                "micro-monty does not support properties as special methods",
            ));
        }
        Ok(Next::Invoke(b, Args::of(args), Value::NONE, cont))
    }

    /// A VM operation's result from `n`: a value, or `UNDEFINED` with the
    /// call left as a request.
    pub(crate) fn next_request(&mut self, n: Next) -> Result<Value, Raised> {
        match n {
            Next::Acc(v) => Ok(v),
            Next::Invoke(func, args, kw, cont) => {
                self.request = Some(Request::Call {
                    func,
                    args,
                    kw,
                    cont,
                    outer: Cont::None,
                });
                Ok(Value::UNDEFINED)
            }
            Next::Throw(gn, exc, cont) => {
                self.request = Some(Request::Throw { gn, exc, cont });
                Ok(Value::UNDEFINED)
            }
            _ => Err(self.err(T::SystemError, "micro-monty: bad continuation")),
        }
    }

    /// Calls dunder `name` of user object `v` as a builtin or a VM
    /// operation (the request marker), or `None` if it does not define it.
    pub(crate) fn dunder_call(
        &mut self,
        v: Value,
        name: Dn,
        args: &[Value],
        cont: Cont,
    ) -> Result<Option<Value>, Raised> {
        if self.user_class(v).is_none() && self.meta_of(v).is_none() {
            return Ok(None);
        }
        self.dunder_request(v, name, args, cont)
    }

    /// Wraps the pending call request so that `outer` finishes it.
    pub(crate) fn wrap_request(&mut self, outer: Cont) {
        if let Some(Request::Call { cont, outer: o, .. }) = self.request.as_mut() {
            if *cont == Cont::None {
                *cont = outer;
            } else {
                *o = outer;
            }
        }
    }

    /// Calls dunder `name` of user object `v` as a VM operation (the
    /// request marker), or `None` if the class does not define it.
    fn dunder_request(
        &mut self,
        v: Value,
        name: Dn,
        args: &[Value],
        cont: Cont,
    ) -> Result<Option<Value>, Raised> {
        // A present `None` is called like CPython's slot does it ("'NoneType'
        // object is not callable").
        let Some(m) = self.dunder(v, name) else {
            return Ok(None);
        };
        let n = self.call_dunder(m, v, args, cont)?;
        self.next_request(n).map(Some)
    }

    // ------------------------------------------------------ operators

    fn binop_cands(&mut self, opb: u8, a: Value, b: Value) -> Vec<(Value, Value, Value)> {
        let names = BINOP_DUNDERS
            .get((opb & 0x7f) as usize)
            .copied()
            .unwrap_or(BINOP_DUNDERS[0]);
        let mut out = Vec::new();
        if opb & 0x80 != 0
            && let Some(m) = self.dunder(a, names[2])
        {
            out.push((m, a, b));
        }
        // A list subclass's `+=` runs the `__iadd__` it inherits (CPython
        // fills its in-place slot from it), which raises rather than
        // declines: neither its own `__add__` nor the right operand's
        // `__radd__` runs (an exact list has no such slot, so `__radd__`
        // does).
        if opb == BinOp::Add as u8 | 0x80
            && out.is_empty()
            && a.is_obj_kind(ty::INST as u8)
            && self.unbx(a).is_obj_kind(ty::LIST as u8)
        {
            return out;
        }
        let fa = self.dunder(a, names[0]);
        let (ca, cb) = (self.class_of(a), self.class_of(b));
        // `str`/`bytes` `%` never declines, so the right operand's `__rmod__`
        // runs first only when its class is a subclass of the left's.
        let percent = fa.is_none()
            && BinOp::ALL.get((opb & 0x7f) as usize) == Some(&BinOp::Mod)
            && matches!(self.type_of(self.unbx(a)), T::str | T::bytes);
        let fb = if ca == cb || percent && !self.subclass_of(cb, ca) {
            None
        } else {
            self.dunder(b, names[1])
        };
        match (fa, fb) {
            (fa, Some(fb)) if self.subclass_of(cb, ca) && Some(fb) != fa => {
                out.push((fb, b, a));
                if let Some(fa) = fa {
                    out.push((fa, a, b));
                }
            }
            (fa, fb) => {
                if let Some(fa) = fa {
                    out.push((fa, a, b));
                }
                if let Some(fb) = fb {
                    out.push((fb, b, a));
                }
            }
        }
        out
    }

    /// Stage `stage` of a binary operator on user objects (`opb`: the
    /// `BinOp` index, bit 7 for the in-place form).
    pub(crate) fn binop_next(
        &mut self,
        opb: u8,
        a: Value,
        b: Value,
        stage: u8,
    ) -> Result<Next, Raised> {
        let cands = self.binop_cands(opb, a, b);
        if let Some(&(m, s, o)) = cands.get(stage as usize) {
            return self.call_dunder(
                m,
                s,
                &[o],
                Cont::Bin {
                    op: opb,
                    stage,
                    a,
                    b,
                },
            );
        }
        let bop = BinOp::ALL
            .get((opb & 0x7f) as usize)
            .copied()
            .unwrap_or(BinOp::Add);
        let (ua, ub) = (self.unbx(a), self.unbx(b));
        if ua != a || ub != b {
            // An in-place dunder that declined (`NotImplemented`): as in
            // CPython's `binary_iop1`, the plain operator runs, which
            // changes neither operand.
            let inplace = opb & 0x80 != 0
                && BINOP_DUNDERS
                    .get((opb & 0x7f) as usize)
                    .is_none_or(|n| self.dunder(a, n[2]).is_none());
            // Builtin-type subclasses: the operation of the values they
            // extend.
            return match self.binop_bx(bop, a, b, inplace) {
                Ok(v) => Ok(Next::Acc(v)),
                Err(Raised) if self.slow => {
                    self.slow = false;
                    // `%` formats its right operand as given. An in-place repeat of a list subclass's instance by
                    // an `__index__` count: the twin repeats its backing
                    // list in place and returns the instance itself. A
                    // sequence subclass's instance repeated by one on the
                    // left goes as itself: only an exact sequence refuses.
                    let keep = inplace && bop == BinOp::Mul && ua.is_obj_kind(ty::LIST as u8);
                    let args = alloc::vec![
                        if keep { a } else { ua },
                        if bop == BinOp::Mul || bop == BinOp::Mod {
                            b
                        } else {
                            ub
                        },
                        Value::int(i64::from(opb & 0x7f)).unwrap_or(Value::NONE),
                        Value::from_bool(inplace),
                    ];
                    let f = self.prelude_fn("_op_binop")?;
                    let (f, args, _) = self.slow_call(f, args, Value::NONE)?;
                    Ok(Next::Invoke(f, args.into(), Value::NONE, Cont::None))
                }
                Err(e) => Err(e),
            };
        }
        // The sequence's own concatenation or repetition reports last.
        let seq = |t: T| matches!(t, T::str | T::list | T::tuple);
        let (ta, tb) = (self.type_of(a), self.type_of(b));
        if bop == BinOp::Add && seq(ta) {
            let msg = format!(
                "can only concatenate {} (not \"{}\") to {}",
                ta.name(),
                self.type_name(b),
                ta.name()
            );
            return Err(self.err(T::TypeError, &msg));
        }
        // Repetition takes bytes and bytearray too; an in-place one only
        // the left operand's (`n *= seq` is unsupported, as in CPython).
        let rep = |t: T| seq(t) || matches!(t, T::bytes | T::bytearray);
        if bop == BinOp::Mul && (rep(ta) || (rep(tb) && opb & 0x80 == 0)) {
            let n = if rep(ta) { b } else { a };
            if self.uflags(n) & f::INDEX != 0 {
                let other = if rep(ta) { a } else { b };
                return Ok(Next::Invoke(
                    self.prelude_fn("_op_binop")?,
                    Args::of(&[
                        other,
                        n,
                        Value::int(2).unwrap_or(Value::NONE),
                        Value::from_bool(opb & 0x80 != 0),
                    ]),
                    Value::NONE,
                    Cont::None,
                ));
            }
            let msg = format!(
                "can't multiply sequence by non-int of type '{}'",
                self.type_name(n)
            );
            return Err(self.err(T::TypeError, &msg));
        }
        if bop == BinOp::Mod && matches!(ta, T::str | T::bytes) {
            return Ok(Next::Invoke(
                self.prelude_fn("_percent")?,
                Args::of(&[a, b]),
                Value::NONE,
                Cont::None,
            ));
        }
        let sym = match (opb & 0x80 != 0, bop) {
            (false, BinOp::Pow) => String::from("** or pow()"),
            (true, _) => format!("{}=", bop.symbol()),
            _ => bop.symbol().into(),
        };
        let msg = format!(
            "unsupported operand type(s) for {sym}: '{}' and '{}'",
            String::from(self.type_name(a)),
            self.type_name(b)
        );
        Err(self.err(T::TypeError, &msg))
    }

    /// `a <op> b` as the dispatch loop runs it.
    pub(crate) fn op_binop(
        &mut self,
        bop: BinOp,
        a: Value,
        b: Value,
        inplace: bool,
    ) -> Result<Value, Raised> {
        if bop == BinOp::Add && Vm::is_str(a) && Vm::is_str(b) {
            if self.stop_before() {
                return Err(Raised);
            }
            let r = self.str_concat(a, b, crate::strmeth::FILL_BYTES);
            if r.is_err() {
                self.oom_retry_op(a, inplace);
            }
            return r;
        }
        if (self.uflags(a) | self.uflags(b)) & f::ARITH != 0 {
            let opb = bop as u8 | if inplace { 0x80 } else { 0 };
            let n = self.binop_next(opb, a, b, 0)?;
            return self.next_request(n);
        }
        if self.fuel.is_some()
            && matches!(
                bop,
                BinOp::Mul | BinOp::Pow | BinOp::Div | BinOp::FloorDiv | BinOp::Mod
            )
            && self.fuel_short(bop, a, b)
        {
            return Err(Raised);
        }
        // After `fuel_short`, whose park says what the operator wants.
        if self.stop_before() {
            return Err(Raised);
        }
        let r = self.binop_top(bop, a, b, inplace);
        if r.is_err() {
            self.oom_retry_op(a, inplace);
        }
        r
    }

    /// Unary `-`, `+`, `~` as the dispatch loop runs them.
    pub(crate) fn op_unary(&mut self, opcode: u8, v: Value) -> Result<Value, Raised> {
        if self.uflags(v) & f::UNARY != 0 {
            let name = match opcode {
                op::NEG => Dn::Neg,
                op::POS => Dn::Pos,
                _ => Dn::Invert,
            };
            if let Some(r) = self.dunder_request(v, name, &[], Cont::None)? {
                return Ok(r);
            }
        }
        self.unary(opcode, v)
    }

    fn cmp_names(opcode: u8) -> (Dn, Dn, &'static str) {
        match opcode {
            op::TEST_LT => (Dn::Lt, Dn::Gt, "<"),
            op::TEST_LE => (Dn::Le, Dn::Ge, "<="),
            op::TEST_GT => (Dn::Gt, Dn::Lt, ">"),
            op::TEST_GE => (Dn::Ge, Dn::Le, ">="),
            op::TEST_EQ => (Dn::Eq, Dn::Eq, "=="),
            _ => (Dn::Ne, Dn::Ne, "!="),
        }
    }

    /// The method for comparison `name` of `v`; `!=` falls back to an
    /// inverted `__eq__` (object's `__ne__`).
    fn cmp_method(&mut self, v: Value, name: Dn) -> Option<(Value, bool)> {
        if let Some(m) = self.dunder(v, name) {
            return Some((m, false));
        }
        if name == Dn::Ne
            && let Some(m) = self.dunder(v, Dn::Eq)
        {
            return Some((m, true));
        }
        None
    }

    fn cmp_cands(&mut self, opcode: u8, a: Value, b: Value) -> Vec<(Value, bool, Value, Value)> {
        let (name, rname, _) = Vm::cmp_names(opcode);
        let fa = self.cmp_method(a, name);
        let (ca, cb) = (self.class_of(a), self.class_of(b));
        // Unlike binary operators, the reflected method is tried even for
        // operands of the same type.
        let fb = self.cmp_method(b, rname);
        let mut out = Vec::new();
        let b_first = fb.is_some() && ca != cb && self.subclass_of(cb, ca);
        if b_first && let Some((m, inv)) = fb {
            out.push((m, inv, b, a));
        }
        if let Some((m, inv)) = fa {
            out.push((m, inv, a, b));
        }
        if !b_first && let Some((m, inv)) = fb {
            out.push((m, inv, b, a));
        }
        out
    }

    /// Stage `stage` of rich comparison `opcode` on user objects.
    pub(crate) fn cmp_next(
        &mut self,
        opcode: u8,
        a: Value,
        b: Value,
        stage: u8,
    ) -> Result<Next, Raised> {
        // Stage 0 without a subclass right operand is `a`'s own method,
        // when it has one: no need to look up the reflected one yet.
        if stage == 0 {
            let (ca, cb) = (self.class_of(a), self.class_of(b));
            if (ca == cb || !self.subclass_of(cb, ca))
                && let Some((m, inv)) = self.cmp_method(a, Vm::cmp_names(opcode).0)
            {
                let cont = Cont::Cmp {
                    op: opcode,
                    stage: if inv { 0x80 } else { 0 },
                    a,
                    b,
                };
                return self.call_dunder(m, a, &[b], cont);
            }
        }
        let cands = self.cmp_cands(opcode, a, b);
        if let Some(&(m, inv, s, o)) = cands.get(stage as usize) {
            let st = stage | if inv { 0x80 } else { 0 };
            return self.call_dunder(
                m,
                s,
                &[o],
                Cont::Cmp {
                    op: opcode,
                    stage: st,
                    a,
                    b,
                },
            );
        }
        let (ua, ub) = (self.unbx(a), self.unbx(b));
        if ua != a || ub != b {
            // Builtin-type subclasses compare as the values they extend
            // (errors name the subclasses).
            let mut left = crate::dict::WALK;
            // A set, or a dict view with a set, orders as sets do.
            let setlike = |t: T| matches!(t, T::set | T::frozenset | T::dict_keys | T::dict_items);
            let sets = Vm::is_set(ua) || setlike(self.type_of(ua)) && setlike(self.type_of(ub));
            let eq = matches!(opcode, op::TEST_EQ | op::TEST_NE);
            if !(eq || sets)
                && let Err(crate::ops::OrdErr::Types(p, q)) = self.order_pure(ua, ub, 0, &mut left)
                && p == ua
                && q == ub
            {
                let (_, _, sym) = Vm::cmp_names(opcode);
                let e = crate::ops::OrdErr::Types(a, b);
                return Err(self.order_error(e, sym));
            }
            return match self.compare(opcode, ua, ub) {
                Ok(v) => Ok(Next::Acc(v)),
                Err(Raised) if self.slow => {
                    self.slow = false;
                    let (f, args) = match opcode {
                        op::TEST_EQ => ("_op_eq", vec![ua, ub]),
                        op::TEST_NE => ("_op_ne", vec![ua, ub]),
                        _ => (
                            "_op_cmp",
                            vec![ua, ub, Value::int(i64::from(opcode)).unwrap_or(Value::NONE)],
                        ),
                    };
                    let f = self.prelude_fn(f)?;
                    let (f, args, _) = self.slow_call(f, args, Value::NONE)?;
                    Ok(Next::Invoke(f, args.into(), Value::NONE, Cont::None))
                }
                Err(e) => Err(e),
            };
        }
        match opcode {
            op::TEST_EQ => Ok(Next::Acc(Value::from_bool(a == b))),
            op::TEST_NE => Ok(Next::Acc(Value::from_bool(a != b))),
            _ => {
                let (_, _, sym) = Vm::cmp_names(opcode);
                let msg = format!(
                    "'{sym}' not supported between instances of '{}' and '{}'",
                    String::from(self.type_name(a)),
                    self.type_name(b)
                );
                Err(self.err(T::TypeError, &msg))
            }
        }
    }

    /// A comparison opcode as the dispatch loop runs it.
    pub(crate) fn op_compare(&mut self, opcode: u8, a: Value, b: Value) -> Result<Value, Raised> {
        if self.stop_before() {
            return Err(Raised);
        }
        match opcode {
            op::TEST_LT | op::TEST_LE | op::TEST_GT | op::TEST_GE => {
                if (self.uflags(a) | self.uflags(b)) & f::ORDER != 0 {
                    let n = self.cmp_next(opcode, a, b, 0)?;
                    return self.next_request(n);
                }
            }
            op::TEST_EQ | op::TEST_NE => {
                if (self.uflags(a) | self.uflags(b)) & (f::EQ | f::NE) != 0 {
                    let n = self.cmp_next(opcode, a, b, 0)?;
                    return self.next_request(n);
                }
            }
            op::TEST_IN | op::TEST_NOT_IN => {
                let neg = opcode == op::TEST_NOT_IN;
                if self.user_class(b).is_some() || self.meta_of(b).is_some() {
                    let fl = self.uflags(b);
                    if fl & f::CONTAINS != 0 && self.user_method(b, Dn::Contains).is_none() {
                        // `__contains__ = None` opts out (no iteration).
                        let msg = format!("'{}' object is not a container", self.type_name(b));
                        return Err(self.err(T::TypeError, &msg));
                    }
                    if fl & f::CONTAINS != 0
                        && let Some(r) =
                            self.dunder_request(b, Dn::Contains, &[a], Cont::Contains(neg))?
                    {
                        return Ok(r);
                    }
                    if fl & (f::ITER | f::GETITEM) != 0 && self.container_backing(b).is_none() {
                        let name = if neg {
                            "_op_not_contains"
                        } else {
                            "_op_contains"
                        };
                        return self.prelude_request(name, vec![b, a], Cont::None);
                    }
                } else if b.is_obj_kind(ty::LIST as u8) || b.is_obj_kind(ty::TUPLE as u8) {
                    // A step of bounded work, then more in `_steps`.
                    let mut i = 0;
                    if let Some(v) = self.seq_in_scan(b, a, &mut i, neg)? {
                        return Ok(v);
                    }
                    return self.chunk_rest(
                        crate::nstep::op::SEQ_IN,
                        &[b, a],
                        &[i as u64, u64::from(neg)],
                    );
                } else if Vm::is_bytes_like(b) {
                    // A step of bounded work, then more in `_steps`.
                    return self.bytes_in(b, a, neg);
                } else if Vm::is_str(b)
                    && crate::strmeth::str_in_long(self, b)
                    && let Some(r) = crate::strmeth::str_in(self, b, a, neg)
                {
                    // A long text: searched a step at a time.
                    return r;
                } else if b.is_obj_kind(ty::VIEW as u8) && self.type_of(b) == T::dict_values {
                    // A step of bounded work, then more in `_steps`.
                    return self.dv_in(b, a, neg);
                }
            }
            _ => {}
        }
        self.compare(opcode, a, b)
    }

    /// The truth test of user object `v` for `mode` (see `cont::truth`);
    /// `None` if it has neither `__bool__` nor `__len__`.
    #[cold]
    pub(crate) fn op_truth(
        &mut self,
        v: Value,
        mode: u8,
        target: u32,
    ) -> Result<Option<Value>, Raised> {
        if v == Value::NOT_IMPLEMENTED {
            return Err(self.err(
                T::TypeError,
                "NotImplemented should not be used in a boolean context",
            ));
        }
        let fl = self.uflags(v);
        if fl & f::BOOL != 0
            && let Some(r) = self.dunder_request(
                v,
                Dn::Bool,
                &[],
                Cont::Truth {
                    mode,
                    target,
                    len: false,
                    orig: v,
                },
            )?
        {
            return Ok(Some(r));
        }
        if fl & f::LEN != 0
            && let Some(r) = self.dunder_request(
                v,
                Dn::Len,
                &[],
                Cont::Truth {
                    mode,
                    target,
                    len: true,
                    orig: v,
                },
            )?
        {
            return Ok(Some(r));
        }
        Ok(None)
    }

    /// `bool(v)` that needs no Python code, if it can be decided so.
    pub(crate) fn truthy_fast(&self, v: Value) -> Option<bool> {
        if self.uflags(v) & (f::BOOL | f::LEN) != 0 || v == Value::NOT_IMPLEMENTED {
            return None;
        }
        Some(self.truthy(v))
    }

    // ----------------------------------------------------- subscripts

    /// `o[k]` as the dispatch loop runs it.
    pub(crate) fn op_getitem(&mut self, o: Value, k: Value) -> Result<Value, Raised> {
        if self.user_class(o).is_some() {
            if let Some(r) = self.dunder_request(o, Dn::Getitem, &[k], Cont::None)? {
                return Ok(r);
            }
            if self.backing(o).is_some() {
                return self.getitem(o, k);
            }
            let msg = format!("'{}' object is not subscriptable", self.type_name(o));
            return Err(self.err(T::TypeError, &msg));
        }
        if Vm::is_class(o) {
            if self.meta_of(o).is_some()
                && let Some(r) = self.dunder_request(o, Dn::Getitem, &[k], Cont::None)?
            {
                return Ok(r);
            }
            let key = self.dn(Dn::ClassGetitem)?;
            if let Some(m) = self.class_lookup(o, key) {
                let m = if m.is_obj_kind(ty::CLASSMETHOD as u8) {
                    self.vslot(m, 0)
                } else {
                    m
                };
                let n = Next::Invoke(m, Args::of(&[o, k]), Value::NONE, Cont::None);
                return self.next_request(n);
            }
            let msg = format!("type '{}' is not subscriptable", self.class_tpname(o));
            return Err(self.err(T::TypeError, &msg));
        }
        if self.type_of_typeobj(o).is_some_and(generic_type) {
            // `list[int]`: a `types.GenericAlias`.
            return self.prelude_request("_generic_alias", vec![o, k], Cont::None);
        }
        self.getitem(o, k)
    }

    /// `o[k] = v` as the dispatch loop runs it (a request is left in
    /// `self.request`).
    pub(crate) fn op_setitem(&mut self, o: Value, k: Value, v: Value) -> Result<(), Raised> {
        let r = self.op_setitem_in(o, k, v);
        if r.is_err() {
            // Stores are idempotent but a list slice's, which builds its
            // new buffer before it installs it.
            self.oom_retry_op(Value::NONE, false);
        }
        r
    }

    fn op_setitem_in(&mut self, o: Value, k: Value, v: Value) -> Result<(), Raised> {
        if self.user_class(o).is_some() {
            if self
                .dunder_request(o, Dn::Setitem, &[k, v], Cont::Discard)?
                .is_some()
            {
                return Ok(());
            }
            if self.backing(o).is_some() {
                return self.setitem(o, k, v);
            }
            let msg = format!(
                "'{}' object does not support item assignment",
                self.type_name(o)
            );
            return Err(self.err(T::TypeError, &msg));
        }
        self.setitem(o, k, v)
    }

    /// `del o[k]` as the dispatch loop runs it.
    pub(crate) fn op_delitem(&mut self, o: Value, k: Value) -> Result<(), Raised> {
        if self.user_class(o).is_some() {
            if self
                .dunder_request(o, Dn::Delitem, &[k], Cont::Discard)?
                .is_some()
            {
                return Ok(());
            }
            if self.backing(o).is_some() {
                return self.delitem(o, k);
            }
            let msg = format!(
                "'{}' object doesn't support item deletion",
                self.type_name(o)
            );
            return Err(self.err(T::TypeError, &msg));
        }
        self.delitem(o, k)
    }

    // ------------------------------------------------------ iteration

    /// `iter(v)` as the dispatch loop runs it.
    pub(crate) fn op_get_iter(&mut self, v: Value) -> Result<Value, Raised> {
        if self.user_class(v).is_some() || self.meta_of(v).is_some() {
            if self.dunder(v, Dn::Iter) == Some(Value::NONE) {
                // `__iter__ = None` opts out of iteration.
                let msg = format!("'{}' object is not iterable", self.type_name(v));
                return Err(self.err(T::TypeError, &msg));
            }
            if let Some(r) = self.dunder_request(v, Dn::Iter, &[], Cont::Check(check::ITER))? {
                return Ok(r);
            }
            if self.uflags(v) & f::GETITEM != 0 && self.container_backing(v).is_none() {
                return self.prelude_request("_seq_iter", vec![v], Cont::None);
            }
        }
        self.get_iter(v)
    }

    /// `next(it)` of a user iterator, finishing through `cont`.
    pub(crate) fn op_user_next(&mut self, it: Value, cont: Cont) -> Result<Value, Raised> {
        if let Some(r) = self.dunder_request(it, Dn::Next, &[], cont)? {
            return Ok(r);
        }
        let msg = format!("'{}' object is not an iterator", self.type_name(it));
        Err(self.err(T::TypeError, &msg))
    }

    /// Is `v` an iterator whose `next` runs no Python code?
    pub(crate) fn is_native_iter(v: Value) -> bool {
        matches!(
            v.obj_kind().map(u16::from),
            Some(ty::ITER | ty::ENUMERATE | ty::ZIP | ty::REVERSED)
        )
    }

    // ------------------------------------------------------------ with

    /// `WITH_ENTER`: the bound `__exit__` and the `__enter__()` call (a
    /// request).
    #[cold]
    pub(crate) fn op_with_enter(
        &mut self,
        ctx: Value,
        is_async: bool,
    ) -> Result<(Value, Value), Raised> {
        let c = self.class_of(ctx);
        let names = [Dn::Exit, Dn::Enter, Dn::Aexit, Dn::Aenter];
        let [exit, enter, aexit, aenter] =
            names.map(|d| self.dn(d).ok().and_then(|k| self.class_lookup(c, k)));
        let (exit, enter, other) = if is_async {
            (aexit, aenter, exit.is_some() && enter.is_some())
        } else {
            (exit, enter, aexit.is_some() && aenter.is_some())
        };
        // CPython's messages, with its hint when the other protocol is there.
        let missed = |vm: &mut Vm, what: &str| {
            let (proto, hint) = if is_async {
                (
                    "asynchronous context manager",
                    "context manager protocol. Did you mean to use 'with'",
                )
            } else {
                (
                    "context manager",
                    "asynchronous context manager protocol. Did you mean to use 'async with'",
                )
            };
            let tn = vm.type_full_name(ctx);
            let msg = if other {
                format!(
                    "'{tn}' object does not support the {proto} protocol (missed {what} method) but it supports the {hint}?"
                )
            } else {
                format!(
                    "'{tn}' object does not support the {proto} protocol (missed {what} method)"
                )
            };
            vm.err(T::TypeError, &msg)
        };
        let ex = if is_async { "__aexit__" } else { "__exit__" };
        let Some(exit) = exit.filter(|_| Vm::is_class(c)) else {
            return Err(missed(self, ex));
        };
        let Some(enter) = enter else {
            return Err(missed(
                self,
                if is_async { "__aenter__" } else { "__enter__" },
            ));
        };
        let bound = self.bind_attr(exit, ctx, c)?;
        let cont = if is_async {
            Cont::Check(check::AENTER)
        } else {
            Cont::None
        };
        let n = self.call_dunder(enter, ctx, &[], cont)?;
        let r = self.next_request(n)?;
        Ok((bound, r))
    }

    /// `GET_AITER`: `aiter(it)` for `async for`.
    #[cold]
    pub(crate) fn op_get_aiter(&mut self, it: Value) -> Result<Value, Raised> {
        self.aiter_of(it, true)
    }

    /// `aiter(it)`; `for_loop` picks `async for`'s messages.
    pub(crate) fn aiter_of(&mut self, it: Value, for_loop: bool) -> Result<Value, Raised> {
        if it.is_obj_kind(ty::AGEN as u8) {
            return Ok(it);
        }
        let Some(m) = self.user_method(it, Dn::Aiter) else {
            let tn = String::from(self.type_name(it));
            let msg = if for_loop {
                format!("'async for' requires an object with __aiter__ method, got {tn}")
            } else {
                format!("'{tn}' object is not an async iterable")
            };
            return Err(self.err(T::TypeError, &msg));
        };
        let k = if for_loop {
            check::AITER_FOR
        } else {
            check::AITER
        };
        let n = self.call_dunder(m, it, &[], Cont::Check(k))?;
        self.next_request(n)
    }

    /// `ANEXT`: the awaitable `it.__anext__()` for `async for`.
    #[cold]
    pub(crate) fn op_anext(&mut self, it: Value) -> Result<Value, Raised> {
        if it.is_obj_kind(ty::AGEN as u8) {
            return self.new_agen_op(it, crate::generator::ag::SEND, Value::NONE);
        }
        let Some(m) = self.user_method(it, Dn::Anext) else {
            let msg = format!(
                "'async for' requires an iterator with __anext__ method, got {}",
                self.type_name(it)
            );
            return Err(self.err(T::TypeError, &msg));
        };
        let n = self.call_dunder(m, it, &[], Cont::Check(check::ANEXT))?;
        self.next_request(n)
    }

    /// `WITH_EXIT`: calls `exit` (a request); with an exception, the
    /// continuation re-raises it unless the result is true.
    #[cold]
    pub(crate) fn op_with_exit(
        &mut self,
        exit: Value,
        exc: Option<Value>,
        is_async: bool,
    ) -> Result<Value, Raised> {
        // `async with` awaits the result; the code after tests it.
        let aexit = Cont::Check(check::AEXIT);
        let n = match exc {
            None => Next::Invoke(
                exit,
                Args::of(&[Value::NONE, Value::NONE, Value::NONE]),
                Value::NONE,
                if is_async { aexit } else { Cont::Discard },
            ),
            Some(e) => {
                let t = self.class_of(e);
                let tb = self.traceback_of(e)?;
                let cont = if is_async { aexit } else { Cont::WithExit(e) };
                Next::Invoke(exit, Args::of(&[t, e, tb]), Value::NONE, cont)
            }
        };
        self.next_request(n)
    }

    // --------------------------------------------------------- truth

    /// The `cont::truth` mode a jump opcode tests with.
    pub(crate) fn truth_mode(opcode: u8) -> u8 {
        match opcode {
            op::JUMP_IF_TRUE => truth::IF_TRUE,
            op::JUMP_IF_FALSE => truth::IF_FALSE,
            op::NOT => truth::NOT,
            _ => truth::BOOL,
        }
    }
}

/// Builtin types with a `__class_getitem__` (`list[int]`).
pub(crate) fn generic_type(t: T) -> bool {
    matches!(
        t,
        T::list
            | T::dict
            | T::tuple
            | T::set
            | T::frozenset
            | T::type_
            | T::enumerate
            | T::classmethod
            | T::staticmethod
            | T::generator
            | T::coroutine
            | T::Match
    )
}
