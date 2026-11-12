//! Symbol-table analysis on tricky scoping inputs: local, global, nonlocal,
//! cell and free bindings must match `CPython`'s `symtable` module.

#![allow(clippy::expect_used, clippy::panic, reason = "tests")]

use mm_compile::symtable::{Binding, Scope, SymTable, build};

fn table(src: &str) -> SymTable {
    let module = mm_parse::parse(src).expect("parses");
    build(&module).expect("analyses")
}

fn scope<'a>(t: &'a SymTable, name: &str) -> &'a Scope {
    t.find(name)
        .unwrap_or_else(|| panic!("no scope named {name}"))
}

fn binding(t: &SymTable, scope_name: &str, var: &str) -> Option<Binding> {
    scope(t, scope_name).binding(var)
}

#[test]
fn assignment_anywhere_makes_a_local() {
    let t = table("x = 1\ndef f():\n    print(x)\n    x = 2\n");
    assert_eq!(binding(&t, "f", "x"), Some(Binding::Local));
    assert_eq!(binding(&t, "f", "print"), Some(Binding::GlobalImplicit));
    // Augmented assignment, for targets, del and except names also bind.
    let t = table(
        "def g():\n    a += 1\n    for b in c:\n        pass\n    del d\n    try:\n        pass\n    except E as e:\n        pass\n",
    );
    for v in ["a", "b", "d", "e"] {
        assert_eq!(binding(&t, "g", v), Some(Binding::Local), "{v}");
    }
    assert_eq!(binding(&t, "g", "c"), Some(Binding::GlobalImplicit));
}

#[test]
fn closures_make_cells_and_frees() {
    let t =
        table("def outer():\n    n = 0\n    def inner():\n        return n\n    return inner\n");
    assert_eq!(binding(&t, "outer", "n"), Some(Binding::Cell));
    assert_eq!(binding(&t, "inner", "n"), Some(Binding::Free));
    assert_eq!(binding(&t, "outer", "inner"), Some(Binding::Local));
}

#[test]
fn free_variables_pass_through_intermediate_scopes() {
    let t = table(
        "def a():\n    v = 1\n    def b():\n        def c():\n            return v\n        return c\n    return b\n",
    );
    assert_eq!(binding(&t, "a", "v"), Some(Binding::Cell));
    // `b` never mentions `v` but must carry it to `c`.
    assert_eq!(binding(&t, "b", "v"), Some(Binding::Free));
    assert_eq!(binding(&t, "c", "v"), Some(Binding::Free));
}

#[test]
fn nonlocal_rebinding_and_global_declarations() {
    let t = table(
        "x = 0\ndef f():\n    x = 1\n    def g():\n        nonlocal x\n        x = 2\n        def h():\n            global x\n            x = 3\n        return h\n    return g\n",
    );
    assert_eq!(binding(&t, "f", "x"), Some(Binding::Cell));
    assert_eq!(binding(&t, "g", "x"), Some(Binding::Free));
    assert_eq!(binding(&t, "h", "x"), Some(Binding::GlobalExplicit));
}

#[test]
fn global_skips_function_scopes_for_inner_functions() {
    // `inner` reads `x`; `outer` declared it global, so it is not a closure.
    let t = table(
        "def outer():\n    global x\n    x = 1\n    def inner():\n        return x\n    return inner\n",
    );
    assert_eq!(binding(&t, "outer", "x"), Some(Binding::GlobalExplicit));
    assert_eq!(binding(&t, "inner", "x"), Some(Binding::GlobalImplicit));
}

#[test]
fn parameters_are_locals_or_cells() {
    let t = table(
        "def f(a, b=1, *args, c, **kw):\n    def g():\n        return b + kw\n    return a\n",
    );
    let f = scope(&t, "f");
    assert_eq!(f.nparams, 5);
    assert_eq!(&f.names[..5], ["a", "b", "c", "args", "kw"]);
    assert_eq!(f.binding("a"), Some(Binding::Local));
    assert_eq!(f.binding("b"), Some(Binding::Cell));
    assert_eq!(f.binding("kw"), Some(Binding::Cell));
}

#[test]
fn comprehensions_are_inlined_and_capture_correctly() {
    // The iteration variable is the comprehension's own; the outer `n` is
    // read directly (PEP 709), with no cell.
    let t = table("def f(n):\n    return [i * n for i in range(n)]\n");
    assert_eq!(binding(&t, "f", "n"), Some(Binding::Local));
    assert_eq!(binding(&t, "f", "i"), None);
    // A lambda inside the comprehension captures the comprehension variable.
    let t = table("def f():\n    return [lambda: i for i in range(3)]\n");
    assert_eq!(binding(&t, "<lambda>", "i"), Some(Binding::Free));
    // The walrus binds in the enclosing function.
    let t = table("def f(xs):\n    [y := x for x in xs]\n    return y\n");
    assert_eq!(binding(&t, "f", "y"), Some(Binding::Local));
}

#[test]
fn module_level_names_are_global() {
    let t = table("a = 1\nfor b in range(2):\n    pass\ndef c():\n    pass\n");
    for v in ["a", "b", "c"] {
        assert!(
            matches!(
                binding(&t, "<module>", v),
                Some(Binding::Local | Binding::GlobalImplicit)
            ),
            "{v}"
        );
    }
}

#[test]
fn scoping_errors_are_syntax_errors() {
    for (src, msg) in [
        (
            "def f():\n    x = 1\n    global x\n",
            "name 'x' is assigned to before global declaration",
        ),
        (
            "def f():\n    nonlocal y\n",
            "no binding for nonlocal 'y' found",
        ),
        (
            "nonlocal z\n",
            "nonlocal declaration not allowed at module level",
        ),
        (
            "def f(a):\n    global a\n",
            "name 'a' is parameter and global",
        ),
    ] {
        let module = mm_parse::parse(src).expect("parses");
        let err = build(&module).expect_err(src);
        assert_eq!(err.kind, "SyntaxError", "{src}");
        assert_eq!(err.msg, msg, "{src}");
    }
}
