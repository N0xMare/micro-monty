//! Builtin types: arena type ids and the Python-visible type table.

/// Arena object type ids (also used as the `Ref` kind byte).
pub mod ty {
    /// mm-core's `str`.
    pub const STR: u16 = mm_core::types::STR;
    /// `tuple`: `n` value slots.
    pub const TUPLE: u16 = 16;
    /// `list`: value slot 0 the buffer; raw slot 1 the length.
    pub const LIST: u16 = 17;
    /// A value buffer (list items, dict entries, set keys).
    pub const BUF: u16 = 18;
    /// `dict`: inline (key, value) pairs, or entries and index; one raw
    /// slot of counts (see `dict.rs`).
    pub const DICT: u16 = 19;
    /// A raw-word buffer (dict index, set hashes).
    pub const RAW: u16 = 20;
    /// `set`: value slots keys, hashes; raw slots fill, used.
    pub const SET: u16 = 21;
    /// A Python function: name, qualname, defaults, kwdefaults, closure,
    /// globals; raw slot code index.
    pub const FUNC: u16 = 22;
    /// A closure cell.
    pub const CELL: u16 = 23;
    /// `range`: raw slots start, stop, step.
    pub const RANGE: u16 = 24;
    /// An iterator: value slot 0 the sequence; raw slots kind, a, b, c.
    pub const ITER: u16 = 25;
    /// A type object: value slot 0 the name; raw slot 1 the `T` index.
    pub const TYPE: u16 = 26;
    /// An exception: type, args, cause, context, traceback, notes; raw
    /// slot 6 flags (bit 0 suppress context).
    pub const EXC: u16 = 27;
    /// A bound builtin method: self, function.
    pub const BOUND: u16 = 28;
    /// An int outside `SmallInt` range (see `intops` for the layout).
    pub const INT: u16 = 29;
    /// `slice`: start, stop, step.
    pub const SLICE: u16 = 30;
    /// `dict_keys`/`dict_values`/`dict_items`: value slot dict; raw kind.
    pub const VIEW: u16 = 31;
    /// `enumerate`: iterator; raw slot count.
    pub const ENUMERATE: u16 = 32;
    /// `zip`: tuple of iterators.
    pub const ZIP: u16 = 33;
    /// `reversed` over a list or tuple: sequence; raw index.
    pub const REVERSED: u16 = 34;
    /// A module: name, attribute dict.
    pub const MODULE: u16 = 35;
    /// A coroutine object: function, argument registers (a `BUF`, `None`
    /// once started); raw state (see `aio::coro`).
    pub const CORO: u16 = 36;
    /// A task running a coroutine: the awaitable layout (see
    /// `aio::aw`), the coroutine in `EXTRA`.
    pub const TASK: u16 = 37;
    /// A host future: the awaitable layout, the call id in `AUX`.
    pub const FUTURE: u16 = 38;
    /// `asyncio.gather`'s future: the awaitable layout, the children tuple
    /// in `EXTRA`, `return_exceptions` in `AUX`.
    pub const GATHER: u16 = 39;
    /// `asyncio.sleep(delay, result)`: the awaitable layout, the delay in
    /// `EXTRA`, the virtual wake time's bits in `AUX`.
    pub const SLEEP: u16 = 40;
    /// `sys.stdout` / `sys.stderr`: raw slot 0 the stream (1 or 2).
    pub const STREAM: u16 = 41;
    /// A host function: name, qualname, `None` (defaults), the name host
    /// calls carry. Python sees a `function`.
    pub const HOSTFN: u16 = 42;
    /// A user class (see `class::cls` for the layout).
    pub const CLASS: u16 = 43;
    /// An instance of a user class: class, shape id or dict, overflow
    /// buffer, inline attribute slots (see `class::inst`).
    pub const INST: u16 = 44;
    /// A bound method: self, function.
    pub const METHOD: u16 = 45;
    /// A generator (see `gen::g` for the layout).
    pub const GEN: u16 = 46;
    /// `property`: fget, fset, fdel, doc.
    pub const PROPERTY: u16 = 47;
    /// `staticmethod`: function.
    pub const STATICMETHOD: u16 = 48;
    /// `classmethod`: function.
    pub const CLASSMETHOD: u16 = 49;
    /// `super`: the class, the object, the type whose MRO is searched.
    pub const SUPER: u16 = 50;
    /// `iter(callable, sentinel)`: callable, sentinel (`UNDEFINED` once
    /// exhausted).
    pub const CALLITER: u16 = 51;
    /// `bytes`: value slot 0 the `ByteRef` of the data.
    pub const BYTES: u16 = 52;
    /// `complex`: raw slots the real and imaginary parts' bits.
    pub const COMPLEX: u16 = 53;
    /// A native continuation's state (`nstep.rs`); never visible to guests.
    pub const NSTATE: u16 = 54;
    /// A traceback object: value slot 0 the exception whose traceback
    /// list it reads; raw slot 1 its entry's index there (see `exc.rs`).
    pub const TRACEBACK: u16 = 55;
    /// An async generator: the generator layout (see `generator::g`).
    pub const AGEN: u16 = 56;
    /// The awaitable `asend()`/`athrow()`/`aclose()` returns: the async
    /// generator, the value sent or the exception thrown; raw slots the
    /// mode and whether it was awaited (see `generator::ag`).
    pub const AGENOP: u16 = 57;
    /// `bytearray`: value slot 0 the `ByteRef` of its capacity block; raw
    /// slot 1 the length (see `bytearray.rs`).
    pub const BYTEARRAY: u16 = 58;
    /// `re.Match`: the pattern, the subject; raw words its spans (see
    /// `re::mt`).
    pub const MATCH: u16 = 59;
}

macro_rules! types {
    ($($id:ident = $name:literal, $base:ident;)*) => {
        /// Every Python-visible builtin type, exceptions included.
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        #[repr(u8)]
        #[allow(non_camel_case_types, reason = "Python type names")]
        pub enum T {
            $(#[doc = $name] $id,)*
        }
        /// Number of builtin types.
        pub const NTYPES: usize = [$(T::$id),*].len();
        /// All types in index order.
        pub const ALL: [T; NTYPES] = [$(T::$id),*];
        impl T {
            /// The type's `__name__`.
            #[must_use]
            #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
            pub fn name(self) -> &'static str {
                match self { $(T::$id => $name,)* }
            }
            /// The type's `__module__`.
            #[must_use]
            #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
            pub fn module(self) -> &'static str {
                match self {
                    T::TextIOWrapper => "_io",
                    T::Task | T::Future => "_asyncio",
                    T::GatheringFuture => "asyncio.tasks",
                    T::CancelledError | T::InvalidStateError => "asyncio.exceptions",
                    T::Match => "re",
                    _ => "builtins",
                }
            }
            /// The base type (`object` for roots).
            #[must_use]
            #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
            pub fn base(self) -> T {
                match self { $(T::$id => T::$base,)* }
            }
        }
    };
}

types! {
    object = "object", object;
    NoneType = "NoneType", object;
    NotImplementedType = "NotImplementedType", object;
    ellipsis = "ellipsis", object;
    int = "int", object;
    bool = "bool", int;
    float = "float", object;
    complex = "complex", object;
    str = "str", object;
    bytes = "bytes", object;
    bytearray = "bytearray", object;
    tuple = "tuple", object;
    list = "list", object;
    dict = "dict", object;
    set = "set", object;
    frozenset = "frozenset", object;
    function = "function", object;
    builtin = "builtin_function_or_method", object;
    type_ = "type", object;
    range = "range", object;
    slice = "slice", object;
    cell = "cell", object;
    list_iterator = "list_iterator", object;
    tuple_iterator = "tuple_iterator", object;
    str_iterator = "str_iterator", object;
    str_ascii_iterator = "str_ascii_iterator", object;
    bytes_iterator = "bytes_iterator", object;
    bytearray_iterator = "bytearray_iterator", object;
    range_iterator = "range_iterator", object;
    dict_keyiterator = "dict_keyiterator", object;
    dict_valueiterator = "dict_valueiterator", object;
    dict_itemiterator = "dict_itemiterator", object;
    set_iterator = "set_iterator", object;
    list_reverseiterator = "list_reverseiterator", object;
    enumerate = "enumerate", object;
    zip = "zip", object;
    dict_keys = "dict_keys", object;
    dict_values = "dict_values", object;
    dict_items = "dict_items", object;
    module = "module", object;
    coroutine = "coroutine", object;
    Task = "Task", object;
    Future = "Future", object;
    GatheringFuture = "_GatheringFuture", object;
    TextIOWrapper = "TextIOWrapper", object;
    generator = "generator", object;
    method = "method", object;
    property = "property", object;
    staticmethod = "staticmethod", object;
    classmethod = "classmethod", object;
    super_ = "super", object;
    callable_iterator = "callable_iterator", object;
    traceback = "traceback", object;
    async_generator = "async_generator", object;
    async_generator_asend = "async_generator_asend", object;
    async_generator_athrow = "async_generator_athrow", object;
    Match = "Match", object;
    BaseException = "BaseException", object;
    SystemExit = "SystemExit", BaseException;
    KeyboardInterrupt = "KeyboardInterrupt", BaseException;
    GeneratorExit = "GeneratorExit", BaseException;
    CancelledError = "CancelledError", BaseException;
    Exception = "Exception", BaseException;
    ArithmeticError = "ArithmeticError", Exception;
    FloatingPointError = "FloatingPointError", ArithmeticError;
    OverflowError = "OverflowError", ArithmeticError;
    ZeroDivisionError = "ZeroDivisionError", ArithmeticError;
    AssertionError = "AssertionError", Exception;
    AttributeError = "AttributeError", Exception;
    BufferError = "BufferError", Exception;
    EOFError = "EOFError", Exception;
    ImportError = "ImportError", Exception;
    ModuleNotFoundError = "ModuleNotFoundError", ImportError;
    LookupError = "LookupError", Exception;
    IndexError = "IndexError", LookupError;
    KeyError = "KeyError", LookupError;
    MemoryError = "MemoryError", Exception;
    NameError = "NameError", Exception;
    UnboundLocalError = "UnboundLocalError", NameError;
    OSError = "OSError", Exception;
    TimeoutError = "TimeoutError", OSError;
    ReferenceError = "ReferenceError", Exception;
    RuntimeError = "RuntimeError", Exception;
    NotImplementedError = "NotImplementedError", RuntimeError;
    RecursionError = "RecursionError", RuntimeError;
    StopAsyncIteration = "StopAsyncIteration", Exception;
    StopIteration = "StopIteration", Exception;
    SyntaxError = "SyntaxError", Exception;
    SystemError = "SystemError", Exception;
    TypeError = "TypeError", Exception;
    ValueError = "ValueError", Exception;
    UnicodeError = "UnicodeError", ValueError;
    UnicodeDecodeError = "UnicodeDecodeError", UnicodeError;
    UnicodeEncodeError = "UnicodeEncodeError", UnicodeError;
    PermissionError = "PermissionError", OSError;
    FileNotFoundError = "FileNotFoundError", OSError;
    FileExistsError = "FileExistsError", OSError;
    IsADirectoryError = "IsADirectoryError", OSError;
    NotADirectoryError = "NotADirectoryError", OSError;
    Warning = "Warning", Exception;
    DeprecationWarning = "DeprecationWarning", Warning;
    PendingDeprecationWarning = "PendingDeprecationWarning", Warning;
    RuntimeWarning = "RuntimeWarning", Warning;
    SyntaxWarning = "SyntaxWarning", Warning;
    UserWarning = "UserWarning", Warning;
    FutureWarning = "FutureWarning", Warning;
    ImportWarning = "ImportWarning", Warning;
    UnicodeWarning = "UnicodeWarning", Warning;
    BytesWarning = "BytesWarning", Warning;
    ResourceWarning = "ResourceWarning", Warning;
    EncodingWarning = "EncodingWarning", Warning;
    InvalidStateError = "InvalidStateError", Exception;
    IndentationError = "IndentationError", SyntaxError;
    TabError = "TabError", IndentationError;
}

impl T {
    /// The type with index `i`.
    #[must_use]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub fn from_index(i: usize) -> Option<T> {
        ALL.get(i).copied()
    }

    /// Is this a `BaseException` subclass?
    #[must_use]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub fn is_exception(self) -> bool {
        self as u8 >= T::BaseException as u8
    }

    /// Is `self` a subclass of `other` (or the same)?
    #[must_use]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub fn is_subclass(self, other: T) -> bool {
        let mut t = self;
        loop {
            if t == other {
                return true;
            }
            if t == T::object {
                return other == T::object;
            }
            t = t.base();
        }
    }

    /// Is the type exposed as a builtin name?
    #[must_use]
    #[cfg_attr(all(mm_shipped, not(target_arch = "wasm32")), optimize(size))]
    pub fn is_builtin_name(self) -> bool {
        matches!(
            self,
            T::object
                | T::int
                | T::bool
                | T::float
                | T::complex
                | T::str
                | T::bytes
                | T::bytearray
                | T::tuple
                | T::list
                | T::dict
                | T::set
                | T::frozenset
                | T::type_
                | T::range
                | T::slice
                | T::enumerate
                | T::zip
                | T::property
                | T::staticmethod
                | T::classmethod
                | T::super_
        ) || self.is_exception() && self.module() == "builtins"
    }
}
