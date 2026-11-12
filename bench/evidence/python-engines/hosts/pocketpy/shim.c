// PocketPy 2.2.0 behind a small C API for the Rust host (src/main.rs). A
// session is one PocketPy VM (py_switchvm): PocketPy allows 16 per process.
// Built from the release's amalgamated pocketpy.c with PK_ENABLE_OS=0 (no
// os/io/sockets/dll) and PK_ENABLE_WATCHDOG=1 (build.rs).

#include <string.h>
#include <stdlib.h>

#include "pocketpy.h"

typedef const char *(*host_call_fn)(const char *name, size_t name_len, const char *args, size_t args_len,
                                    size_t *out_len, int *is_err);
typedef long long (*host_sync_fn)(long long x);
typedef void (*host_out_fn)(const char *s, size_t len);
static host_call_fn g_call;
static host_sync_fn g_sync;
static host_out_fn g_out;
static int g_used[16];
static char *g_err;

static void on_print(const char *s) {
    g_out(s, strlen(s));
}

static bool hsync(int argc, py_StackRef argv) {
    PY_CHECK_ARGC(1);
    py_newint(py_retval(), g_sync(py_toint(py_arg(0))));
    return true;
}

// _hc(name, json_args) -> (True, json_reply) | (False, type, message)
static bool hc(int argc, py_StackRef argv) {
    PY_CHECK_ARGC(2);
    int nl, al;
    const char *n = py_tostrn(py_arg(0), &nl);
    const char *a = py_tostrn(py_arg(1), &al);
    size_t ol;
    int is_err = 0;
    const char *r = g_call(n, (size_t)nl, a, (size_t)al, &ol, &is_err);
    if (!is_err) {
        py_newtuple(py_retval(), 2);
        py_newbool(py_tuple_getitem(py_retval(), 0), true);
        py_newstrv(py_tuple_getitem(py_retval(), 1), (c11_sv){r, (int)ol});
    } else {
        size_t tl = strlen(r);
        py_newtuple(py_retval(), 3);
        py_newbool(py_tuple_getitem(py_retval(), 0), false);
        py_newstrv(py_tuple_getitem(py_retval(), 1), (c11_sv){r, (int)tl});
        py_newstrv(py_tuple_getitem(py_retval(), 2), (c11_sv){r + tl + 1, (int)(ol - tl - 1)});
    }
    return true;
}

void host_pk_init(host_call_fn call, host_sync_fn sync, host_out_fn out) {
    g_call = call;
    g_sync = sync;
    g_out = out;
    py_initialize();
}

// A new VM, or -1 when all 16 are in use.
int host_pk_new_session(void) {
    for (int i = 0; i < 16; i++) {
        if (g_used[i] == 1) continue;
        py_switchvm(i);
        if (g_used[i] == 2) py_resetvm();  // used before: rebuild it
        g_used[i] = 1;
        py_callbacks()->print = on_print;
        py_GlobalRef main = py_getmodule("__main__");
        py_bindfunc(main, "hsync", hsync);
        py_bindfunc(main, "_hc", hc);
        return i;
    }
    return -1;
}

void host_pk_drop_session(int i) {
    g_used[i] = 2;
}

// Runs `src` in VM `i`'s __main__. Returns 0, or 1 with the formatted
// exception in *err (valid until the next call).
int host_pk_exec(int i, const char *src, const char **err) {
    py_switchvm(i);
    py_StackRef p0 = py_peek(0);
    if (py_exec(src, "cell.py", EXEC_MODE, NULL)) return 0;
    free(g_err);
    g_err = py_formatexc();
    py_clearexc(p0);
    *err = g_err ? g_err : "";
    return 1;
}

// The watchdog: `ms` of CPU time from now (0: at the VM's next step), or off.
void host_pk_watchdog(int i, long long ms) {
    py_switchvm(i);
    if (ms < 0) py_watchdog_end();
    else py_watchdog_begin(ms);
}

// From another thread, no VM switch: fire the running VM's watchdog now.
// PocketPy has no thread-safe interrupt; with threads off its current VM is a
// plain global, so this is the same store py_watchdog_begin(0) makes.
void host_pk_interrupt(void) {
    py_watchdog_begin(0);
}
