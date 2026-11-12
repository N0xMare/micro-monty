//! The `MATCH` instruction (`match` statements, PEP 634): native steps for
//! builtin values; user objects (a `__len__`, a mapping's `get`, a class
//! pattern's `__match_args__` and attributes) continue in the prelude's
//! `_op_match`.

use crate::class::Dn;
use crate::cont::Cont;
use crate::types::{T, ty};
use crate::vm::{Raised, Vm};
use alloc::{format, vec::Vec};
use mm_compile::match_kind as kind;
use mm_core::Value;

/// `collections.abc` collection flags (`__abc_tpflags__`), as CPython's.
pub(crate) const TPFLAGS_SEQUENCE: i64 = 1 << 5;
pub(crate) const TPFLAGS_MAPPING: i64 = 1 << 6;

impl Vm {
    /// One `MATCH` step: `x` is the accumulator, `a`/`b` the registers it
    /// names, `n` the immediate.
    pub(crate) fn match_step(
        &mut self,
        k: u8,
        x: Value,
        a: Value,
        b: Value,
        n: usize,
    ) -> Result<Value, Raised> {
        match k {
            kind::SEQ => Ok(Value::from_bool(
                self.collection_flags(x) & TPFLAGS_SEQUENCE != 0,
            )),
            kind::MAP => Ok(Value::from_bool(
                self.collection_flags(x) & TPFLAGS_MAPPING != 0,
            )),
            kind::LEN => {
                if let Some(r) = self.dunder_call(x, Dn::Len, &[], Cont::Len)? {
                    return Ok(r);
                }
                let n = self.len_of(x)?;
                self.new_int(i64::try_from(n).unwrap_or(i64::MAX))
            }
            kind::KEYS => self.match_keys(a, x),
            kind::REST => {
                if !a.is_obj_kind(ty::DICT as u8) {
                    return Err(self.need_py());
                }
                let d = self.dict_copy(a)?;
                for key in self.tuple_items(x) {
                    if self.dict_del(d, key)?.is_none() {
                        let r = self.repr_whole(key)?;
                        return Err(self.err(T::KeyError, &r));
                    }
                }
                Ok(d)
            }
            kind::CLASS => self.match_class(x, a, b, n),
            _ => Err(self.err(T::SystemError, "micro-monty: bad MATCH kind")),
        }
    }

    /// `Py_TPFLAGS_SEQUENCE | Py_TPFLAGS_MAPPING` of `v`'s type.
    pub(crate) fn collection_flags(&mut self, v: Value) -> i64 {
        let own = |t: T| match t {
            T::list | T::tuple | T::range => TPFLAGS_SEQUENCE,
            T::dict => TPFLAGS_MAPPING,
            _ => 0,
        };
        let Some(c) = self.user_class(v) else {
            return own(self.type_of(v));
        };
        let base = own(self.class_base_t(c));
        if base != 0 {
            return base;
        }
        let Ok(name) = self.intern("__abc_tpflags__") else {
            return 0;
        };
        match self.user_lookup(c, name) {
            Some(fl) => fl.as_int().unwrap_or(0) & (TPFLAGS_SEQUENCE | TPFLAGS_MAPPING),
            None => 0,
        }
    }

    /// The values of `keys` (a tuple) in mapping `m`, or `None` if one is
    /// missing.
    fn match_keys(&mut self, m: Value, keys: Value) -> Result<Value, Raised> {
        if !m.is_obj_kind(ty::DICT as u8) {
            return Err(self.need_py());
        }
        let keys = self.tuple_items(keys);
        let mut vals = Vec::with_capacity(keys.len());
        for (i, &key) in keys.iter().enumerate() {
            self.hash_as(key, "set element")?;
            for &prev in &keys[..i] {
                if self.eq_values(prev, key)? {
                    let r = self.repr_whole(key)?;
                    let msg = format!("mapping pattern checks duplicate key ({r})");
                    return Err(self.err(T::ValueError, &msg));
                }
            }
            match self.dict_get(m, key)? {
                Some(v) => vals.push(v),
                None => return Ok(Value::NONE),
            }
        }
        self.alloc_tuple(&vals)
    }

    /// A class pattern's attributes: `None` if `x` is not an instance of
    /// `c`; Python code decides anything past builtin self-matching.
    fn match_class(
        &mut self,
        x: Value,
        c: Value,
        kw: Value,
        nargs: usize,
    ) -> Result<Value, Raised> {
        if !Vm::is_type(c) || self.has_instancecheck(c) {
            return Err(self.need_py());
        }
        if !self.instance_of(x, c) {
            return Ok(Value::NONE);
        }
        let nkw = self.tuple_len(kw);
        if nargs == 0 && nkw == 0 {
            return self.alloc_tuple(&[]);
        }
        let self_matching = matches!(
            self.type_of_typeobj(c),
            Some(
                T::bool
                    | T::bytes
                    | T::bytearray
                    | T::dict
                    | T::float
                    | T::int
                    | T::list
                    | T::set
                    | T::frozenset
                    | T::str
                    | T::tuple
            )
        );
        if self_matching && nargs == 1 && nkw == 0 {
            return self.alloc_tuple(&[x]);
        }
        Err(self.need_py())
    }

    /// Does class `c`'s metaclass define `__instancecheck__`?
    pub(crate) fn has_instancecheck(&mut self, c: Value) -> bool {
        self.meta_of(c).is_some() && self.user_method(c, Dn::Instancecheck).is_some()
    }
}
