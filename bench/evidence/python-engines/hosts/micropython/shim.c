// MicroPython (unix port, sandbox variant) behind a small C API that the Rust
// host (src/main.rs) drives. One interpreter per process: MicroPython keeps its
// state in the global `mp_state_ctx`, so a "session" is a fresh globals dict in
// that interpreter, kept alive in sys.modules (a GC root) until dropped.
//
// Compiled by build.rs with the port's own CFLAGS (build.sh writes them) and
// linked with libmicropython.a (without the CLI's main.o).

#include <stdio.h>
#include <string.h>

#include "py/builtin.h"
#include "py/compile.h"
#include "py/cstack.h"
#include "py/gc.h"
#include "py/lexer.h"
#include "py/mperrno.h"
#include "py/nlr.h"
#include "py/objstr.h"
#include "py/runtime.h"
#include "py/stackctrl.h"

// Callbacks into the Rust host, set by mpy_init.
typedef const char *(*host_call_fn)(const char *name, size_t name_len, const char *args, size_t args_len,
                                    size_t *out_len, int *is_err);
typedef long long (*host_sync_fn)(long long x);
typedef void (*host_out_fn)(const char *s, size_t len);
static host_call_fn g_call;
static host_sync_fn g_sync;
static host_out_fn g_out;
static char *g_heap;
static mp_obj_t g_sessions;  // dict id -> globals, stored in sys.modules
static long g_next_id = 1;

// Every entry from Rust starts a new C stack top for the GC scan and the
// stack check (the Rust caller's frames hold no MicroPython objects).
#define ENTER() \
    volatile int _top; \
    mp_cstack_init_with_top((void *)&_top, g_stack)
static size_t g_stack;

// The print sink. The unix port's `print` writes to fd 1 through sys.stdout,
// so each session gets this native `print` (sep, end; `file=` goes to the
// builtin one), which hands the text to the host.
static void out_strn(void *env, const char *str, size_t len) {
    (void)env;
    g_out(str, len);
}
static const mp_print_t host_printer = {NULL, out_strn};

static mp_obj_t host_print(size_t n_args, const mp_obj_t *pos, mp_map_t *kw) {
    mp_map_elem_t *file = mp_map_lookup(kw, MP_OBJ_NEW_QSTR(MP_QSTR_file), MP_MAP_LOOKUP);
    if (file != NULL && file->value != mp_const_none) {
        return mp_builtin_print_obj.fun.kw(n_args, pos, kw);
    }
    mp_map_elem_t *sep = mp_map_lookup(kw, MP_OBJ_NEW_QSTR(MP_QSTR_sep), MP_MAP_LOOKUP);
    mp_map_elem_t *end = mp_map_lookup(kw, MP_OBJ_NEW_QSTR(MP_QSTR_end), MP_MAP_LOOKUP);
    const char *sep_s = " ", *end_s = "\n";
    size_t sep_l = 1, end_l = 1;
    if (sep != NULL && sep->value != mp_const_none) {
        sep_s = mp_obj_str_get_data(sep->value, &sep_l);
    }
    if (end != NULL && end->value != mp_const_none) {
        end_s = mp_obj_str_get_data(end->value, &end_l);
    }
    for (size_t i = 0; i < n_args; i++) {
        if (i > 0) {
            g_out(sep_s, sep_l);
        }
        mp_obj_print_helper(&host_printer, pos[i], PRINT_STR);
    }
    g_out(end_s, end_l);
    return mp_const_none;
}
static MP_DEFINE_CONST_FUN_OBJ_KW(host_print_obj, 0, host_print);

// What the CLI's main.o (left out of the library) defines.
static void stderr_print_strn(void *env, const char *str, size_t len) {
    (void)env;
    fwrite(str, 1, len, stderr);
}
const mp_print_t mp_stderr_print = {NULL, stderr_print_strn};
bool mp_compile_only = false;

void nlr_jump_fail(void *val) {
    fprintf(stderr, "FATAL: uncaught NLR %p\n", val);
    exit(1);
}

static mp_obj_t hsync(mp_obj_t x) {
    return mp_obj_new_int_from_ll(g_sync(mp_obj_get_int(x)));
}
static MP_DEFINE_CONST_FUN_OBJ_1(hsync_obj, hsync);

// _hc(name, json_args) -> (True, json_reply) | (False, type, message)
static mp_obj_t hc(mp_obj_t name, mp_obj_t args) {
    size_t nl, al, ol;
    const char *n = mp_obj_str_get_data(name, &nl);
    const char *a = mp_obj_str_get_data(args, &al);
    int is_err = 0;
    const char *r = g_call(n, nl, a, al, &ol, &is_err);
    if (!is_err) {
        mp_obj_t items[2] = {mp_const_true, mp_obj_new_str(r, ol)};
        return mp_obj_new_tuple(2, items);
    }
    // "Type\0message"
    size_t tl = strlen(r);
    mp_obj_t items[3] = {mp_const_false, mp_obj_new_str(r, tl), mp_obj_new_str(r + tl + 1, ol - tl - 1)};
    return mp_obj_new_tuple(3, items);
}
static MP_DEFINE_CONST_FUN_OBJ_2(hc_obj, hc);

static mp_obj_t key(long id) {
    return mp_obj_new_int(id);
}

void mpy_init(size_t heap_size, size_t stack_size, host_call_fn call, host_sync_fn sync, host_out_fn out) {
    g_call = call;
    g_sync = sync;
    g_out = out;
    g_stack = stack_size;
    ENTER();
    g_heap = malloc(heap_size);
    gc_init(g_heap, g_heap + heap_size);
    mp_init();
    // Imports come from the frozen modules only: no filesystem path.
    mp_sys_path = mp_obj_new_list(0, NULL);
    mp_obj_list_append(mp_sys_path, MP_OBJ_NEW_QSTR(qstr_from_str(".frozen")));
    mp_obj_list_init(MP_OBJ_TO_PTR(mp_sys_argv), 0);
    g_sessions = mp_obj_new_dict(0);
    mp_obj_dict_store(MP_OBJ_FROM_PTR(&MP_STATE_VM(mp_loaded_modules_dict)),
        MP_OBJ_NEW_QSTR(qstr_from_str("__host_sessions__")), g_sessions);
}

long mpy_new_session(void) {
    ENTER();
    mp_obj_t d = mp_obj_new_dict(0);
    mp_obj_dict_store(d, MP_OBJ_NEW_QSTR(MP_QSTR___name__), MP_OBJ_NEW_QSTR(MP_QSTR___main__));
    mp_obj_dict_store(d, MP_OBJ_NEW_QSTR(qstr_from_str("hsync")), MP_OBJ_FROM_PTR(&hsync_obj));
    mp_obj_dict_store(d, MP_OBJ_NEW_QSTR(qstr_from_str("_hc")), MP_OBJ_FROM_PTR(&hc_obj));
    mp_obj_dict_store(d, MP_OBJ_NEW_QSTR(MP_QSTR_print), MP_OBJ_FROM_PTR(&host_print_obj));
    long id = g_next_id++;
    mp_obj_dict_store(g_sessions, key(id), d);
    return id;
}

void mpy_drop_session(long id) {
    ENTER();
    nlr_buf_t nlr;
    if (nlr_push(&nlr) == 0) {
        mp_obj_dict_delete(g_sessions, key(id));
        nlr_pop();
    }
}

static vstr_t g_err;

// Runs `src` in session `id`. Returns 0, or 1 with the formatted exception
// in *err (valid until the next call).
int mpy_exec(long id, const char *src, size_t len, const char **err, size_t *err_len) {
    ENTER();
    int rc = 0;
    nlr_buf_t nlr;
    mp_obj_dict_t *old_g = mp_globals_get(), *old_l = mp_locals_get();
    if (nlr_push(&nlr) == 0) {
        mp_obj_t d = mp_obj_dict_get(g_sessions, key(id));
        mp_lexer_t *lex = mp_lexer_new_from_str_len(qstr_from_str("cell.py"), src, len, 0);
        mp_parse_compile_execute(lex, MP_PARSE_FILE_INPUT, MP_OBJ_TO_PTR(d), MP_OBJ_TO_PTR(d));
        nlr_pop();
    } else {
        mp_globals_set(old_g);
        mp_locals_set(old_l);
        mp_obj_t exc = MP_OBJ_FROM_PTR(nlr.ret_val);
        if (g_err.buf) {
            vstr_clear(&g_err);
        }
        mp_print_t pr;
        vstr_init_print(&g_err, 256, &pr);
        mp_obj_print_exception(&pr, exc);
        *err = g_err.buf;
        *err_len = g_err.len;
        rc = 1;
    }
    // A stop that arrived after the script's last check must not hit the next run.
    MP_STATE_THREAD(mp_pending_exception) = MP_OBJ_NULL;
    return rc;
}

// From another thread: raise KeyboardInterrupt at the VM's next check (a
// backward jump or call), as the unix port's SIGINT handler does.
void mpy_interrupt(void) {
    mp_sched_keyboard_interrupt();
}

// Collects and returns the GC heap's used bytes.
size_t mpy_heap_used(void) {
    ENTER();
    gc_collect();
    gc_info_t info;
    gc_info(&info);
    return info.used;
}
