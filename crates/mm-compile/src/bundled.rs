//! Private native indices shared by the bundled compilers and the VM.
//!
//! They are relative to the contiguous private-native range in the VM's
//! native table. Stdlib features can be built without asyncio, so the
//! independent, prelude-only asyncio group follows the stdlib groups.

const CORE: &[&str] = &[
    "module_dict",
    "signature",
    "nfkc_table",
    "backing",
    "index",
    "method",
    "clock",
    "tzif",
    "caller",
    "exc_match_part",
    "exc_name",
    "unicode_fields",
    "unicode_str",
    "make_class",
    "exc_same_meta",
    "exc_tb_here",
    "ctx_get",
    "ctx_swap",
    "ctx_copy",
    "code_kind",
    "local_zone",
    "gc_collect",
    "gc_wait",
    "urandom",
    "hint",
    "host_help",
    "os_function",
    "rec_new",
];
const DATETIME: &[&str] = &["dt_iso", "dt_ord", "dt_ymd", "dt_strptime"];
const ASYNCIO: &[&str] = &[
    "aio_run",
    "aio_create_task",
    "aio_spawn",
    "aio_first",
    "aio_deadline",
    "aio_future",
    "aio_set",
    "aio_asyncgens",
    "aio_current_task",
    "aio_all_tasks",
    "aio_spawn_internal",
    "aio_name_set",
    "aio_soon",
    "aio_fire",
    "aio_time",
    "aio_running",
    "aio_set_exc",
    "aio_tasks",
    "aio_stop",
    "aio_main_end",
    "aio_task_ctx",
    "aio_unretrieved",
    "aio_escape",
];

/// The native groups enabled for the unit being compiled.
#[derive(Clone, Copy)]
pub struct Natives {
    /// Whether the engine's datetime natives are enabled.
    pub datetime: bool,
    /// Whether the engine's asyncio natives are enabled.
    pub asyncio: bool,
}

impl Natives {
    /// The private native at a bundled constant's index.
    #[must_use]
    pub const fn name(self, i: usize) -> Option<&'static str> {
        if i < CORE.len() {
            return Some(CORE[i]);
        }
        let mut n = i - CORE.len();
        if self.datetime {
            if n < DATETIME.len() {
                return Some(DATETIME[n]);
            }
            n -= DATETIME.len();
        }
        if self.asyncio && n < ASYNCIO.len() {
            Some(ASYNCIO[n])
        } else {
            None
        }
    }

    #[cfg(feature = "bundled")]
    pub(crate) fn index(self, name: &str) -> Option<u8> {
        (0..=u8::MAX)
            .map_while(|i| self.name(usize::from(i)).map(|n| (i, n)))
            .find_map(|(i, n)| (n == name).then_some(i))
    }
}
