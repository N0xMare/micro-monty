// MicroPython "sandbox" variant for docs/research/python-engines-compare.md:
// the unix port's standard feature level (MPZ big ints, floats, asyncio,
// json, re, f-strings, ...) without the modules that reach outside a
// sandbox or that code mode never needs (ffi, sockets, ssl, btree, termios,
// FAT/LittleFS, machine, os.system). The REPL keeps readline: the CLI does not link without it. Built by build.sh; see README "Builds".
#define MICROPY_CONFIG_ROM_LEVEL (MICROPY_CONFIG_ROM_LEVEL_EXTRA_FEATURES)
#include "../mpconfigvariant_common.h"
#undef MICROPY_PY_OS_SYSTEM
#define MICROPY_PY_OS_SYSTEM (0)
#undef MICROPY_PY_MACHINE
#define MICROPY_PY_MACHINE (0)
#undef MICROPY_PY_MACHINE_PULSE
#define MICROPY_PY_MACHINE_PULSE (0)
#undef MICROPY_PY_MACHINE_PIN_BASE
#define MICROPY_PY_MACHINE_PIN_BASE (0)
#undef MICROPY_PY_WEBSOCKET
#define MICROPY_PY_WEBSOCKET (0)
