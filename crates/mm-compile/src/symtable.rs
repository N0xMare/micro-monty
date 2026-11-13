//! Symbol tables: which names are local, global, nonlocal, cell or free in
//! each scope (CPython's `symtable.c` rules).
//!
//! Comprehensions get a scope of their own here, as in CPython, but the
//! compiler inlines them (PEP 709): a comprehension's direct uses of an
//! enclosing function's locals do not turn those locals into cells; only a
//! real function nested below does.
//!
//! Scopes are keyed by the address of the AST node that creates them, so the
//! compiler can emit a body more than once (a `finally` inlined on several
//! paths) and still find the right scope.

use crate::ast::{
    ClassDef, Comprehension, Expr, ExprKind, FPart, FuncDef, Module, Params, Pattern, PatternKind,
    Pos, Stmt, StmtKind,
};
use crate::error::CompileError;
use alloc::{
    borrow::Cow,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};

/// Index of a scope in [`SymTable::scopes`].
pub type ScopeId = usize;

/// What creates a scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeKind {
    /// The module.
    Module,
    /// A `def`, `lambda` or generator expression.
    Function,
    /// A list, set or dict comprehension (inlined into its parent).
    Comprehension,
    /// A class body: its bindings live in the class namespace dict.
    Class,
}

/// How a name is bound in a scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// A plain local in a register.
    Local,
    /// A local captured by an inner function: its register holds a cell.
    Cell,
    /// A variable of an enclosing function, reached through a cell.
    Free,
    /// Declared `global`.
    GlobalExplicit,
    /// Not bound in any enclosing function: a module global or builtin.
    GlobalImplicit,
}

#[derive(Clone, Copy, Debug, Default)]
struct Flags {
    assigned: bool,
    used: bool,
    param: bool,
    global: bool,
    nonlocal: bool,
    deleted: bool,
    /// Bound by an import (which, unlike other bindings, may precede a
    /// `global` declaration).
    imported: bool,
}

/// One scope's symbols.
#[derive(Clone, Debug)]
pub struct Scope {
    /// What created it.
    pub kind: ScopeKind,
    /// The function name (`<module>`, `<lambda>`, `<listcomp>`, ...).
    pub name: String,
    /// The enclosing scope.
    pub parent: Option<ScopeId>,
    /// Names in first-seen order (parameters first).
    pub names: Vec<String>,
    /// Final bindings, parallel to `names`.
    pub bindings: Vec<Binding>,
    /// Names that are deleted somewhere in the scope.
    pub deleted: Vec<String>,
    /// Number of parameters at the start of `names`.
    pub nparams: usize,
    /// The scope contains `yield` (a generator function).
    pub generator: bool,
    /// A generator expression with `async for` or `await`: an async
    /// generator.
    pub coroutine: bool,
    /// For a class scope: names it forwards from enclosing functions to the
    /// functions it contains without binding them itself.
    pub class_free: Vec<String>,
    index: BTreeMap<String, usize>,
    flags: Vec<Flags>,
    children: Vec<ScopeId>,
    /// How many comprehension iterables enclose the expression being
    /// visited in this scope (CPython's `ste_comp_iter_expr`).
    comp_iter: u32,
    /// The name of the innermost enclosing class (this one, for a class
    /// scope) without its leading underscores: private names (`__x`) in the
    /// scope are mangled with it ("" for none).
    pub private: String,
}

/// CPython's private name mangling (`_Py_Mangle`): `__x` inside class
/// `private` (its name without leading underscores) is `_private__x`;
/// dunder names, dotted names and names outside a class are kept.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn mangle<'a>(private: &str, name: &'a str) -> Cow<'a, str> {
    if private.is_empty() || !name.starts_with("__") || name.ends_with("__") || name.contains('.') {
        return Cow::Borrowed(name);
    }
    Cow::Owned(format!("_{private}{name}"))
}

impl Scope {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn new(kind: ScopeKind, name: String, parent: Option<ScopeId>) -> Scope {
        Scope {
            kind,
            name,
            parent,
            names: Vec::new(),
            bindings: Vec::new(),
            deleted: Vec::new(),
            nparams: 0,
            generator: false,
            coroutine: false,
            class_free: Vec::new(),
            index: BTreeMap::new(),
            flags: Vec::new(),
            children: Vec::new(),
            comp_iter: 0,
            private: String::new(),
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn slot(&mut self, name: &str) -> usize {
        let name = &*mangle(&self.private, name);
        if let Some(&i) = self.index.get(name) {
            return i;
        }
        let i = self.names.len();
        self.names.push(name.to_string());
        self.flags.push(Flags::default());
        self.bindings.push(Binding::GlobalImplicit);
        self.index.insert(name.to_string(), i);
        i
    }

    /// The binding of `name`, if the scope mentions it.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn binding(&self, name: &str) -> Option<Binding> {
        self.position(name).map(|i| self.bindings[i])
    }

    /// Index of `name` in `names`.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn position(&self, name: &str) -> Option<usize> {
        self.index.get(&*mangle(&self.private, name)).copied()
    }

    /// Is `name` deleted anywhere in this scope?
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn is_deleted(&self, name: &str) -> bool {
        self.position(name)
            .is_some_and(|i| self.flags.get(i).is_some_and(|f| f.deleted))
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn binds_locally(&self, name: &str) -> bool {
        matches!(self.binding(name), Some(Binding::Local | Binding::Cell))
    }
}

/// Every scope of a module.
#[derive(Clone, Debug)]
pub struct SymTable {
    /// Scope 0 is the module.
    pub scopes: Vec<Scope>,
    by_node: BTreeMap<usize, ScopeId>,
}

/// Key of the scope created by a `def`/`lambda`.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn func_key(f: &FuncDef) -> usize {
    core::ptr::from_ref(f) as usize
}

/// Key of the scope created by a `class` statement.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn class_key(c: &ClassDef) -> usize {
    core::ptr::from_ref(c) as usize
}

/// Key of the scope created by a comprehension or generator expression.
#[must_use]
#[cfg_attr(mm_shipped, optimize(size))]
pub fn expr_key(e: &Expr) -> usize {
    core::ptr::from_ref(e) as usize
}

impl SymTable {
    /// The scope created by the node with this key.
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn scope_of(&self, key: usize) -> Option<ScopeId> {
        self.by_node.get(&key).copied()
    }

    /// The first scope named `name` (for tests and tools).
    #[must_use]
    #[cfg_attr(mm_shipped, optimize(size))]
    pub fn find(&self, name: &str) -> Option<&Scope> {
        self.scopes.iter().find(|s| s.name == name)
    }
}

/// Builds and analyses the symbol table of `module`.
#[cfg_attr(mm_shipped, optimize(size))]
pub fn build(module: &Module) -> Result<SymTable, CompileError> {
    let mut b = Builder {
        scopes: alloc::vec![Scope::new(ScopeKind::Module, "<module>".to_string(), None)],
        by_node: BTreeMap::new(),
        cur: 0,
    };
    for s in &module.body {
        b.stmt(s)?;
    }
    let mut t = SymTable {
        scopes: b.scopes,
        by_node: b.by_node,
    };
    analyze(&mut t, 0, &[])?;
    Ok(t)
}

struct Builder {
    scopes: Vec<Scope>,
    by_node: BTreeMap<usize, ScopeId>,
    cur: ScopeId,
}

#[cfg_attr(mm_shipped, optimize(size))]
fn err(msg: String, pos: Pos) -> CompileError {
    CompileError::syntax(msg, pos)
}

impl Builder {
    #[cfg_attr(mm_shipped, optimize(size))]
    fn scope(&mut self) -> &mut Scope {
        &mut self.scopes[self.cur]
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn def_name(&mut self, name: &str, pos: Pos) -> Result<(), CompileError> {
        let s = self.scope();
        let i = s.slot(name);
        let f = s.flags[i];
        if f.param && (f.global || f.nonlocal) {
            return Err(err(format!("name '{name}' is parameter and global"), pos));
        }
        s.flags[i].assigned = true;
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn def_import(&mut self, name: &str, pos: Pos) -> Result<(), CompileError> {
        let s = self.scope();
        let i = s.slot(name);
        let was = s.flags[i].assigned;
        self.def_name(name, pos)?;
        let f = &mut self.scope().flags[i];
        f.assigned = was;
        f.imported = true;
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn use_name(&mut self, name: &str) {
        let s = self.scope();
        let i = s.slot(name);
        s.flags[i].used = true;
    }

    /// A walrus target binds in the nearest enclosing non-comprehension scope.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn walrus_target(&mut self, name: &str, pos: Pos) -> Result<(), CompileError> {
        let mut id = self.cur;
        while self.scopes[id].kind == ScopeKind::Comprehension {
            let s = &mut self.scopes[id];
            let i = s.slot(name);
            if s.flags[i].assigned {
                return Err(err(
                    format!(
                        "assignment expression cannot rebind comprehension iteration variable '{name}'"
                    ),
                    pos,
                ));
            }
            s.flags[i].used = true;
            s.flags[i].nonlocal = false;
            match s.parent {
                Some(p) => id = p,
                None => break,
            }
        }
        let saved = self.cur;
        self.cur = id;
        let r = self.def_name(name, pos);
        self.cur = saved;
        r
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn push(&mut self, kind: ScopeKind, name: &str, key: usize) -> ScopeId {
        let id = self.scopes.len();
        let mut s = Scope::new(kind, name.to_string(), Some(self.cur));
        // A scope inside a comprehension iterable is still inside it.
        s.comp_iter = self.scopes[self.cur].comp_iter;
        s.private = if kind == ScopeKind::Class {
            name.trim_start_matches('_').to_string()
        } else {
            self.scopes[self.cur].private.clone()
        };
        self.scopes.push(s);
        self.scopes[self.cur].children.push(id);
        self.by_node.insert(key, id);
        self.cur = id;
        id
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn body(&mut self, b: &[Stmt]) -> Result<(), CompileError> {
        for s in b {
            self.stmt(s)?;
        }
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn stmt(&mut self, s: &Stmt) -> Result<(), CompileError> {
        match &s.kind {
            StmtKind::Expr(e) => self.expr(e)?,
            StmtKind::Assign(targets, value) => {
                self.expr(value)?;
                for t in targets {
                    self.target(t)?;
                }
            }
            StmtKind::AugAssign(t, _, v) => {
                // The target is read and written.
                if let ExprKind::Name(n) = &t.kind {
                    self.use_name(n);
                }
                self.target(t)?;
                self.expr(v)?;
            }
            StmtKind::AnnAssign(t, v, _) => {
                if let Some(v) = v {
                    self.expr(v)?;
                    self.target(t)?;
                } else if let ExprKind::Name(n) = &t.kind {
                    // `x: int` alone makes x local without binding it.
                    self.def_name(n, t.pos)?;
                } else {
                    self.target(t)?;
                }
            }
            StmtKind::If(..) => {
                // An `elif` chain in a loop (see the compiler).
                let mut cur = s;
                while let StmtKind::If(test, body, orelse) = &cur.kind {
                    self.expr(test)?;
                    self.body(body)?;
                    let [
                        next @ Stmt {
                            kind: StmtKind::If(..),
                            ..
                        },
                    ] = orelse.as_slice()
                    else {
                        self.body(orelse)?;
                        break;
                    };
                    cur = next;
                }
            }
            StmtKind::While(test, body, orelse) => {
                self.expr(test)?;
                self.body(body)?;
                self.body(orelse)?;
            }
            StmtKind::For(target, iter, body, orelse, _) => {
                self.expr(iter)?;
                self.target(target)?;
                self.body(body)?;
                self.body(orelse)?;
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    self.expr(v)?;
                }
            }
            StmtKind::FunctionDef(f) => {
                for d in &f.decorators {
                    self.expr(d)?;
                }
                self.func(f)?;
                self.def_name(&f.name, f.pos)?;
            }
            StmtKind::Delete(targets) => {
                for t in targets {
                    self.del_target(t)?;
                }
            }
            StmtKind::Global(names) => {
                for n in names {
                    self.declare(n, true, s.pos)?;
                }
            }
            StmtKind::Nonlocal(names) => {
                if self.scopes[self.cur].kind == ScopeKind::Module {
                    return Err(err(
                        "nonlocal declaration not allowed at module level".to_string(),
                        s.pos,
                    ));
                }
                for n in names {
                    self.declare(n, false, s.pos)?;
                }
            }
            StmtKind::Raise(e, c) => {
                if let Some(e) = e {
                    self.expr(e)?;
                }
                if let Some(c) = c {
                    self.expr(c)?;
                }
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            } => {
                self.body(body)?;
                for h in handlers {
                    if let Some(t) = &h.typ {
                        self.expr(t)?;
                    }
                    if let Some(n) = &h.name {
                        self.def_name(n, h.pos)?;
                        let s = self.scope();
                        let i = s.slot(n);
                        s.flags[i].deleted = true;
                    }
                    self.body(&h.body)?;
                }
                self.body(orelse)?;
                self.body(finalbody)?;
            }
            StmtKind::Assert(t, m) => {
                self.expr(t)?;
                if let Some(m) = m {
                    self.expr(m)?;
                }
            }
            StmtKind::Import(names) => {
                for a in names {
                    // `import a.b` binds `a`; `import a.b as c` binds `c`.
                    let bound = a
                        .asname
                        .as_deref()
                        .unwrap_or_else(|| a.name.split('.').next().unwrap_or(&a.name));
                    self.def_import(bound, s.pos)?;
                }
            }
            StmtKind::ImportFrom(_, names) => {
                for a in names {
                    if a.name == "*" {
                        if self.scopes[self.cur].kind != ScopeKind::Module {
                            // The `*` ends the statement.
                            let pos = Pos {
                                line: s.pos.end_line,
                                col: s.pos.end_col.saturating_sub(1),
                                ..s.pos
                            };
                            let msg = "import * only allowed at module level".into();
                            return Err(CompileError::syntax(msg, pos));
                        }
                        continue;
                    }
                    self.def_import(a.asname.as_deref().unwrap_or(&a.name), s.pos)?;
                }
            }
            StmtKind::ClassDef(c) => {
                for d in &c.decorators {
                    self.expr(d)?;
                }
                for b in &c.bases {
                    self.expr(b)?;
                }
                for k in &c.keywords {
                    self.expr(&k.value)?;
                }
                let saved = self.cur;
                self.push(ScopeKind::Class, &c.name, class_key(c));
                // The namespace dict arrives as the body's one parameter.
                let s = self.scope();
                let i = s.slot(".ns");
                s.flags[i].param = true;
                s.flags[i].assigned = true;
                s.nparams = 1;
                self.body(&c.body)?;
                self.cur = saved;
                self.def_name(&c.name, c.pos)?;
            }
            StmtKind::With(items, body, _) => {
                for it in items {
                    self.expr(&it.ctx)?;
                    if let Some(t) = &it.target {
                        self.target(t)?;
                    }
                }
                self.body(body)?;
            }
            StmtKind::Match(subject, cases) => {
                self.expr(subject)?;
                for c in cases {
                    self.pattern(&c.pattern)?;
                    if let Some(g) = &c.guard {
                        self.expr(g)?;
                    }
                    self.body(&c.body)?;
                }
            }
        }
        Ok(())
    }

    /// The names a pattern binds and the expressions it evaluates.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn pattern(&mut self, p: &Pattern) -> Result<(), CompileError> {
        match &p.kind {
            PatternKind::Value(e) => self.expr(e)?,
            PatternKind::Singleton(_) => {}
            PatternKind::Sequence(items) | PatternKind::Or(items) => {
                for x in items {
                    self.pattern(x)?;
                }
            }
            PatternKind::Star(name) => {
                if let Some(n) = name {
                    self.def_name(n, p.pos)?;
                }
            }
            PatternKind::Mapping {
                keys,
                patterns,
                rest,
            } => {
                for (k, x) in keys.iter().zip(patterns) {
                    self.expr(k)?;
                    self.pattern(x)?;
                }
                if let Some(n) = rest {
                    self.def_name(n, p.pos)?;
                }
            }
            PatternKind::Class {
                cls,
                patterns,
                kwd_patterns,
                ..
            } => {
                self.expr(cls)?;
                for x in patterns.iter().chain(kwd_patterns) {
                    self.pattern(x)?;
                }
            }
            PatternKind::As(inner, name) => {
                if let Some(x) = inner {
                    self.pattern(x)?;
                }
                if let Some(n) = name {
                    self.def_name(n, p.pos)?;
                }
            }
        }
        Ok(())
    }

    /// Records an `await` or `async for` in the current scope: the
    /// generator expression it runs in (through inlined comprehensions)
    /// becomes an async generator.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn async_here(&mut self) {
        let mut id = self.cur;
        while self.scopes[id].kind == ScopeKind::Comprehension
            && let Some(p) = self.scopes[id].parent
        {
            id = p;
        }
        let s = &mut self.scopes[id];
        if s.kind == ScopeKind::Function && s.name == "<genexpr>" {
            s.coroutine = true;
        }
    }

    /// Records a `yield` in the current scope.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn yield_here(&mut self, pos: Pos) -> Result<(), CompileError> {
        let s = &self.scopes[self.cur];
        match s.kind {
            ScopeKind::Module | ScopeKind::Class => {
                Err(err("'yield' outside function".to_string(), pos))
            }
            ScopeKind::Comprehension => {
                let what = match s.name.as_str() {
                    "<setcomp>" => "set comprehension",
                    "<dictcomp>" => "dict comprehension",
                    _ => "list comprehension",
                };
                Err(err(format!("'yield' inside {what}"), pos))
            }
            ScopeKind::Function if s.name == "<genexpr>" => {
                Err(err("'yield' inside generator expression".to_string(), pos))
            }
            ScopeKind::Function => {
                self.scope().generator = true;
                Ok(())
            }
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn declare(&mut self, n: &str, global: bool, pos: Pos) -> Result<(), CompileError> {
        let kind = if global { "global" } else { "nonlocal" };
        let s = self.scope();
        let i = s.slot(n);
        let f = s.flags[i];
        if f.param {
            return Err(err(format!("name '{n}' is parameter and {kind}"), pos));
        }
        if global && f.nonlocal {
            return Err(err(format!("name '{n}' is nonlocal and global"), pos));
        }
        if !global && f.global {
            return Err(err(format!("name '{n}' is nonlocal and global"), pos));
        }
        if f.assigned {
            return Err(err(
                format!("name '{n}' is assigned to before {kind} declaration"),
                pos,
            ));
        }
        if f.used {
            return Err(err(
                format!("name '{n}' is used prior to {kind} declaration"),
                pos,
            ));
        }
        if global {
            s.flags[i].global = true;
        } else {
            s.flags[i].nonlocal = true;
        }
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn params(&mut self, p: &Params, pos: Pos) -> Result<(), CompileError> {
        let mut names: Vec<&str> = Vec::new();
        for a in p.posonly.iter().chain(&p.args) {
            names.push(&a.name);
        }
        for a in &p.kwonly {
            names.push(&a.name);
        }
        if let Some(v) = &p.vararg {
            names.push(v);
        }
        if let Some(k) = &p.kwarg {
            names.push(k);
        }
        for n in names {
            let s = self.scope();
            if s.position(n).is_some() {
                return Err(err(
                    format!("duplicate argument '{n}' in function definition"),
                    pos,
                ));
            }
            let i = s.slot(n);
            s.flags[i].param = true;
            s.flags[i].assigned = true;
            s.nparams += 1;
        }
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn func(&mut self, f: &FuncDef) -> Result<(), CompileError> {
        for a in f.params.posonly.iter().chain(&f.params.args) {
            if let Some(d) = &a.default {
                self.expr(d)?;
            }
        }
        for a in &f.params.kwonly {
            if let Some(d) = &a.default {
                self.expr(d)?;
            }
        }
        let saved = self.cur;
        self.push(ScopeKind::Function, &f.name, func_key(f));
        self.params(&f.params, f.pos)?;
        self.body(&f.body)?;
        self.cur = saved;
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn comp(
        &mut self,
        e: &Expr,
        kind: ScopeKind,
        name: &str,
        gens: &[Comprehension],
        elts: &[&Expr],
    ) -> Result<(), CompileError> {
        let Some(first) = gens.first() else {
            return Ok(());
        };
        self.comp_iter(&first.iter)?;
        let saved = self.cur;
        self.push(kind, name, expr_key(e));
        if kind == ScopeKind::Function {
            // The generator's hidden iterator parameter.
            let s = self.scope();
            let i = s.slot(".0");
            s.flags[i].param = true;
            s.flags[i].assigned = true;
            s.nparams = 1;
        }
        for (k, g) in gens.iter().enumerate() {
            if k > 0 {
                self.comp_iter(&g.iter)?;
            }
            if g.is_async {
                self.async_here();
            }
            self.target(&g.target)?;
            for c in &g.ifs {
                self.expr(c)?;
            }
        }
        for x in elts {
            self.expr(x)?;
        }
        self.cur = saved;
        Ok(())
    }

    /// A comprehension's iterable, where a walrus is a SyntaxError.
    #[cfg_attr(mm_shipped, optimize(size))]
    fn comp_iter(&mut self, e: &Expr) -> Result<(), CompileError> {
        self.scope().comp_iter += 1;
        let r = self.expr(e);
        self.scope().comp_iter -= 1;
        r
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn target(&mut self, t: &Expr) -> Result<(), CompileError> {
        match &t.kind {
            ExprKind::Name(n) => self.def_name(n, t.pos),
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for i in items {
                    self.target(i)?;
                }
                Ok(())
            }
            ExprKind::Starred(inner) => self.target(inner),
            ExprKind::Attribute(v, _) => self.expr(v),
            ExprKind::Subscript(v, i) => {
                self.expr(v)?;
                self.expr(i)
            }
            _ => self.expr(t),
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn del_target(&mut self, t: &Expr) -> Result<(), CompileError> {
        match &t.kind {
            ExprKind::Name(n) => {
                self.def_name(n, t.pos)?;
                let s = self.scope();
                let i = s.slot(n);
                s.flags[i].deleted = true;
                Ok(())
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for i in items {
                    self.del_target(i)?;
                }
                Ok(())
            }
            _ => self.target(t),
        }
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn fparts(&mut self, parts: &[FPart]) -> Result<(), CompileError> {
        for p in parts {
            if let FPart::Field { expr, spec, .. } = p {
                self.expr(expr)?;
                if let Some(s) = spec {
                    self.fparts(s)?;
                }
            }
        }
        Ok(())
    }

    #[cfg_attr(mm_shipped, optimize(size))]
    fn expr(&mut self, e: &Expr) -> Result<(), CompileError> {
        // A left-nested chain (`a + b + c`, `x.f(1).g`): its innermost
        // operand, then each step's other operands, in a loop.
        if e.chain_left().and_then(Expr::chain_left).is_some() {
            let mut steps = alloc::vec![e];
            let mut cur = e;
            while let Some(c) = cur.chain_left() {
                steps.push(c);
                cur = c;
            }
            steps.pop();
            self.expr(cur)?;
            for s in steps.into_iter().rev() {
                self.chain_rest(s)?;
            }
            return Ok(());
        }
        match &e.kind {
            ExprKind::Const(_) => {}
            ExprKind::Name(n) => {
                self.use_name(n);
                // Zero-argument `super()` reads the implicit `__class__` cell.
                if n == "super"
                    && matches!(
                        self.scopes[self.cur].kind,
                        ScopeKind::Function | ScopeKind::Comprehension
                    )
                {
                    self.use_name("__class__");
                }
            }
            ExprKind::BinOp(a, ..) | ExprKind::Subscript(a, _) | ExprKind::Attribute(a, _) => {
                self.expr(a)?;
                self.chain_rest(e)?;
            }
            ExprKind::Unary(_, a) | ExprKind::Starred(a) => {
                self.expr(a)?;
            }
            ExprKind::BoolOp(_, items)
            | ExprKind::List(items)
            | ExprKind::Tuple(items)
            | ExprKind::Set(items) => {
                for i in items {
                    self.expr(i)?;
                }
            }
            ExprKind::Compare(a, rest) => {
                self.expr(a)?;
                for (_, b) in rest {
                    self.expr(b)?;
                }
            }
            ExprKind::IfExp(..) => {
                let mut cur = e;
                while let ExprKind::IfExp(t, b, o) = &cur.kind {
                    self.expr(t)?;
                    self.expr(b)?;
                    cur = o;
                }
                self.expr(cur)?;
            }
            ExprKind::Lambda(f) => self.func(f)?,
            ExprKind::Call(f, ..) => {
                self.expr(f)?;
                self.chain_rest(e)?;
            }
            ExprKind::Slice(a, b, c) => {
                for x in [a, b, c].into_iter().flatten() {
                    self.expr(x)?;
                }
            }
            ExprKind::Dict(items) => {
                for (k, v) in items {
                    if let Some(k) = k {
                        self.expr(k)?;
                    }
                    self.expr(v)?;
                }
            }
            ExprKind::ListComp(elt, gens) => {
                self.comp(e, ScopeKind::Comprehension, "<listcomp>", gens, &[elt])?;
            }
            ExprKind::SetComp(elt, gens) => {
                self.comp(e, ScopeKind::Comprehension, "<setcomp>", gens, &[elt])?;
            }
            ExprKind::DictComp(k, v, gens) => {
                self.comp(e, ScopeKind::Comprehension, "<dictcomp>", gens, &[k, v])?;
            }
            ExprKind::GenExp(elt, gens) => {
                self.comp(e, ScopeKind::Function, "<genexpr>", gens, &[elt])?;
            }
            ExprKind::FString(parts) => self.fparts(parts)?,
            ExprKind::Await(v) => {
                self.expr(v)?;
                self.async_here();
            }
            ExprKind::Yield(v) => {
                if let Some(v) = v {
                    self.expr(v)?;
                }
                self.yield_here(e.pos)?;
            }
            ExprKind::YieldFrom(v) => {
                self.expr(v)?;
                self.yield_here(e.pos)?;
            }
            ExprKind::Named(n, v) => {
                if self.scope().comp_iter > 0 {
                    return Err(err(
                        "assignment expression cannot be used in a comprehension iterable expression"
                            .into(),
                        e.pos,
                    ));
                }
                self.expr(v)?;
                self.walrus_target(n, e.pos)?;
            }
        }
        Ok(())
    }
}

impl Builder {
    /// The operands of chain step `e` after its first ([`Expr::chain_left`]).
    #[cfg_attr(mm_shipped, optimize(size))]
    fn chain_rest(&mut self, e: &Expr) -> Result<(), CompileError> {
        match &e.kind {
            ExprKind::BinOp(_, _, b) | ExprKind::Subscript(_, b) => self.expr(b),
            ExprKind::Call(_, args, kws) => {
                for a in args {
                    self.expr(a)?;
                }
                for k in kws {
                    self.expr(&k.value)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// A name a scope needs from its enclosing scopes, and whether a real
/// function below needs it as a cell (as opposed to an inlined comprehension
/// reading it directly).
struct Need {
    name: String,
    cell: bool,
}

/// Resolves every name of scope `id`. `visible` lists the names bound by
/// enclosing function scopes (innermost last). Returns the names `id` needs
/// from outside.
#[cfg_attr(mm_shipped, optimize(size))]
fn analyze(t: &mut SymTable, id: ScopeId, visible: &[String]) -> Result<Vec<Need>, CompileError> {
    let kind = t.scopes[id].kind;
    let mut needs: Vec<Need> = Vec::new();
    {
        let s = &mut t.scopes[id];
        for i in 0..s.names.len() {
            let f = s.flags[i];
            if f.deleted {
                s.deleted.push(s.names[i].clone());
            }
            let b = if kind == ScopeKind::Module {
                if f.global {
                    Binding::GlobalExplicit
                } else {
                    Binding::GlobalImplicit
                }
            } else if f.global {
                Binding::GlobalExplicit
            } else if f.nonlocal {
                if !visible.iter().any(|v| v == &s.names[i]) {
                    return Err(CompileError::syntax(
                        format!("no binding for nonlocal '{}' found", s.names[i]),
                        Pos::default(),
                    ));
                }
                needs.push(Need {
                    name: s.names[i].clone(),
                    cell: true,
                });
                Binding::Free
            } else if f.assigned || f.param || f.imported {
                Binding::Local
            } else if visible.iter().any(|v| v == &s.names[i]) {
                needs.push(Need {
                    name: s.names[i].clone(),
                    cell: kind != ScopeKind::Comprehension,
                });
                Binding::Free
            } else {
                Binding::GlobalImplicit
            };
            s.bindings[i] = b;
        }
    }
    // Names visible to children: everything bound locally here, plus what is
    // visible from outside (module names are globals, not closure variables).
    // A class body's own names are not visible to the functions inside it;
    // its implicit `__class__` cell is.
    let mut child_visible: Vec<String> = Vec::new();
    if kind == ScopeKind::Class {
        child_visible.extend(visible.iter().cloned());
        child_visible.push("__class__".to_string());
    } else if kind != ScopeKind::Module {
        child_visible.extend(visible.iter().cloned());
        let s = &t.scopes[id];
        for (i, n) in s.names.iter().enumerate() {
            match s.bindings[i] {
                Binding::Local | Binding::Cell => child_visible.push(n.clone()),
                // `global x` hides an enclosing `x` from the scopes inside.
                Binding::GlobalExplicit => child_visible.retain(|v| v != n),
                _ => {}
            }
        }
    }
    let children = t.scopes[id].children.clone();
    for c in children {
        let child_needs = analyze(t, c, &child_visible)?;
        let s = &mut t.scopes[id];
        for need in child_needs {
            if kind == ScopeKind::Class {
                if need.name == "__class__" {
                    let i = s.slot("__class__");
                    s.bindings[i] = Binding::Cell;
                } else if s.binding(&need.name) == Some(Binding::Free) {
                    // Already a free variable of the body itself.
                } else if !s.class_free.contains(&need.name) {
                    s.class_free.push(need.name.clone());
                    needs.push(Need {
                        name: need.name,
                        cell: true,
                    });
                }
                continue;
            }
            if s.binds_locally(&need.name) {
                if need.cell {
                    let i = s.slot(&need.name);
                    s.bindings[i] = Binding::Cell;
                }
            } else if kind != ScopeKind::Module {
                let explicit_global = s
                    .binding(&need.name)
                    .is_some_and(|b| b == Binding::GlobalExplicit);
                if explicit_global {
                    continue;
                }
                let i = s.slot(&need.name);
                let was_free = s.bindings[i] == Binding::Free;
                s.bindings[i] = Binding::Free;
                let cell = need.cell || kind != ScopeKind::Comprehension;
                if let Some(n) = needs.iter_mut().find(|n| n.name == need.name) {
                    n.cell |= cell;
                } else if !was_free || cell {
                    needs.push(Need {
                        name: need.name,
                        cell,
                    });
                }
            }
        }
    }
    Ok(needs)
}
