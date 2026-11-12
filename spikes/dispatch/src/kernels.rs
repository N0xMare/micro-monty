//! The seven kernels: expected results (computed by CPython 3.12 with
//! kernels.py) and the untimed setup shared by every design.

use crate::heap::{Fail, Heap, names};
use crate::rt;
use crate::value::Value;
use crate::vm::Program;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kernel {
    Fib,
    Loop,
    Closure,
    Attr,
    Method,
    List,
    Dict,
}

impl Kernel {
    pub const ALL: [Kernel; 7] = [
        Kernel::Fib,
        Kernel::Loop,
        Kernel::Closure,
        Kernel::Attr,
        Kernel::Method,
        Kernel::List,
        Kernel::Dict,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kernel::Fib => "fib",
            Kernel::Loop => "loop",
            Kernel::Closure => "closure",
            Kernel::Attr => "attr",
            Kernel::Method => "method",
            Kernel::List => "list",
            Kernel::Dict => "dict",
        }
    }

    /// The value CPython 3.12 computes (see kernels.py, `EXPECTED`).
    pub fn expected(self) -> i64 {
        match self {
            Kernel::Fib => 196_418,
            Kernel::Loop => 4_499_998_500_000,
            Kernel::Closure => 3_000_000,
            Kernel::Attr => 500_000_500_002,
            Kernel::Method => 2_000_000,
            Kernel::List => 499_999_500_000,
            Kernel::Dict => 19_999_900_000,
        }
    }
}

/// Untimed setup: globals, classes and instances. Returns the entry code's
/// arguments. `range` is a global bound to a native.
pub fn setup(k: Kernel, prog: &Program, heap: &mut Heap) -> Result<Vec<Value>, Fail> {
    heap.define_global(names::RANGE, Value::native(rt::NATIVE_RANGE));
    match k {
        Kernel::Fib => {
            let f = rt::new_function(heap, prog.code("fib"), &[])?;
            heap.define_global(names::FIB, f);
            Ok(vec![])
        }
        Kernel::Closure => {
            let f = rt::new_function(heap, prog.code("make"), &[])?;
            heap.define_global(names::MAKE, f);
            Ok(vec![])
        }
        Kernel::Attr => {
            // class P: def __init__(self): self.x = 1; self.y = 2
            let cls = heap.new_class(&[])?;
            let shape = heap.add_shape(vec![names::X, names::Y]);
            let p = heap.new_instance(cls, shape, &[Value::int(1), Value::int(2)])?;
            Ok(vec![p])
        }
        Kernel::Method => {
            // class C: __init__ sets self.k = 2; def inc(self, v): return v + self.k
            let inc = rt::new_function(heap, prog.code("inc"), &[])?;
            let cls = heap.new_class(&[(names::INC, inc)])?;
            let shape = heap.add_shape(vec![names::K]);
            let c = heap.new_instance(cls, shape, &[Value::int(2)])?;
            Ok(vec![c])
        }
        Kernel::Loop | Kernel::List | Kernel::Dict => Ok(vec![]),
    }
}
