//! User classes and instances (docs/spec.md §7.2, §8.1).
//!
//! A class holds its name, bases, cached C3 MRO, namespace dict, a version
//! tag and a bitset of the dunders it (or a base) defines in Python. Every
//! change to a class's namespace gives it and its subclasses new version
//! tags, which invalidates the attribute and method inline caches keyed by
//! them.
//!
//! An instance holds its class, a shape id (a hidden class: the ordered
//! attribute names, shared by every instance that gained the same
//! attributes in the same order), inline slots and an overflow buffer. A
//! deletion or a `__dict__` access switches it to a plain attribute dict.
//! Exceptions keep their own layout (`ty::EXC`) with an attribute dict.
//!
//! Attribute access that needs Python code (a property getter,
//! `__getattr__`) does not run it here: it leaves a call request
//! (`Request::Call`) and returns `UNDEFINED`, and the dispatch loop pushes
//! the frame.

use crate::cont::Cont;
use crate::types::{T, ty};
use crate::vm::{Raised, Request, Vm, exc_slot, fn_slot};
use alloc::{format, string::String, vec, vec::Vec};
use mm_core::Value;

/// Class slots: values, then raw words.
pub(crate) mod cls {
    pub const NAME: u32 = 0;
    pub const QUALNAME: u32 = 1;
    pub const BASES: u32 = 2;
    pub const MRO: u32 = 3;
    pub const DICT: u32 = 4;
    /// A list of the direct subclasses (for version invalidation).
    pub const SUBS: u32 = 5;
    /// The metaclass when not `type`, else `None`.
    pub const META: u32 = 6;
    pub const VERSION: u32 = 7;
    pub const FLAGS: u32 = 8;
    /// The builtin type instances are laid out as (`object` or an
    /// exception type), as a `T` index.
    pub const BASE: u32 = 9;
    /// Inline slots new instances get.
    pub const HINT: u32 = 10;
}

/// Instance slots.
pub(crate) mod inst {
    pub const CLASS: u32 = 0;
    /// A `SmallInt` shape id, or the attribute dict.
    pub const SHAPE: u32 = 1;
    /// A `BUF` of attributes past the inline slots, or `None`.
    pub const OVER: u32 = 2;
    /// For an instance of a subclass of a builtin type (`tuple`, `dict`,
    /// ...): the builtin value it extends; else `None`.
    pub const BACK: u32 = 3;
    pub const FIRST: u32 = 4;
}

/// Dunder flags: the class or a base defines the method in Python.
pub(crate) mod f {
    pub const EQ: u64 = 1;
    pub const HASH: u64 = 1 << 1;
    pub const ORDER: u64 = 1 << 2;
    pub const REPR: u64 = 1 << 3;
    pub const STR: u64 = 1 << 4;
    pub const BOOL: u64 = 1 << 5;
    pub const LEN: u64 = 1 << 6;
    pub const ITER: u64 = 1 << 7;
    pub const NEXT: u64 = 1 << 8;
    pub const CONTAINS: u64 = 1 << 9;
    pub const GETITEM: u64 = 1 << 10;
    pub const SETITEM: u64 = 1 << 11;
    pub const DELITEM: u64 = 1 << 12;
    pub const CALL: u64 = 1 << 13;
    pub const GETATTR: u64 = 1 << 14;
    pub const SETATTR: u64 = 1 << 15;
    pub const DELATTR: u64 = 1 << 16;
    pub const ARITH: u64 = 1 << 17;
    pub const UNARY: u64 = 1 << 18;
    pub const INDEX: u64 = 1 << 19;
    pub const INT: u64 = 1 << 20;
    pub const FLOAT: u64 = 1 << 21;
    pub const FORMAT: u64 = 1 << 22;
    pub const INIT: u64 = 1 << 23;
    pub const NEW: u64 = 1 << 24;
    pub const SET_NAME: u64 = 1 << 25;
    pub const INIT_SUBCLASS: u64 = 1 << 26;
    pub const NE: u64 = 1 << 27;
    pub const ENTER: u64 = 1 << 28;
    pub const EXIT: u64 = 1 << 29;
    pub const DESCR: u64 = 1 << 30;
    /// The class names itself in messages as a builtin type would
    /// (`__mm_tpname__`, e.g. `collections.deque`).
    pub const TPNAME: u64 = 1 << 31;
    /// The class extends `int`, whose value its instances' natives read:
    /// such a class never has [`INDEX`], as CPython's `PyNumber_Index`
    /// takes an int subclass's value without calling its `__index__`.
    pub const INT_BASE: u64 = 1 << 32;
}

/// Declares [`Dn`] and its names.
macro_rules! dunders {
    ($($id:ident = $name:literal,)*) => {
        /// A dunder name the engine looks up itself; the session interns
        /// each once ([`Vm::dn`]).
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #[repr(u8)]
        pub(crate) enum Dn {
            $($id,)*
        }
        /// The text of each [`Dn`].
        pub(crate) const DUNDER_NAMES: &[&str] = &[$($name,)*];
    };
}

dunders! {
    Abs = "__abs__",
    Add = "__add__",
    Aenter = "__aenter__",
    Aexit = "__aexit__",
    Aiter = "__aiter__",
    Anext = "__anext__",
    And = "__and__",
    Bool = "__bool__",
    Bytes = "__bytes__",
    Call = "__call__",
    Ceil = "__ceil__",
    ClassGetitem = "__class_getitem__",
    Complex = "__complex__",
    Contains = "__contains__",
    Delattr = "__delattr__",
    Delete = "__delete__",
    Delitem = "__delitem__",
    Enter = "__enter__",
    Eq = "__eq__",
    Exit = "__exit__",
    Float = "__float__",
    Floor = "__floor__",
    Floordiv = "__floordiv__",
    Format = "__format__",
    Ge = "__ge__",
    Get = "__get__",
    Getattr = "__getattr__",
    Getitem = "__getitem__",
    Gt = "__gt__",
    Hash = "__hash__",
    Iadd = "__iadd__",
    Iand = "__iand__",
    Ifloordiv = "__ifloordiv__",
    Ilshift = "__ilshift__",
    Imatmul = "__imatmul__",
    Imod = "__imod__",
    Imul = "__imul__",
    Index = "__index__",
    Init = "__init__",
    InitSubclass = "__init_subclass__",
    Instancecheck = "__instancecheck__",
    Int = "__int__",
    Invert = "__invert__",
    Ior = "__ior__",
    Ipow = "__ipow__",
    Irshift = "__irshift__",
    Isub = "__isub__",
    Iter = "__iter__",
    Itruediv = "__itruediv__",
    Ixor = "__ixor__",
    Le = "__le__",
    Len = "__len__",
    Lshift = "__lshift__",
    Lt = "__lt__",
    Matmul = "__matmul__",
    Missing = "__missing__",
    MmTpname = "__mm_tpname__",
    Mod = "__mod__",
    Mul = "__mul__",
    Ne = "__ne__",
    Neg = "__neg__",
    New = "__new__",
    Next = "__next__",
    Or = "__or__",
    Pos = "__pos__",
    Pow = "__pow__",
    Radd = "__radd__",
    Rand = "__rand__",
    Repr = "__repr__",
    Reversed = "__reversed__",
    Rfloordiv = "__rfloordiv__",
    Rlshift = "__rlshift__",
    Rmatmul = "__rmatmul__",
    Rmod = "__rmod__",
    Rmul = "__rmul__",
    Ror = "__ror__",
    Round = "__round__",
    Rpow = "__rpow__",
    Rrshift = "__rrshift__",
    Rshift = "__rshift__",
    Rsub = "__rsub__",
    Rtruediv = "__rtruediv__",
    Rxor = "__rxor__",
    Set = "__set__",
    SetName = "__set_name__",
    Setattr = "__setattr__",
    Setitem = "__setitem__",
    Str = "__str__",
    Sub = "__sub__",
    Subclasscheck = "__subclasscheck__",
    Truediv = "__truediv__",
    Trunc = "__trunc__",
    Xor = "__xor__",
}

/// Number of [`Dn`] names.
pub(crate) const NDUNDERS: usize = DUNDER_NAMES.len();

/// The dunder each flag stands for.
const FLAG_NAMES: &[(Dn, u64)] = &[
    (Dn::Eq, f::EQ),
    (Dn::Ne, f::NE),
    (Dn::Hash, f::HASH),
    (Dn::Lt, f::ORDER),
    (Dn::Le, f::ORDER),
    (Dn::Gt, f::ORDER),
    (Dn::Ge, f::ORDER),
    (Dn::Repr, f::REPR),
    (Dn::Str, f::STR),
    (Dn::Bool, f::BOOL),
    (Dn::Len, f::LEN),
    (Dn::Iter, f::ITER),
    (Dn::Next, f::NEXT),
    (Dn::Contains, f::CONTAINS),
    (Dn::Getitem, f::GETITEM),
    (Dn::Setitem, f::SETITEM),
    (Dn::Delitem, f::DELITEM),
    (Dn::Call, f::CALL),
    (Dn::Getattr, f::GETATTR),
    (Dn::Setattr, f::SETATTR),
    (Dn::Delattr, f::DELATTR),
    (Dn::Neg, f::UNARY),
    (Dn::Pos, f::UNARY),
    (Dn::Invert, f::UNARY),
    (Dn::Abs, f::UNARY),
    (Dn::Index, f::INDEX),
    (Dn::Int, f::INT),
    (Dn::Float, f::FLOAT),
    (Dn::Format, f::FORMAT),
    (Dn::MmTpname, f::TPNAME),
    (Dn::Init, f::INIT),
    (Dn::New, f::NEW),
    (Dn::SetName, f::SET_NAME),
    (Dn::InitSubclass, f::INIT_SUBCLASS),
    (Dn::Enter, f::ENTER),
    (Dn::Exit, f::EXIT),
    (Dn::Get, f::DESCR),
    (Dn::Set, f::DESCR),
    (Dn::Delete, f::DESCR),
];

/// Forward, reflected and in-place dunders of each binary operator, by
/// `BinOp` index.
pub(crate) const BINOP_DUNDERS: [[Dn; 3]; 13] = [
    [Dn::Add, Dn::Radd, Dn::Iadd],
    [Dn::Sub, Dn::Rsub, Dn::Isub],
    [Dn::Mul, Dn::Rmul, Dn::Imul],
    [Dn::Truediv, Dn::Rtruediv, Dn::Itruediv],
    [Dn::Floordiv, Dn::Rfloordiv, Dn::Ifloordiv],
    [Dn::Mod, Dn::Rmod, Dn::Imod],
    [Dn::Pow, Dn::Rpow, Dn::Ipow],
    [Dn::Lshift, Dn::Rlshift, Dn::Ilshift],
    [Dn::Rshift, Dn::Rrshift, Dn::Irshift],
    [Dn::And, Dn::Rand, Dn::Iand],
    [Dn::Or, Dn::Ror, Dn::Ior],
    [Dn::Xor, Dn::Rxor, Dn::Ixor],
    [Dn::Matmul, Dn::Rmatmul, Dn::Imatmul],
];

/// The dunders [`Vm::compute_flags`] decides, by position: `FLAG_NAMES`,
/// then `BINOP_DUNDERS` (flag `ARITH`).
const NFLAGGED: usize = FLAG_NAMES.len() + 3 * BINOP_DUNDERS.len();

/// The dunder at position `j` below `NFLAGGED`.
const fn dn_at(j: usize) -> Dn {
    if j < FLAG_NAMES.len() {
        FLAG_NAMES[j].0
    } else {
        let k = j - FLAG_NAMES.len();
        BINOP_DUNDERS[k / 3][k % 3]
    }
}

/// `hash_bytes` of each of `NFLAGGED`'s dunders, by position: a `str`
/// caches its hash, so a class dict key is matched without reading text.
const FLAGGED_HASH: [u64; NFLAGGED] = {
    let mut t = [0; NFLAGGED];
    let mut j = 0;
    while j < NFLAGGED {
        t[j] = mm_core::hash_bytes_const(DUNDER_NAMES[dn_at(j) as usize].as_bytes());
        j += 1;
    }
    t
};

/// Open addressing over `FLAGGED_HASH`: from slot `hash % 256`, linear
/// probing to an empty slot; each holds a position plus one (0 is empty).
/// Under a third full, so a miss usually reads one slot.
const FLAGGED_SLOTS: [u8; 256] = {
    let mut t = [0u8; 256];
    let mut j = 0;
    while j < NFLAGGED {
        let h = FLAGGED_HASH[j];
        let mut i = (h % 256) as usize;
        while t[i] != 0 {
            // Distinct hashes, so a hash match is that dunder or no dunder.
            assert!(FLAGGED_HASH[t[i] as usize - 1] != h);
            i = (i + 1) % 256;
        }
        t[i] = j as u8 + 1;
        j += 1;
    }
    t
};

/// The position (below `NFLAGGED`) and flag of the dunder named `name`
/// (whose `hash_bytes` is `hash`), if [`Vm::compute_flags`] decides it:
/// one probe and, for a dunder, one comparison of its text.
fn flagged(hash: u64, name: &[u8]) -> Option<(usize, u64)> {
    let mut i = usize::try_from(hash % 256).unwrap_or(0);
    loop {
        let j = usize::from(*FLAGGED_SLOTS.get(i)?).checked_sub(1)?;
        if FLAGGED_HASH.get(j) == Some(&hash) {
            let text = DUNDER_NAMES.get(dn_at(j) as usize)?;
            return (text.as_bytes() == name)
                .then(|| (j, FLAG_NAMES.get(j).map_or(f::ARITH, |&(_, bit)| bit)));
        }
        i = (i + 1) % FLAGGED_SLOTS.len();
    }
}

/// Most inline slots an instance gets; more attributes go to the overflow
/// buffer.
const MAX_INLINE: u64 = 32;

/// Wider objects use a dict: retaining every name-list prefix is quadratic.
pub(crate) const MAX_SHAPE_ATTRS: usize = 32;

/// A hidden class: the ordered attribute names of the instances that have
/// it, and the shapes that add one more name.
#[derive(Default)]
pub(crate) struct Shape {
    pub names: Vec<Value>,
    pub trans: Vec<(Value, u32)>,
    /// Between-cell compaction workspace, admitted with the shape record.
    pub reclaim_index: u32,
}

/// An attribute lookup that may need Python code.
pub(crate) enum Got {
    Value(Value),
    /// Not found (no exception raised yet).
    Missing,
}

impl Vm {
    // --------------------------------------------------------- basics

    /// Is `v` a user class?
    #[inline]
    pub(crate) fn is_class(v: Value) -> bool {
        v.is_obj_kind(ty::CLASS as u8)
    }

    /// Is `v` a class: a user class or a builtin type object?
    pub(crate) fn is_type(v: Value) -> bool {
        v.is_obj_kind(ty::CLASS as u8) || v.is_obj_kind(ty::TYPE as u8)
    }

    /// The user class of an instance (or of an exception of a user class).
    #[inline]
    pub(crate) fn user_class(&self, v: Value) -> Option<Value> {
        let k = u16::from(v.obj_kind()?);
        let c = match k {
            ty::INST => self.vslot(v, inst::CLASS),
            ty::EXC => self.vslot(v, exc_slot::TYPE),
            _ => return None,
        };
        Vm::is_class(c).then_some(c)
    }

    /// The dunder flags of `v`'s user class (0 for builtin values).
    #[inline]
    pub(crate) fn uflags(&self, v: Value) -> u64 {
        match self.user_class(v).or_else(|| self.meta_of(v)) {
            Some(c) => c.as_obj().map_or(0, |r| self.raw(r, cls::FLAGS)),
            None => 0,
        }
    }

    /// The metaclass of class `c` when it is not `type`.
    pub(crate) fn meta_of(&self, c: Value) -> Option<Value> {
        if !c.is_obj_kind(ty::CLASS as u8) {
            return None;
        }
        let m = self.vslot(c, cls::META);
        Vm::is_class(m).then_some(m)
    }

    /// `type(v)`.
    pub(crate) fn class_of(&self, v: Value) -> Value {
        if let Some(m) = self.meta_of(v) {
            return m;
        }
        if v.is_obj_kind(ty::INST as u8) {
            return self.vslot(v, inst::CLASS);
        }
        if v.is_obj_kind(ty::EXC as u8) {
            return self.vslot(v, exc_slot::TYPE);
        }
        self.type_obj(self.type_of(v))
    }

    /// The builtin type a user class's instances are laid out as.
    pub(crate) fn class_base_t(&self, c: Value) -> T {
        match c.as_obj() {
            Some(r) if Vm::is_class(c) => {
                T::from_index(usize::try_from(self.raw(r, cls::BASE)).unwrap_or(0))
                    .unwrap_or(T::object)
            }
            _ => self.type_of_typeobj(c).unwrap_or(T::object),
        }
    }

    /// A class's `__name__`.
    pub(crate) fn class_name(&self, c: Value) -> &str {
        if Vm::is_class(c) {
            self.str_ref(self.vslot(c, cls::NAME))
        } else {
            self.type_of_typeobj(c).map_or("object", T::name)
        }
    }

    /// The name a class gives itself in messages: its `__mm_tpname__` when
    /// set, else its `__name__`.
    pub(crate) fn class_tpname(&self, c: Value) -> &str {
        if Vm::is_class(c)
            && c.as_obj().map_or(0, |r| self.raw(r, cls::FLAGS)) & f::TPNAME != 0
            && let Some(k) = self.find_interned("__mm_tpname__")
            && let Some(n) = self.class_lookup(c, k)
            && Vm::is_str(n)
        {
            return self.str_ref(n);
        }
        self.class_name(c)
    }

    /// The MRO of a class or builtin type, as a vector of classes.
    pub(crate) fn mro_of(&self, c: Value) -> Vec<Value> {
        if Vm::is_class(c) {
            return self.tuple_items(self.vslot(c, cls::MRO));
        }
        let mut out = Vec::new();
        if let Some(mut t) = self.type_of_typeobj(c) {
            loop {
                out.push(self.type_obj(t));
                if t == T::object {
                    break;
                }
                t = t.base();
            }
        }
        out
    }

    /// `issubclass(c, target)` for classes and builtin types.
    pub(crate) fn subclass_of(&self, c: Value, target: Value) -> bool {
        if c == target {
            return true;
        }
        if Vm::is_class(c) {
            return self.mro_of(c).contains(&target);
        }
        match (self.type_of_typeobj(c), self.type_of_typeobj(target)) {
            (Some(a), Some(b)) => a.is_subclass(b),
            _ => false,
        }
    }

    /// `isinstance(v, cls)` for one class.
    pub(crate) fn instance_of(&self, v: Value, c: Value) -> bool {
        if let Some(t) = self.type_of_typeobj(c)
            && self.user_class(v).is_none()
        {
            return self.type_of(v).is_subclass(t);
        }
        self.subclass_of(self.class_of(v), c)
    }

    fn new_version(&mut self) -> u64 {
        let v = self.next_version;
        self.next_version += 1;
        v
    }

    // ------------------------------------------------------- lookup

    /// Looks `name` up along the MRO of class `c` (a user class or a
    /// builtin type): namespace dicts, then builtin method tables.
    pub(crate) fn class_lookup(&self, c: Value, name: Value) -> Option<Value> {
        let s = self.str_bytes(name);
        if Vm::is_class(c) {
            // One validation of the MRO and of the name for every entry.
            let mro = self.vslot(c, cls::MRO).as_obj()?;
            for &w in self.heap.slot_words(mro).unwrap_or(&[]) {
                let k = Value::from_bits(w).unwrap_or(Value::NONE);
                let v = if Vm::is_class(k) {
                    self.dict_get_str(self.vslot(k, cls::DICT), name)
                } else {
                    self.type_of_typeobj(k)
                        .and_then(|t| crate::builtins::method_index(t, s))
                        .map(Value::native)
                };
                if v.is_some() {
                    return v;
                }
            }
            return None;
        }
        let mut t = self.type_of_typeobj(c)?;
        loop {
            if let Some(f) = crate::builtins::method_index(t, s) {
                return Some(Value::native(f));
            }
            if t == T::object {
                return None;
            }
            t = t.base();
        }
    }

    /// `name` in class `k` itself (no bases).
    pub(crate) fn own_lookup(&self, k: Value, name: Value) -> Option<Value> {
        if Vm::is_class(k) {
            let d = self.vslot(k, cls::DICT);
            return self.dict_get_str(d, name);
        }
        let t = self.type_of_typeobj(k)?;
        crate::builtins::method_index(t, self.str_bytes(name)).map(Value::native)
    }

    /// The dunder `name` of `v`'s user class, if Python code defines it.
    pub(crate) fn dunder(&mut self, v: Value, name: Dn) -> Option<Value> {
        let c = self.user_class(v).or_else(|| self.meta_of(v))?;
        let n = self.dn(name).ok()?;
        self.user_lookup(c, n)
    }

    /// The interned name `d`: interned once per session and kept alive by
    /// the session ([`Vm::dunders`] is a GC root).
    #[inline]
    pub(crate) fn dn(&mut self, d: Dn) -> Result<Value, Raised> {
        let i = d as usize;
        match self.dunders.get(i) {
            Some(&v) if v != Value::UNDEFINED => Ok(v),
            _ => self.dn_slow(i),
        }
    }

    #[cold]
    #[inline(never)]
    fn dn_slow(&mut self, i: usize) -> Result<Value, Raised> {
        let v = self.intern(DUNDER_NAMES.get(i).copied().unwrap_or("__init__"))?;
        if self.dunders.len() < NDUNDERS {
            self.dunders.resize(NDUNDERS, Value::UNDEFINED);
        }
        if let Some(slot) = self.dunders.get_mut(i) {
            *slot = v;
        }
        Ok(v)
    }

    /// `name` along user class `c`'s MRO when a user class (not a builtin
    /// type such as `object`) defines it.
    pub(crate) fn user_lookup(&self, c: Value, name: Value) -> Option<Value> {
        // One validation of the MRO for every entry, as `class_lookup`.
        let mro = self.vslot(c, cls::MRO).as_obj()?;
        for &w in self.heap.slot_words(mro).unwrap_or(&[]) {
            let k = Value::from_bits(w).unwrap_or(Value::NONE);
            if Vm::is_class(k) {
                if let Some(v) = self.dict_get_str(self.vslot(k, cls::DICT), name) {
                    return Some(v);
                }
            } else if let Some(t) = self.type_of_typeobj(k)
                && crate::builtins::method_index(t, self.str_bytes(name)).is_some()
            {
                return None;
            }
        }
        None
    }

    /// Recomputes the dunder flags of class `c`. As in [`Vm::user_lookup`],
    /// the first MRO entry that defines a name decides it, so a builtin
    /// base's own method hides a later user class's. One pass over the MRO:
    /// a class dict's dunder keys, or a builtin type's native methods.
    /// Returns the number of MRO entries read.
    fn compute_flags(&mut self, c: Value) -> u64 {
        let mut decided = 0u128;
        let mut flags = 0;
        let mro = self.vslot(c, cls::MRO);
        let n = self.tuple_len(mro);
        for i in 0..n {
            let k = self.vslot(mro, u32::try_from(i).unwrap_or(0));
            if Vm::is_class(k) {
                flags |= self.own_flags(k, &mut decided);
            } else if i + 1 < n
                && let Some(t) = self.type_of_typeobj(k)
            {
                // A builtin type decides what it defines for the entries
                // after it (none after the last, `object`).
                if t == T::int {
                    flags |= f::INT_BASE;
                }
                for m in crate::builtins::methods_of(t) {
                    let name = m.name().as_bytes();
                    if name.starts_with(b"__")
                        && let Some((j, _)) = flagged(mm_core::hash_bytes(name), name)
                    {
                        decided |= 1 << j;
                    }
                }
            }
        }
        if let Some(r) = c.as_obj() {
            self.set_raw(r, cls::FLAGS, Self::index_flag(flags));
        }
        n as u64
    }

    /// `flags` without [`f::INDEX`] for a subclass of `int`.
    fn index_flag(flags: u64) -> u64 {
        if flags & f::INT_BASE == 0 {
            flags
        } else {
            flags & !f::INDEX
        }
    }

    /// [`Vm::compute_flags`], except that a class with one user base takes
    /// that base's flags and its own: the base's MRO follows the class in
    /// its own (walking every subclass's whole MRO when a dunder changed was
    /// quadratic in a deep hierarchy). The base's flags must be current.
    /// Returns the number of MRO entries read.
    #[inline(never)]
    fn refresh_flags(&mut self, c: Value) -> u64 {
        let bases = self.vslot(c, cls::BASES);
        let p = self.vslot(bases, 0);
        if self.tuple_len(bases) != 1 || !Vm::is_class(p) {
            return self.compute_flags(c);
        }
        let inherited = p.as_obj().map_or(0, |r| self.raw(r, cls::FLAGS));
        let flags = self.own_flags(c, &mut 0) | inherited;
        if let Some(r) = c.as_obj() {
            self.set_raw(r, cls::FLAGS, Self::index_flag(flags));
        }
        1
    }

    /// The flags of the dunders user class `k` defines itself that
    /// `decided` does not hold yet, which it then does. Inlined into both
    /// callers, as the loop was before it was shared: out of line it moved
    /// a spill into the dispatch loop's global load (+0.54% `loop` Ir).
    #[allow(
        clippy::inline_always,
        reason = "measured: out of line costs +0.54% loop Ir"
    )]
    #[inline(always)]
    fn own_flags(&self, k: Value, decided: &mut u128) -> u64 {
        let mut flags = 0;
        let d = self.vslot(k, cls::DICT);
        if !d.is_obj_kind(ty::DICT as u8) {
            return 0;
        }
        for e in 0..self.dict_used(d) {
            let Some((key, _)) = self.dict_entry(d, e) else {
                continue;
            };
            if let Some((j, bit)) = self.flagged_key(key)
                && *decided & 1 << j == 0
            {
                *decided |= 1 << j;
                flags |= bit;
            }
        }
        flags
    }

    /// [`flagged`] for `str` `key` (its cached hash).
    fn flagged_key(&self, key: Value) -> Option<(usize, u64)> {
        let name = self.str_bytes(key);
        if !name.starts_with(b"__") {
            return None;
        }
        flagged(self.heap.str_hash(key.as_obj()?).ok()?, name)
    }

    /// Gives `c` and every subclass new version tags and flags (after a
    /// change to `c`'s entry `name`). The flags are recomputed only when
    /// they depend on `name`.
    pub(crate) fn class_changed(&mut self, c: Value, name: Value) {
        let flags = self.flagged_key(name).is_some();
        let mut work = vec![c];
        // Versions only grow: one at least `start` was given by this walk
        // (a `seen` list made `B.x = v` quadratic in B's subclasses).
        let start = self.next_version;
        let mut visits = 0u64;
        while let Some(k) = work.pop() {
            let Some(r) = k.as_obj() else {
                continue;
            };
            if self.raw(r, cls::VERSION) >= start {
                continue;
            }
            let v = self.new_version();
            self.set_raw(r, cls::VERSION, v);
            visits += 1;
            if flags {
                // A base comes first, so its flags are current.
                visits += self.refresh_flags(k);
            }
            let subs = self.vslot(k, cls::SUBS);
            work.extend(self.list_items(subs));
        }
        self.burn(visits);
    }

    // ------------------------------------------------ class creation

    /// The C3 linearization of a class with these bases (CPython's error
    /// for an inconsistent one).
    fn c3(&mut self, bases: &[Value]) -> Result<Vec<Value>, Raised> {
        if let [b] = bases {
            // One base: merging its MRO with `[b]` leaves the MRO as is.
            return Ok(self.mro_of(*b));
        }
        let mut seqs: Vec<Vec<Value>> = bases.iter().map(|&b| self.mro_of(b)).collect();
        seqs.push(bases.to_vec());
        // How many lists hold each class past their head: a head is free
        // when none does (scanning every tail for each pick was cubic).
        let mut all: Vec<u64> = seqs
            .iter()
            .flat_map(|s| s.iter().skip(1).map(|v| v.to_bits()))
            .collect();
        all.sort_unstable();
        let mut tails: Vec<(u64, u32)> = Vec::new();
        for b in all {
            match tails.last_mut() {
                Some((k, n)) if *k == b => *n += 1,
                _ => tails.push((b, 1)),
            }
        }
        let find =
            |tails: &[(u64, u32)], v: Value| tails.binary_search_by_key(&v.to_bits(), |e| e.0);
        let mut at = vec![0usize; seqs.len()];
        let mut out = Vec::new();
        let mut steps = tails.len() as u64;
        loop {
            let mut pick = None;
            let mut live = false;
            for (s, &i) in seqs.iter().zip(&at) {
                steps += 1;
                if let Some(&head) = s.get(i) {
                    live = true;
                    if find(&tails, head).map_or(true, |k| tails[k].1 == 0) {
                        pick = Some(head);
                        break;
                    }
                }
            }
            if !live {
                self.burn(steps);
                return Ok(out);
            }
            let Some(head) = pick else {
                self.burn(steps);
                let mut names: Vec<String> = Vec::new();
                for (s, &i) in seqs.iter().zip(&at) {
                    let Some(&h) = s.get(i) else {
                        continue;
                    };
                    let n = String::from(self.class_name(h));
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
                let msg = format!(
                    "Cannot create a consistent method resolution order (MRO) for bases {}",
                    names.join(", ")
                );
                return Err(self.err(T::TypeError, &msg));
            };
            out.push(head);
            for (s, i) in seqs.iter().zip(&mut at) {
                if s.get(*i) == Some(&head) {
                    *i += 1;
                    // The next entry leaves the tail for the head.
                    if let Some(&next) = s.get(*i)
                        && let Ok(k) = find(&tails, next)
                    {
                        tails[k].1 -= 1;
                    }
                }
            }
        }
    }

    /// Creates a class from a namespace dict. `__set_name__` and
    /// `__init_subclass__` are the caller's (they run Python code).
    pub(crate) fn new_class(
        &mut self,
        name: Value,
        bases: &[Value],
        ns: Value,
    ) -> Result<Value, Raised> {
        let object = self.type_obj(T::object);
        let mut base_list: Vec<Value> = Vec::with_capacity(bases.len().max(1));
        for &b in bases {
            if !Vm::is_type(b) {
                let msg = format!(
                    "bases must be types, not {}",
                    String::from(self.type_name(b))
                );
                return Err(self.err(T::TypeError, &msg));
            }
            if base_list.contains(&b) {
                let msg = format!("duplicate base class {}", self.class_name(b));
                return Err(self.err(T::TypeError, &msg));
            }
            if let Some(t) = self.type_of_typeobj(b)
                && t != T::object
                && !t.is_exception()
                && !Vm::is_bx_type(t)
                && t != T::type_
            {
                let msg = if t == T::bool {
                    String::from("type 'bool' is not an acceptable base type")
                } else if t.module() == "re" {
                    format!("type 're.{}' is not an acceptable base type", t.name())
                } else {
                    format!("micro-monty does not support subclassing '{}'", t.name())
                };
                return Err(self.err(T::TypeError, &msg));
            }
            base_list.push(b);
        }
        if base_list.is_empty() {
            base_list.push(object);
        }
        let mro_rest = self.c3(&base_list)?;
        // The layout: the first builtin type of the MRO.
        let base_t = mro_rest
            .iter()
            .find_map(|&c| self.type_of_typeobj(c))
            .unwrap_or(T::object);
        // Two builtin bases with their own layouts cannot combine.
        let mut layouts = base_list
            .iter()
            .map(|&b| self.class_base_t(b))
            .filter(|&t| t != T::object && !t.is_exception());
        if let Some(first) = layouts.next()
            && layouts.any(|t| t != first)
        {
            return Err(self.err(
                T::TypeError,
                "multiple bases have instance lay-out conflict",
            ));
        }
        let qual_key = self.intern("__qualname__")?;
        let qualname = match self.dict_get_str(ns, qual_key) {
            Some(q) if Vm::is_str(q) => q,
            Some(_) => {
                return Err(self.err(T::TypeError, "type __qualname__ must be a str, not int"));
            }
            None => name,
        };
        // The class dict: a copy of the namespace, `__qualname__` removed,
        // `__new__` a staticmethod and `__init_subclass__` a classmethod.
        let d = self.new_dict()?;
        let new_key = self.dn(Dn::New)?;
        let isc_key = self.dn(Dn::InitSubclass)?;
        let eq_key = self.dn(Dn::Eq)?;
        let hash_key = self.dn(Dn::Hash)?;
        let doc_key = self.intern("__doc__")?;
        let mut has_eq = false;
        let mut has_hash = false;
        // `__classcell__` (a metaclass passing the class body's namespace
        // on): the `__class__` cell, filled here as CPython's `type.__new__`
        // does, so methods using `super()` work before the metaclass returns.
        let cell_key = self.find_interned("__classcell__");
        let mut cell = Value::NONE;
        for (k, v) in self.dict_items_vec(ns) {
            if k == qual_key {
                continue;
            }
            if Some(k) == cell_key && v.is_obj_kind(ty::CELL as u8) {
                cell = v;
                continue;
            }
            let v = if (k == new_key) && v.is_obj_kind(ty::FUNC as u8) {
                self.wrap_descr(ty::STATICMETHOD, v)?
            } else if k == isc_key && v.is_obj_kind(ty::FUNC as u8) {
                self.wrap_descr(ty::CLASSMETHOD, v)?
            } else {
                v
            };
            has_eq |= k == eq_key;
            has_hash |= k == hash_key;
            self.dict_set(d, k, v)?;
        }
        if has_eq && !has_hash {
            self.dict_set(d, hash_key, Value::NONE)?;
        }
        if self.dict_get_str(d, doc_key).is_none() {
            self.dict_set(d, doc_key, Value::NONE)?;
        }
        let bases_t = self.alloc_tuple(&base_list)?;
        let subs = self.new_list(&[])?;
        let version = self.new_version();
        // Inline slots for instances: what the bases' instances use, or
        // what `__init__` stores on `self`, so a first instance is cached.
        let mut hint = 2;
        for &b in &base_list {
            if Vm::is_class(b)
                && let Some(br) = b.as_obj()
            {
                hint = hint.max(self.raw(br, cls::HINT));
            }
        }
        let init_key = self.dn(Dn::Init)?;
        if let Some(init) = self.dict_get_str(d, init_key) {
            hint = hint.max(self.init_stores(init));
        }
        let r = {
            let res = self.heap.alloc_init(
                ty::CLASS,
                0,
                &[name, qualname, bases_t, Value::NONE, d, subs, Value::NONE],
                &[version, 0, base_t as u64, hint.min(MAX_INLINE)],
            );
            self.hx(res)
        }?;
        let c = Value::from_ref(r, ty::CLASS as u8);
        if let Some(cr) = cell.as_obj() {
            self.set_slot(cr, 0, c);
        }
        let mut mro = Vec::with_capacity(mro_rest.len() + 1);
        mro.push(c);
        mro.extend(mro_rest);
        let mro_t = self.alloc_tuple(&mro)?;
        self.set_slot(r, cls::MRO, mro_t);
        for &b in &base_list {
            if Vm::is_class(b) {
                let l = self.vslot(b, cls::SUBS);
                self.list_push(l, c)?;
            }
        }
        let n = self.refresh_flags(c);
        self.burn(n);
        Ok(c)
    }

    /// How many distinct attributes `init`, if a Python function, stores
    /// on its first parameter (`self.name = ...` in its own code).
    fn init_stores(&self, init: Value) -> u64 {
        if !init.is_obj_kind(ty::FUNC as u8) {
            return 0;
        }
        let Some(code) = init
            .as_obj()
            .and_then(|r| usize::try_from(self.raw(r, fn_slot::CODE)).ok())
            .and_then(|i| self.codes.get(i))
        else {
            return 0;
        };
        let bc = &code.code.bytecode;
        let mut names = [0usize; MAX_INLINE as usize];
        let mut n = 0;
        let mut pc = 0;
        while let Some(&w) = bc.get(pc) {
            let op = (w & 0xff) as u8;
            if op == mm_compile::bytecode::op::SET_ATTR
                && mm_compile::bytecode::operand(bc, pc, 0) == 0
            {
                let name = mm_compile::bytecode::operand(bc, pc, 1);
                if !names[..n].contains(&name) {
                    names[n] = name;
                    n += 1;
                    if n == names.len() {
                        break;
                    }
                }
            }
            pc += mm_compile::bytecode::instr_len(op);
        }
        n as u64
    }

    /// A `staticmethod` or `classmethod` around `func`.
    pub(crate) fn wrap_descr(&mut self, kind: u16, func: Value) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(kind, 0, &[func], &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, kind as u8))
    }

    /// Does creating class `c` need Python code (`__set_name__` on a
    /// namespace value, a Python `__init_subclass__` in a base, or class
    /// keywords)?
    pub(crate) fn class_needs_finish(&mut self, c: Value, kwargs: Value) -> Result<bool, Raised> {
        if kwargs != Value::NONE && self.dict_len(kwargs) > 0 {
            return Ok(true);
        }
        for (_, v) in self.dict_items_vec(self.vslot(c, cls::DICT)) {
            if self.uflags(v) & f::SET_NAME != 0 {
                return Ok(true);
            }
        }
        // `__init_subclass__` of a base (the class's own does not run).
        let mro = self.mro_of(c);
        self.burn(mro.len() as u64);
        let key = self.dn(Dn::InitSubclass)?;
        for &k in mro.iter().skip(1) {
            if let Some(v) = self.own_lookup(k, key) {
                return Ok(v.as_native().is_none());
            }
        }
        Ok(false)
    }

    /// `BUILD_CLASS`: the class, or `UNDEFINED` with a request for the
    /// prelude to run `__set_name__` and `__init_subclass__`. A class
    /// statement (`cell` not `UNDEFINED`) with a base that is not a class
    /// first has the prelude resolve its `__mro_entries__` (PEP 560).
    pub(crate) fn build_class(
        &mut self,
        name: Value,
        bases: Value,
        ns: Value,
        kwargs: Value,
        cell: Value,
        meta: Value,
    ) -> Result<Value, Raised> {
        let unresolved = bases.as_obj().is_some_and(|r| {
            self.heap
                .slot_words(r)
                .unwrap_or(&[])
                .iter()
                .any(|&w| !Vm::is_type(Value::from_bits(w).unwrap_or(Value::NONE)))
        });
        if cell != Value::UNDEFINED && unresolved {
            return self.prelude_request(
                "_build_bases",
                vec![name, bases, ns, kwargs, cell],
                Cont::None,
            );
        }
        self.build_resolved(name, bases, ns, kwargs, cell, meta)
    }

    /// [`Vm::build_class`] once the bases are resolved.
    pub(crate) fn build_resolved(
        &mut self,
        name: Value,
        bases: Value,
        ns: Value,
        kwargs: Value,
        cell: Value,
        meta: Value,
    ) -> Result<Value, Raised> {
        let bases = self.tuple_items(bases);
        // The metaclass: `metaclass=` or the most derived of the bases'.
        let mut meta = meta;
        let mut kwargs = kwargs;
        let mkey = self.intern("metaclass")?;
        if kwargs != Value::NONE
            && let Some(m) = self.dict_get_str(kwargs, mkey)
        {
            kwargs = self.dict_copy(kwargs)?;
            self.dict_del(kwargs, mkey)?;
            meta = m;
        }
        for &b in &bases {
            let bm = self.class_of(b);
            if Vm::is_type(b)
                && Vm::is_class(bm)
                && (meta == Value::NONE || self.subclass_of(bm, meta))
            {
                meta = bm;
            }
        }
        if meta != Value::NONE
            && meta != self.type_obj(T::type_)
            && Vm::is_class(meta)
            && let Some(k) = self.find_interned("__prepare__")
            && self.class_lookup(meta, k).is_some()
        {
            // The body has run in a plain dict; refuse rather than ignore.
            return Err(self.err(
                T::NotImplementedError,
                "micro-monty does not support __prepare__ (the class body always runs in a plain dict)",
            ));
        }
        if meta != Value::NONE && meta != self.type_obj(T::type_) && cell != Value::UNDEFINED {
            // A metaclass builds the class in Python code.
            let b = self.alloc_tuple(&bases)?;
            let kw = if kwargs == Value::NONE {
                self.new_dict()?
            } else {
                kwargs
            };
            return self.prelude_request(
                "_build_meta",
                vec![meta, name, b, ns, kw, cell],
                Cont::None,
            );
        }
        let c = self.new_class(name, &bases, ns)?;
        if Vm::is_class(meta)
            && let Some(r) = c.as_obj()
        {
            self.set_slot(r, cls::META, meta);
            self.compute_flags(c);
        }
        if cell.is_obj_kind(ty::CELL as u8)
            && let Some(r) = cell.as_obj()
        {
            self.set_slot(r, 0, c);
        }
        if self.class_needs_finish(c, kwargs)? {
            let kw = if kwargs == Value::NONE {
                self.new_dict()?
            } else {
                kwargs
            };
            return self.prelude_request("_finish_class", vec![c, kw], Cont::None);
        }
        Ok(c)
    }

    // ------------------------------------------------------ instances

    /// Can user classes extend builtin type `t` (instances wrap a value of
    /// it, see [`inst::BACK`])?
    pub(crate) fn is_bx_type(t: T) -> bool {
        matches!(
            t,
            T::tuple | T::list | T::dict | T::set | T::int | T::str | T::float | T::bytes
        )
    }

    /// The builtin value `v` extends, if it is an instance of a subclass of
    /// a builtin type.
    #[inline]
    pub(crate) fn backing(&self, v: Value) -> Option<Value> {
        if !v.is_obj_kind(ty::INST as u8) {
            return None;
        }
        let b = self.vslot(v, inst::BACK);
        (b != Value::NONE).then_some(b)
    }

    /// Is `v` an instance of an int or float subclass? Its base gives it
    /// `__int__`, `__float__` and `__trunc__`, so they read its value and
    /// never call its own `__index__` (CPython's `nb_int`, `nb_float`).
    pub(crate) fn num_backed(&self, v: Value) -> bool {
        self.backing(v)
            .is_some_and(|b| Vm::is_intlike(b) || b.as_float().is_some())
    }

    /// `v`, or the builtin value it extends.
    #[inline]
    pub(crate) fn unbx(&self, v: Value) -> Value {
        self.backing(v).unwrap_or(v)
    }

    /// A new instance of builtin-type subclass `c` extending `value`.
    pub(crate) fn new_bx(&mut self, c: Value, value: Value) -> Result<Value, Raised> {
        let hint = c.as_obj().map_or(2, |r| self.raw(r, cls::HINT));
        let o = self.new_instance(c, u32::try_from(hint).unwrap_or(2))?;
        if let Some(r) = o.as_obj() {
            self.set_slot(r, inst::BACK, self.unbx(value));
        }
        Ok(o)
    }

    /// The receiver a builtin method `d` found on an instance's class gets:
    /// the extended builtin value for its own type's methods.
    pub(crate) fn native_self(&self, d: Value, obj: Value) -> Value {
        match (
            self.backing(obj),
            d.as_native().and_then(crate::builtins::native),
        ) {
            (Some(_), Some(n)) if crate::builtins::keeps_instance(n) => obj,
            (Some(b), Some(n)) if n.owner.is_some_and(Vm::is_bx_type) => b,
            _ => obj,
        }
    }

    /// A new instance of class `c` with `n` inline slots, at most
    /// `MAX_INLINE` (a class's size hint never exceeds it, but a restored
    /// image may say anything).
    pub(crate) fn new_instance(&mut self, c: Value, n: u32) -> Result<Value, Raised> {
        let len = inst::FIRST as usize + (n as usize).min(MAX_INLINE as usize);
        let mut small = [Value::UNDEFINED; (inst::FIRST as usize) + MAX_INLINE as usize];
        let vals = &mut small[..len];
        vals[inst::CLASS as usize] = c;
        vals[inst::SHAPE as usize] = Value::int(0).unwrap_or(Value::NONE);
        vals[inst::OVER as usize] = Value::NONE;
        vals[inst::BACK as usize] = Value::NONE;
        let r = {
            let res = self.heap.alloc_init(ty::INST, 0, vals, &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::INST as u8))
    }

    /// Allocates an instance of class `c` (an exception for exception
    /// classes) with `args` as the exception arguments.
    pub(crate) fn alloc_of_class(&mut self, c: Value, args: &[Value]) -> Result<Value, Raised> {
        let base = self.class_base_t(c);
        if base.is_exception() {
            if matches!(base, T::NameError | T::UnboundLocalError) {
                self.name_error_built = true;
            }
            let a = self.alloc_tuple(args)?;
            return self.alloc_exc(c, a);
        }
        let hint = c.as_obj().map_or(2, |r| self.raw(r, cls::HINT));
        self.new_instance(c, u32::try_from(hint).unwrap_or(2))
    }

    /// The shape id of an instance in shape mode.
    #[inline]
    pub(crate) fn shape_of(&self, o: Value) -> Option<u32> {
        self.vslot(o, inst::SHAPE)
            .as_int()
            .and_then(|i| u32::try_from(i).ok())
    }

    /// Index of `name` in shape `s`.
    pub(crate) fn shape_index(&self, s: u32, name: Value) -> Option<usize> {
        self.shapes
            .get(s as usize)
            .and_then(|sh| sh.names.iter().position(|&n| n == name))
    }

    /// The value slot (inline) or overflow position of attribute `i`.
    fn attr_slot(&self, o: Value, i: usize) -> (bool, u32) {
        let ninline = o
            .as_obj()
            .map_or(0, |r| self.obj_len(r).saturating_sub(inst::FIRST));
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        if i < ninline {
            (true, inst::FIRST + i)
        } else {
            (false, i - ninline)
        }
    }

    /// An attribute stored on the instance itself.
    pub(crate) fn inst_get(&self, o: Value, name: Value) -> Option<Value> {
        if o.is_obj_kind(ty::EXC as u8) {
            let d = self.vslot(o, exc_slot::DICT);
            return self.dict_get_str(d, name);
        }
        let sh = self.vslot(o, inst::SHAPE);
        let Some(s) = sh.as_int() else {
            return self.dict_get_str(sh, name);
        };
        let i = self.shape_index(u32::try_from(s).ok()?, name)?;
        let (inline, k) = self.attr_slot(o, i);
        let v = if inline {
            self.vslot(o, k)
        } else {
            self.vslot(self.vslot(o, inst::OVER), k)
        };
        (v != Value::UNDEFINED).then_some(v)
    }

    /// The instance's slot holding attribute `name`, for the inline cache.
    pub(crate) fn inst_inline_slot(&self, o: Value, name: Value) -> Option<(u32, u32)> {
        let s = self.shape_of(o)?;
        let i = self.shape_index(s, name)?;
        let (inline, k) = self.attr_slot(o, i);
        inline.then_some((s, k))
    }

    /// The shape after adding `name` to shape `s`.
    fn shape_add(&mut self, s: u32, name: Value) -> Result<u32, Raised> {
        let Some(sh) = self.shapes.get(s as usize) else {
            return Err(self.err(T::SystemError, "bad instance shape"));
        };
        // A shape's transitions and names are scanned and copied: a unit
        // per 8 past the first 64 (`setattr` of many names was quadratic
        // for a unit each).
        let found = sh.trans.iter().position(|(n, _)| *n == name);
        let scanned = found.unwrap_or(sh.trans.len() + sh.names.len());
        if scanned >= 64 {
            self.burn(scanned as u64 / 8);
        }
        let Some(sh) = self.shapes.get(s as usize) else {
            return Err(self.err(T::SystemError, "bad instance shape"));
        };
        if let Some(&(_, to)) = found.and_then(|i| sh.trans.get(i)) {
            return Ok(to);
        }
        let n = sh.names.len() + 1;
        let trans_cap = sh.trans.capacity();
        let grow_trans = if sh.trans.len() == trans_cap {
            trans_cap.max(4)
        } else {
            0
        };
        let cap = self.shapes.capacity();
        let grow = if self.shapes.len() == cap {
            cap.max(4)
        } else {
            0
        };
        let extra = (grow * size_of::<Shape>()
            + grow_trans * size_of::<(Value, u32)>()
            + n * size_of::<Value>()) as u64;
        self.reserve_execution(extra)?;
        let mut names = Vec::new();
        let result = self
            .shapes
            .try_reserve_exact(grow)
            .and_then(|()| self.shapes[s as usize].trans.try_reserve_exact(grow_trans))
            .and_then(|()| names.try_reserve_exact(n));
        // Account partial table growth even when a later allocation failed.
        // Updating the cached total avoids scanning every code/shape per store.
        self.code_charge += ((self.shapes.capacity() - cap) * size_of::<Shape>()
            + (self.shapes[s as usize].trans.capacity() - trans_cap) * size_of::<(Value, u32)>())
            as u64;
        if result.is_err() {
            self.reserve_execution(0)?;
            return Err(self.err(T::MemoryError, ""));
        }
        names.extend_from_slice(&self.shapes[s as usize].names);
        names.push(name);
        self.code_charge += (names.capacity() * size_of::<Value>()) as u64;
        let id = u32::try_from(self.shapes.len()).unwrap_or(u32::MAX);
        self.shapes.push(Shape {
            names,
            trans: Vec::new(),
            reclaim_index: 0,
        });
        self.shapes[s as usize].trans.push((name, id));
        self.reserve_execution(0)?;
        Ok(id)
    }

    /// Stores an attribute on the instance itself; returns `(shape before,
    /// shape after, inline slot)` when an inline cache may remember it.
    pub(crate) fn inst_set(
        &mut self,
        o: Value,
        name: Value,
        v: Value,
    ) -> Result<Option<(u32, u32, u32)>, Raised> {
        let name = self.intern_value(name)?;
        let Some(r) = o.as_obj() else { return Ok(None) };
        if o.is_obj_kind(ty::EXC as u8) {
            let mut d = self.slot(r, exc_slot::DICT);
            if d == Value::NONE {
                d = self.new_dict()?;
                self.set_slot(r, exc_slot::DICT, d);
            }
            self.dict_set(d, name, v)?;
            return Ok(None);
        }
        let sh = self.slot(r, inst::SHAPE);
        let Some(s) = sh.as_int().and_then(|s| u32::try_from(s).ok()) else {
            self.dict_set(sh, name, v)?;
            return Ok(None);
        };
        let (s2, i) = if let Some(i) = self.shape_index(s, name) {
            (s, i)
        } else {
            if self
                .shapes
                .get(s as usize)
                .is_some_and(|sh| sh.names.len() >= MAX_SHAPE_ATTRS)
            {
                let d = self.inst_dict(o)?;
                self.dict_set(d, name, v)?;
                return Ok(None);
            }
            let to = self.shape_add(s, name)?;
            let n = self.shapes.get(to as usize).map_or(0, |x| x.names.len());
            (to, n - 1)
        };
        let (inline, k) = self.attr_slot(o, i);
        if inline {
            self.set_slot(r, k, v);
        } else {
            let mut over = self.slot(r, inst::OVER);
            let cap = over.as_obj().map_or(0, |b| self.obj_len(b));
            if k >= cap {
                let nb = self.alloc_buf(((cap * 2).max(4).max(k + 1)) as usize)?;
                for j in 0..cap {
                    let x = self.vslot(over, j);
                    self.set_slot(nb, j, x);
                }
                over = Value::from_ref(nb, ty::BUF as u8);
                self.set_slot(r, inst::OVER, over);
            }
            if let Some(b) = over.as_obj() {
                self.set_slot(b, k, v);
            }
        }
        if s2 != s {
            self.set_slot(
                r,
                inst::SHAPE,
                Value::int(i64::from(s2)).unwrap_or(Value::NONE),
            );
            // Let later instances of the class hold this many inline.
            let c = self.slot(r, inst::CLASS);
            if let Some(cr) = c.as_obj()
                && Vm::is_class(c)
            {
                let want = (i as u64 + 1).min(MAX_INLINE);
                if self.raw(cr, cls::HINT) < want {
                    self.set_raw(cr, cls::HINT, want);
                }
            }
        }
        Ok(inline.then_some((s, s2, k)))
    }

    /// Switches an instance to dict mode and returns its attribute dict.
    pub(crate) fn inst_dict(&mut self, o: Value) -> Result<Value, Raised> {
        let Some(r) = o.as_obj() else {
            return self.new_dict();
        };
        if o.is_obj_kind(ty::EXC as u8) {
            let mut d = self.slot(r, exc_slot::DICT);
            if d == Value::NONE {
                d = self.new_dict()?;
                self.set_slot(r, exc_slot::DICT, d);
            }
            return Ok(d);
        }
        let sh = self.slot(r, inst::SHAPE);
        let Some(s) = sh.as_int().and_then(|s| u32::try_from(s).ok()) else {
            return Ok(sh);
        };
        let d = self.new_dict()?;
        let names = self
            .shapes
            .get(s as usize)
            .map_or_default(|x| x.names.clone());
        for (i, n) in names.iter().enumerate() {
            let (inline, k) = self.attr_slot(o, i);
            let v = if inline {
                self.slot(r, k)
            } else {
                self.vslot(self.slot(r, inst::OVER), k)
            };
            if v != Value::UNDEFINED {
                self.dict_set(d, *n, v)?;
            }
        }
        self.set_slot(r, inst::SHAPE, d);
        self.set_slot(r, inst::OVER, Value::NONE);
        for i in inst::FIRST..self.obj_len(r) {
            self.set_slot(r, i, Value::UNDEFINED);
        }
        Ok(d)
    }

    /// Deletes an attribute stored on the instance; `false` if absent.
    pub(crate) fn inst_del(&mut self, o: Value, name: Value) -> Result<bool, Raised> {
        if self.inst_get(o, name).is_none() {
            return Ok(false);
        }
        let d = self.inst_dict(o)?;
        Ok(self.dict_del(d, name)?.is_some())
    }

    /// The names of an instance's attributes, for hints.
    pub(crate) fn inst_attr_names(&self, o: Value) -> Vec<Value> {
        if o.is_obj_kind(ty::EXC as u8) {
            let d = self.vslot(o, exc_slot::DICT);
            return self.dict_items_vec(d).into_iter().map(|(k, _)| k).collect();
        }
        let sh = self.vslot(o, inst::SHAPE);
        match sh.as_int() {
            Some(s) => self
                .shapes
                .get(usize::try_from(s).unwrap_or(usize::MAX))
                .map_or_default(|x| x.names.clone()),
            None => self
                .dict_items_vec(sh)
                .into_iter()
                .map(|(k, _)| k)
                .collect(),
        }
    }

    /// An interned copy of a `str` (attribute names are compared by
    /// identity in shapes).
    pub(crate) fn intern_value(&mut self, s: Value) -> Result<Value, Raised> {
        let Some(r) = s.as_obj() else { return Ok(s) };
        if self
            .heap
            .header(r)
            .is_ok_and(|h| h.flags() & mm_core::flags::INTERNED != 0)
        {
            return Ok(s);
        }
        let res = self.heap.intern_str(r);
        let r = self.hx(res)?;
        Ok(Value::from_ref(r, ty::STR as u8))
    }

    // ----------------------------------------------------- attributes

    /// A bound method object.
    pub(crate) fn new_method(&mut self, func: Value, obj: Value) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(ty::METHOD, 0, &[obj, func], &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::METHOD as u8))
    }

    /// A builtin method bound to `obj`.
    pub(crate) fn new_bound(&mut self, f: Value, obj: Value) -> Result<Value, Raised> {
        let r = {
            let res = self.heap.alloc_init(ty::BOUND, 0, &[obj, f], &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::BOUND as u8))
    }

    /// Binds a class attribute found for instance `obj` of class `c`
    /// (descriptors: functions, classmethods, staticmethods, properties).
    /// May leave a call request (a property getter).
    pub(crate) fn bind_attr(&mut self, d: Value, obj: Value, c: Value) -> Result<Value, Raised> {
        match d.obj_kind().map(u16::from) {
            Some(ty::FUNC | ty::HOSTFN) => self.new_method(d, obj),
            Some(ty::CLASSMETHOD) => {
                let f = self.vslot(d, 0);
                self.new_method(f, c)
            }
            Some(ty::STATICMETHOD) => Ok(self.vslot(d, 0)),
            Some(ty::PROPERTY) => {
                let g = self.vslot(d, 0);
                if g == Value::NONE {
                    return Err(self.no_accessor(d, obj, "getter"));
                }
                self.call_request(g, vec![obj], Cont::None)
            }
            _ if Self::native_classmethod(d) => self.new_bound(d, c),
            _ if d
                .as_native()
                .and_then(crate::builtins::native)
                .is_some_and(|n| n.owner.is_some()) =>
            {
                // A builtin method (builtin functions do not bind).
                let recv = self.native_self(d, obj);
                self.new_bound(d, recv)
            }
            _ => Ok(d),
        }
    }

    /// "property 'x' of 'C' object has no setter".
    fn no_accessor(&mut self, prop: Value, obj: Value, what: &str) -> Raised {
        let g = self.vslot(prop, 0);
        let name = self.vslot(g, fn_slot::NAME);
        let name = if Vm::is_str(name) {
            format!("property '{}'", self.str_ref(name))
        } else {
            String::from("property")
        };
        let msg = format!("{name} of '{}' object has no {what}", self.type_name(obj));
        self.err(T::AttributeError, &msg)
    }

    /// Binds a class attribute found by `C.name` on class `c`.
    fn bind_class_attr(&mut self, d: Value, c: Value) -> Result<Value, Raised> {
        match d.obj_kind().map(u16::from) {
            Some(ty::CLASSMETHOD) => {
                let f = self.vslot(d, 0);
                self.new_method(f, c)
            }
            Some(ty::STATICMETHOD) => Ok(self.vslot(d, 0)),
            _ if Self::native_classmethod(d) => self.new_bound(d, c),
            _ => Ok(d),
        }
    }

    /// Is `d` a builtin class method (`bytes.fromhex`), bound to the class
    /// also when a subclass inherits it?
    fn native_classmethod(d: Value) -> bool {
        d.as_native()
            .and_then(crate::builtins::native)
            .is_some_and(crate::builtins::is_classmethod)
    }

    /// Does user object `d` define `__set__` or `__delete__`?
    pub(crate) fn is_data_descr(&mut self, d: Value) -> bool {
        self.uflags(d) & f::DESCR != 0
            && (self.user_method(d, Dn::Set).is_some() || self.user_method(d, Dn::Delete).is_some())
    }

    /// `obj.name` for instances of user classes (and exceptions). `Missing`
    /// when nothing defines it and the class has no `__getattr__`.
    pub(crate) fn inst_getattr(&mut self, obj: Value, name: Value) -> Result<Got, Raised> {
        let c = self.class_of(obj);
        let d = self.class_lookup(c, name);
        if let Some(d) = d
            && d.is_obj_kind(ty::PROPERTY as u8)
        {
            let v = self.bind_attr(d, obj, c)?;
            if v == Value::UNDEFINED && self.uflags(obj) & f::GETATTR != 0 {
                // A getter's AttributeError falls back to `__getattr__`.
                if let Some(Request::Call { cont, .. }) = self.request.as_mut() {
                    *cont = Cont::GetattrFallback(obj, name);
                }
            }
            return Ok(Got::Value(v));
        }
        // A user data descriptor comes before the instance's own attribute;
        // a non-data one after it.
        let descr = match d {
            Some(d) if self.uflags(d) & f::DESCR != 0 => self.user_method(d, Dn::Get),
            _ => None,
        };
        if let (Some(d), Some(get)) = (d, descr)
            && self.is_data_descr(d)
        {
            let n = self.call_dunder(get, d, &[obj, c], Cont::None)?;
            return Ok(Got::Value(self.next_request(n)?));
        }
        if let Some(v) = self.inst_get(obj, name) {
            return Ok(Got::Value(v));
        }
        if let (Some(d), Some(get)) = (d, descr) {
            let n = self.call_dunder(get, d, &[obj, c], Cont::None)?;
            return Ok(Got::Value(self.next_request(n)?));
        }
        if let Some(d) = d {
            return Ok(Got::Value(self.bind_attr(d, obj, c)?));
        }
        let s = self.str_ref(name);
        match s {
            "__class__" => return Ok(Got::Value(c)),
            "__dict__" if obj.is_obj_kind(ty::INST as u8) && Vm::is_class(c) => {
                return Ok(Got::Value(self.inst_dict(obj)?));
            }
            _ => {}
        }
        if obj.is_obj_kind(ty::EXC as u8)
            && let Some(v) = self.exc_attr(obj, name)?
        {
            return Ok(Got::Value(v));
        }
        // The numeric data attributes of an int, float or complex subclass
        // instance are its value's (`I(5).real` is the int 5).
        if matches!(
            self.str_ref(name),
            "real" | "imag" | "numerator" | "denominator"
        ) && let Some(b) = self.backing(obj)
            && matches!(self.type_of(b), T::int | T::bool | T::float | T::complex)
        {
            return Ok(Got::Value(self.getattr(b, name)?));
        }
        if self.uflags(obj) & f::GETATTR != 0
            && let Ok(key) = self.dn(Dn::Getattr)
            && let Some(g) = self.class_lookup(c, key)
        {
            let m = self.bind_attr(g, obj, c)?;
            return Ok(Got::Value(self.call_request(m, vec![name], Cont::None)?));
        }
        Ok(Got::Missing)
    }

    /// `obj.name` for user instances, classes and the other objects this
    /// module adds; `None` for objects the generic lookup handles. May
    /// leave a call request (returning `UNDEFINED`).
    pub(crate) fn user_getattr(
        &mut self,
        obj: Value,
        name: Value,
    ) -> Result<Option<Value>, Raised> {
        let Some(kind) = obj.obj_kind().map(u16::from) else {
            return Ok(None);
        };
        let n = String::from(self.str_ref(name));
        let v = match kind {
            ty::INST => match self.inst_getattr(obj, name)? {
                Got::Value(v) => v,
                Got::Missing => return Err(self.attr_error(obj, name)),
            },
            ty::EXC if self.user_class(obj).is_some() => match self.inst_getattr(obj, name)? {
                Got::Value(v) => v,
                Got::Missing => return Err(self.attr_error(obj, name)),
            },
            ty::EXC => match self.inst_get(obj, name) {
                Some(v) => v,
                None if n == "__dict__" => self.inst_dict(obj)?,
                None => return Ok(None),
            },
            ty::CLASS => match self.class_getattr(obj, name)? {
                Got::Value(v) => v,
                Got::Missing => return Err(self.attr_error(obj, name)),
            },
            ty::TYPE => {
                let Some(t) = self.type_of_typeobj(obj) else {
                    return Ok(None);
                };
                match n.as_str() {
                    "__mro__" => {
                        let m = self.mro_of(obj);
                        self.alloc_tuple(&m)?
                    }
                    "__bases__" => {
                        let b = if t == T::object {
                            Vec::new()
                        } else {
                            vec![self.type_obj(t.base())]
                        };
                        self.alloc_tuple(&b)?
                    }
                    "__base__" if t == T::object => Value::NONE,
                    "__base__" => self.type_obj(t.base()),
                    "__module__" => self.intern(t.module())?,
                    "__dict__" => crate::builtins::type_dict(self, t)?,
                    "__class_getitem__" if crate::dunder::generic_type(t) => {
                        let f = self.prelude_fn("_class_getitem")?;
                        self.new_method(f, obj)?
                    }
                    "__class__" => self.type_obj(T::type_),
                    "mro" => match crate::builtins::find_method(T::type_, "mro") {
                        Some(f) => self.new_bound(Value::native(f), obj)?,
                        None => return Ok(None),
                    },
                    _ => return Ok(None),
                }
            }
            ty::METHOD => match n.as_str() {
                "__self__" => self.vslot(obj, 0),
                "__func__" => self.vslot(obj, 1),
                "__class__" => self.type_obj(T::method),
                _ => {
                    // A method of methods passes the name on to the
                    // innermost function: a loop, not Rust recursion.
                    let mut f = self.vslot(obj, 1);
                    while f.is_obj_kind(ty::METHOD as u8) {
                        f = self.vslot(f, 1);
                    }
                    return self.getattr(f, name).map(Some);
                }
            },
            ty::SUPER => match self.super_getattr(obj, name)? {
                Got::Value(v) => v,
                Got::Missing => {
                    let msg = format!("'super' object has no attribute '{n}'");
                    return Err(self.err(T::AttributeError, &msg));
                }
            },
            ty::GEN | ty::AGEN => {
                let f = self.vslot(obj, crate::generator::g::FUNC);
                let running = if kind == ty::GEN {
                    "gi_running"
                } else {
                    "ag_running"
                };
                match n.as_str() {
                    "__name__" => self.vslot(f, fn_slot::NAME),
                    "__qualname__" => self.vslot(f, fn_slot::QUALNAME),
                    m if m == running => {
                        Value::from_bool(self.gen_state(obj) == crate::generator::g::RUNNING)
                    }
                    _ => return Ok(None),
                }
            }
            ty::PROPERTY => match n.as_str() {
                "fget" => self.vslot(obj, 0),
                "fset" => self.vslot(obj, 1),
                "fdel" => self.vslot(obj, 2),
                "__doc__" => self.vslot(obj, 3),
                "__isabstractmethod__" => self.any_abstract(obj, 3),
                _ => return Ok(None),
            },
            ty::STATICMETHOD | ty::CLASSMETHOD => match n.as_str() {
                "__func__" | "__wrapped__" => self.vslot(obj, 0),
                "__isabstractmethod__" => self.any_abstract(obj, 1),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        Ok(Some(v))
    }

    /// `__isabstractmethod__` of a property or class/static method: whether
    /// one of its first `n` wrapped functions has a true one in its dict.
    fn any_abstract(&self, d: Value, n: u32) -> Value {
        let key = self.find_interned("__isabstractmethod__");
        let any = (0..n).any(|i| {
            let f = self.vslot(d, i);
            f.is_obj_kind(ty::FUNC as u8)
                && key
                    .and_then(|k| self.dict_get_str(self.vslot(f, fn_slot::DICT), k))
                    .is_some_and(|v| self.truthy(v))
        });
        Value::from_bool(any)
    }

    /// `C.name` for user classes.
    pub(crate) fn class_getattr(&mut self, c: Value, name: Value) -> Result<Got, Raised> {
        // `type`'s data descriptors come before the class namespace.
        let special = matches!(
            self.str_ref(name),
            "__name__"
                | "__qualname__"
                | "__mro__"
                | "__bases__"
                | "__base__"
                | "__class__"
                | "__dict__"
                | "__annotations__"
        );
        if !special && let Some(d) = self.class_lookup(c, name) {
            if self.uflags(d) & f::DESCR != 0
                && let Some(get) = self.user_method(d, Dn::Get)
            {
                let n = self.call_dunder(get, d, &[Value::NONE, c], Cont::None)?;
                return Ok(Got::Value(self.next_request(n)?));
            }
            return Ok(Got::Value(self.bind_class_attr(d, c)?));
        }
        let v = match self.str_ref(name) {
            "__name__" => self.vslot(c, cls::NAME),
            "__qualname__" => self.vslot(c, cls::QUALNAME),
            "__mro__" => self.vslot(c, cls::MRO),
            "__bases__" => self.vslot(c, cls::BASES),
            "__base__" => {
                let b = self.vslot(c, cls::BASES);
                self.vslot(b, 0)
            }
            "__class__" => self.class_of(c),
            "__dict__" => {
                let d = self.vslot(c, cls::DICT);
                self.dict_copy(d)?
            }
            // The class's own (stringized) annotations, never a base's; an
            // empty dict, kept, when the body had none (as in 3.14).
            "__annotations__" => {
                let d = self.vslot(c, cls::DICT);
                if let Some(v) = self.dict_get_str(d, name) {
                    v
                } else {
                    let v = self.new_dict()?;
                    self.dict_set(d, name, v)?;
                    self.class_changed(c, name);
                    v
                }
            }
            "mro" => match crate::builtins::find_method(T::type_, "mro") {
                Some(f) => self.new_bound(Value::native(f), c)?,
                None => return Ok(Got::Missing),
            },
            _ => {
                // The metaclass's attributes, bound to the class.
                let Some(m) = self.meta_of(c).and_then(|m| self.class_lookup(m, name)) else {
                    return Ok(Got::Missing);
                };
                if self.uflags(m) & f::DESCR != 0
                    && let Some(get) = self.user_method(m, Dn::Get)
                {
                    let mc = self.class_of(c);
                    let n = self.call_dunder(get, m, &[c, mc], Cont::None)?;
                    return Ok(Got::Value(self.next_request(n)?));
                }
                let mc = self.class_of(c);
                self.bind_attr(m, c, mc)?
            }
        };
        Ok(Got::Value(v))
    }

    /// Exception data attributes.
    pub(crate) fn exc_attr(&mut self, obj: Value, name: Value) -> Result<Option<Value>, Raised> {
        Ok(match self.str_ref(name) {
            "args" => Some(self.vslot(obj, exc_slot::ARGS)),
            "__cause__" => Some(self.vslot(obj, exc_slot::CAUSE)),
            "__context__" => Some(self.vslot(obj, exc_slot::CONTEXT)),
            "__traceback__" => Some(self.traceback_of(obj)?),
            "__suppress_context__" => Some(Value::from_bool(
                obj.as_obj()
                    .is_some_and(|r| self.raw(r, exc_slot::FLAGS) & 1 != 0),
            )),
            "__dict__" => Some(self.inst_dict(obj)?),
            "value" if self.instance_of(obj, self.type_obj(T::StopIteration)) => {
                let args = self.tuple_items(self.vslot(obj, exc_slot::ARGS));
                Some(args.first().copied().unwrap_or(Value::NONE))
            }
            n @ ("errno" | "strerror" | "filename" | "filename2")
                if self.instance_of(obj, self.type_obj(T::OSError)) =>
            {
                Some(self.os_error_attr(obj, n))
            }
            n @ ("name" | "path" | "name_from" | "msg") if self.is_import_error(obj) => {
                Some(self.import_attr(obj, n))
            }
            n @ ("msg"
            | "filename"
            | "lineno"
            | "offset"
            | "text"
            | "end_lineno"
            | "end_offset"
            | "print_file_and_line")
                if self.exc_type(obj).is_subclass(T::SyntaxError) =>
            {
                Some(self.syntax_attr(obj, n))
            }
            "name" => self.exc_name_attr(obj, "name"),
            "obj" => self.exc_name_attr(obj, "obj"),
            "code" => self.exit_code(obj),
            _ => None,
        })
    }

    /// `SystemExit.code`: what was assigned, else the one argument, the
    /// arguments, or `None`.
    pub(crate) fn exit_code(&mut self, e: Value) -> Option<Value> {
        if !self.instance_of(e, self.type_obj(T::SystemExit)) {
            return None;
        }
        if let Some(k) = self.find_interned("code")
            && let Some(v) = self.inst_get(e, k)
        {
            return Some(v);
        }
        let args = self.vslot(e, exc_slot::ARGS);
        Some(match self.tuple_items(args)[..] {
            [] => Value::NONE,
            [a] => a,
            _ => args,
        })
    }

    /// `OSError`'s `errno`, `strerror`, `filename` and `filename2`, from
    /// its arguments as CPython's constructor reads them (2 to 5 of them).
    pub(crate) fn os_error_attr(&self, obj: Value, name: &str) -> Value {
        let args = self.tuple_items(self.vslot(obj, exc_slot::ARGS));
        if !(2..=5).contains(&args.len()) {
            return Value::NONE;
        }
        let i = match name {
            "errno" => 0,
            "strerror" => 1,
            "filename" => 2,
            _ => 4,
        };
        args.get(i).copied().unwrap_or(Value::NONE)
    }

    /// `obj.name = v` for instances of user classes; may leave a call
    /// request (a property setter, `__setattr__`). Returns what an inline
    /// cache may remember.
    pub(crate) fn inst_setattr(
        &mut self,
        obj: Value,
        name: Value,
        v: Value,
    ) -> Result<Option<(u32, u32, u32)>, Raised> {
        let c = self.class_of(obj);
        if self.uflags(obj) & f::SETATTR != 0
            && let Ok(key) = self.dn(Dn::Setattr)
            && let Some(sa) = self.class_lookup(c, key)
            && sa.as_native().is_none()
        {
            let m = self.bind_attr(sa, obj, c)?;
            self.call_request(m, vec![name, v], Cont::Discard)?;
            return Ok(None);
        }
        self.generic_setattr(obj, name, v)
    }

    /// `object.__setattr__`: data descriptors, then the instance.
    pub(crate) fn generic_setattr(
        &mut self,
        obj: Value,
        name: Value,
        v: Value,
    ) -> Result<Option<(u32, u32, u32)>, Raised> {
        let c = self.class_of(obj);
        let d = self.class_lookup(c, name);
        if let Some(d) = d
            && d.is_obj_kind(ty::PROPERTY as u8)
        {
            let s = self.vslot(d, 1);
            if s == Value::NONE {
                return Err(self.no_accessor(d, obj, "setter"));
            }
            self.call_request(s, vec![obj, v], Cont::Discard)?;
            return Ok(None);
        }
        if let Some(d) = d
            && self.uflags(d) & f::DESCR != 0
            && let Some(set) = self.user_method(d, Dn::Set)
        {
            let n = self.call_dunder(set, d, &[obj, v], Cont::Discard)?;
            self.next_request(n)?;
            return Ok(None);
        }
        if !Vm::is_class(c) {
            let n = String::from(self.str_ref(name));
            let msg = format!(
                "'{}' object has no attribute '{n}' and no __dict__ for setting new attributes",
                self.type_name(obj)
            );
            return Err(self.err(T::AttributeError, &msg));
        }
        if obj.is_obj_kind(ty::EXC as u8) && self.exc_set_special(obj, name, v)? {
            return Ok(None);
        }
        self.inst_set(obj, name, v)
    }

    /// `del obj.name` for instances of user classes.
    pub(crate) fn inst_delattr(&mut self, obj: Value, name: Value) -> Result<(), Raised> {
        let c = self.class_of(obj);
        if self.uflags(obj) & f::DELATTR != 0
            && let Ok(key) = self.dn(Dn::Delattr)
            && let Some(da) = self.class_lookup(c, key)
            && da.as_native().is_none()
        {
            let m = self.bind_attr(da, obj, c)?;
            self.call_request(m, vec![name], Cont::Discard)?;
            return Ok(());
        }
        self.generic_delattr(obj, name)
    }

    /// `object.__delattr__`.
    pub(crate) fn generic_delattr(&mut self, obj: Value, name: Value) -> Result<(), Raised> {
        let c = self.class_of(obj);
        let d = self.class_lookup(c, name);
        if let Some(d) = d
            && d.is_obj_kind(ty::PROPERTY as u8)
        {
            let del = self.vslot(d, 2);
            if del == Value::NONE {
                return Err(self.no_accessor(d, obj, "deleter"));
            }
            self.call_request(del, vec![obj], Cont::Discard)?;
            return Ok(());
        }
        if let Some(d) = d
            && self.uflags(d) & f::DESCR != 0
            && let Some(del) = self.user_method(d, Dn::Delete)
        {
            let n = self.call_dunder(del, d, &[obj], Cont::Discard)?;
            self.next_request(n)?;
            return Ok(());
        }
        if self.inst_del(obj, name)? {
            return Ok(());
        }
        Err(self.attr_error(obj, name))
    }

    /// Calls the Python method `name` (dunder flag `flag`) of class `c`'s
    /// metaclass with `args`, leaving a call request; false when the
    /// metaclass has none.
    pub(crate) fn meta_hook(
        &mut self,
        c: Value,
        name: &str,
        flag: u64,
        args: Vec<Value>,
    ) -> Result<bool, Raised> {
        if self.uflags(c) & flag == 0 {
            return Ok(false);
        }
        let Some(m) = self.meta_of(c) else {
            return Ok(false);
        };
        let key = self.intern(name)?;
        match self.class_lookup(m, key) {
            Some(h) if h.as_native().is_none() => {
                let b = self.bind_attr(h, c, m)?;
                self.call_request(b, args, Cont::Discard)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// `C.name = v` on a user class.
    pub(crate) fn class_setattr(&mut self, c: Value, name: Value, v: Value) -> Result<(), Raised> {
        let name = self.intern_value(name)?;
        match self.str_ref(name) {
            "__name__" => {
                if !Vm::is_str(v) {
                    return Err(self.err(T::TypeError, "can only assign string to C.__name__"));
                }
                if let Some(r) = c.as_obj() {
                    self.set_slot(r, cls::NAME, v);
                }
                return Ok(());
            }
            "__qualname__" => {
                if let Some(r) = c.as_obj() {
                    self.set_slot(r, cls::QUALNAME, v);
                }
                return Ok(());
            }
            _ => {}
        }
        let d = self.vslot(c, cls::DICT);
        self.dict_set(d, name, v)?;
        if name == self.dn(Dn::Init)?
            && let Some(r) = c.as_obj()
        {
            let hint = self.init_stores(v).min(MAX_INLINE);
            if self.raw(r, cls::HINT) < hint {
                self.set_raw(r, cls::HINT, hint);
            }
        }
        self.class_changed(c, name);
        Ok(())
    }

    /// `del C.name` on a user class.
    pub(crate) fn class_delattr(&mut self, c: Value, name: Value) -> Result<(), Raised> {
        let d = self.vslot(c, cls::DICT);
        if self.dict_del(d, name)?.is_none() {
            return Err(self.attr_error(c, name));
        }
        self.class_changed(c, name);
        Ok(())
    }

    /// `super(type, obj)`.
    pub(crate) fn new_super(&mut self, c: Value, obj: Value) -> Result<Value, Raised> {
        if !Vm::is_type(c) {
            let msg = format!(
                "super() argument 1 must be a type, not {}",
                String::from(self.type_name(c))
            );
            return Err(self.err(T::TypeError, &msg));
        }
        let start = if Vm::is_type(obj) && self.subclass_of(obj, c) {
            obj
        } else if self.instance_of(obj, c) {
            self.class_of(obj)
        } else {
            let msg = format!(
                "super(type, obj): obj (instance of {}) is not an instance or subtype of type ({}).",
                String::from(self.type_name(obj)),
                self.class_name(c)
            );
            return Err(self.err(T::TypeError, &msg));
        };
        let r = {
            let res = self.heap.alloc_init(ty::SUPER, 0, &[c, obj, start], &[]);
            self.hx(res)
        }?;
        Ok(Value::from_ref(r, ty::SUPER as u8))
    }

    /// `super(...).name`.
    pub(crate) fn super_getattr(&mut self, s: Value, name: Value) -> Result<Got, Raised> {
        let c = self.vslot(s, 0);
        let obj = self.vslot(s, 1);
        let start = self.vslot(s, 2);
        let mro = self.mro_of(start);
        let pos = mro
            .iter()
            .position(|&k| k == c)
            .map_or(mro.len(), |p| p + 1);
        for &k in &mro[pos..] {
            let Some(d) = self.own_lookup(k, name) else {
                continue;
            };
            let is_cls = obj == start;
            if d.as_native().is_some() {
                return Ok(Got::Value(match self.str_ref(name) {
                    "__new__" => d,
                    "__init_subclass__" => self.new_bound(d, start)?,
                    _ if is_cls => d,
                    _ => {
                        let recv = self.native_self(d, obj);
                        self.new_bound(d, recv)?
                    }
                }));
            }
            if is_cls {
                return Ok(Got::Value(self.bind_class_attr(d, start)?));
            }
            return Ok(Got::Value(self.bind_attr(d, obj, start)?));
        }
        if self.str_ref(name) == "__class__" {
            return Ok(Got::Value(self.type_obj(T::super_)));
        }
        Ok(Got::Missing)
    }

    /// `type(name, bases, dict)`.
    pub(crate) fn type_new(
        &mut self,
        name: Value,
        bases: Value,
        ns: Value,
        kwargs: Value,
        meta: Value,
    ) -> Result<Value, Raised> {
        for (i, v, k, want) in [
            (1, name, ty::STR, "str"),
            (2, bases, ty::TUPLE, "tuple"),
            (3, ns, ty::DICT, "dict"),
        ] {
            let ok = if k == ty::STR {
                Vm::is_str(v)
            } else {
                v.is_obj_kind(k as u8)
            };
            if !ok {
                // CPython's argument-type message says `None`, not `NoneType`.
                let got = if v == Value::NONE {
                    String::from("None")
                } else {
                    String::from(self.type_name(v))
                };
                let msg = format!("type.__new__() argument {i} must be {want}, not {got}");
                return Err(self.err(T::TypeError, &msg));
            }
        }
        let ns2 = self.dict_copy(ns)?;
        let mkey = self.intern("__module__")?;
        if self.dict_get_str(ns2, mkey).is_none() {
            let nkey = self.intern("__name__")?;
            if let Some(m) = self.dict_get_str(self.globals, nkey) {
                self.dict_set(ns2, mkey, m)?;
            }
        }
        self.build_class(name, bases, ns2, kwargs, Value::UNDEFINED, meta)
    }

    /// CPython's `%T` for `v`'s type: `module.qualname`, without the
    /// module when it is `builtins` or `__main__`.
    pub(crate) fn type_full_name(&self, v: Value) -> String {
        let Some(c) = self
            .user_class(v)
            .filter(|_| self.uflags(v) & f::TPNAME == 0)
        else {
            return self.type_name(v).into();
        };
        let d = self.vslot(c, cls::DICT);
        let m = self
            .find_interned("__module__")
            .and_then(|k| self.dict_get_str(d, k));
        let q = self.str_ref(self.vslot(c, cls::QUALNAME));
        match m.filter(|&m| Vm::is_str(m)).map(|m| self.str_ref(m)) {
            Some(m) if m != "builtins" && m != "__main__" => format!("{m}.{q}"),
            _ => q.into(),
        }
    }

    /// `repr(cls)`.
    pub(crate) fn class_repr(&self, c: Value) -> String {
        let d = self.vslot(c, cls::DICT);
        let m = self
            .find_interned("__module__")
            .and_then(|k| self.dict_get_str(d, k));
        let q = self.str_ref(self.vslot(c, cls::QUALNAME));
        match m {
            Some(m) if Vm::is_str(m) && self.str_ref(m) != "builtins" => {
                format!("<class '{}.{q}'>", self.str_ref(m))
            }
            _ => format!("<class '{q}'>"),
        }
    }

    /// An interned string if it exists (no allocation).
    pub(crate) fn find_interned(&self, s: &str) -> Option<Value> {
        self.heap
            .find_interned(s)
            .map(|r| Value::from_ref(r, ty::STR as u8))
    }

    /// The qualified name used in reports: `Outer.Name` (no module for
    /// `__main__` and builtins).
    pub(crate) fn class_report_name(&self, c: Value) -> String {
        if !Vm::is_class(c) {
            // Engine types CPython defines outside `builtins`
            // (`asyncio.exceptions.CancelledError`) report their module.
            return match self.type_of_typeobj(c) {
                Some(t) if t.module() != "builtins" => {
                    format!("{}.{}", t.module(), self.class_name(c))
                }
                _ => String::from(self.class_name(c)),
            };
        }
        let q = String::from(self.str_ref(self.vslot(c, cls::QUALNAME)));
        let d = self.vslot(c, cls::DICT);
        let m = self
            .find_interned("__module__")
            .and_then(|k| self.dict_get_str(d, k));
        match m {
            Some(m) if Vm::is_str(m) && !matches!(self.str_ref(m), "__main__" | "builtins") => {
                format!("{}.{q}", self.str_ref(m))
            }
            _ => q,
        }
    }

    /// Sets an exception attribute that lives in a slot; `false` for other
    /// names.
    pub(crate) fn exc_set_special(
        &mut self,
        e: Value,
        name: Value,
        v: Value,
    ) -> Result<bool, Raised> {
        let Some(r) = e.as_obj() else {
            return Ok(false);
        };
        match self.str_ref(name) {
            "__cause__" => {
                self.set_slot(r, exc_slot::CAUSE, v);
                let fl = self.raw(r, exc_slot::FLAGS) | 1;
                self.set_raw(r, exc_slot::FLAGS, fl);
            }
            "__context__" => self.set_slot(r, exc_slot::CONTEXT, v),
            "__suppress_context__" => {
                let fl = self.raw(r, exc_slot::FLAGS);
                let fl = if self.truthy(v) { fl | 1 } else { fl & !1 };
                self.set_raw(r, exc_slot::FLAGS, fl);
            }
            "args" => {
                let items = match self.collect(v) {
                    Ok(items) => items,
                    Err(Raised) if self.slow => {
                        // An iterator that runs Python code, or goes on in
                        // chunks (`Vm::partial`): `setattr` in the prelude.
                        self.slow = false;
                        let f = self.prelude_fn("_op_store_attr")?;
                        let (f, args, _) = self.slow_call(f, vec![e, name, v], Value::NONE)?;
                        self.request_call(f, args, Cont::Discard);
                        return Ok(true);
                    }
                    Err(err) => return Err(err),
                };
                let t = self.alloc_tuple(&items)?;
                self.set_slot(r, exc_slot::ARGS, t);
            }
            "__traceback__" => self.set_traceback(e, v)?,
            "name" => return self.set_exc_name_attr(e, "name", v),
            "obj" => return self.set_exc_name_attr(e, "obj", v),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Leaves a request to call `func(*args)` and finish with `cont`;
    /// returns `UNDEFINED` (the request marker).
    #[allow(
        clippy::unnecessary_wraps,
        reason = "used as the result of attribute lookups, like the other request makers"
    )]
    pub(crate) fn call_request(
        &mut self,
        func: Value,
        args: Vec<Value>,
        cont: Cont,
    ) -> Result<Value, Raised> {
        self.request = Some(Request::Call {
            func,
            args: args.into(),
            kw: Value::NONE,
            cont,
            outer: Cont::None,
        });
        Ok(Value::UNDEFINED)
    }
}
