//! Retained measurements, not verdicts. The tree ID pins the producer's
//! files together; an incomplete or edited copy is a miss. The caller still
//! checks the gate's own recipe, tool and workload keys before using a row.

use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::{command_output, wasm_wall::git};

const TAG: &str = "b3dacbd8d723e762fc4aee3714ebe84c65ece419";
const DATA: &str = "3b7d0500d37624fd6675f8489aef2cfd8b051eb3";
static HOST: OnceLock<bool> = OnceLock::new();

pub(crate) fn file(root: &Path, sha: &str, name: &str) -> Option<PathBuf> {
    // The seed only changes floor files, which the tag's builds do not
    // read. Never substitute a tag for an engine or recipe change.
    if std::env::var_os("MM_RECOUNT_TAG").is_some()
        || sha != TAG
            && git(
                root,
                &[
                    "diff",
                    "--quiet",
                    TAG,
                    sha,
                    "--",
                    ".",
                    ":(exclude)xtask/ir-floors/",
                ],
            )
            .is_err()
    {
        return None;
    }
    if !*HOST.get_or_init(|| {
        let version = |tool, args: &[&str]| command_output(Path::new(tool), args).ok();
        version("cc", &["--version"])
            .and_then(|s| s.lines().next().map(str::to_owned))
            .as_deref()
            == Some("cc (Debian 14.2.0-19) 14.2.0")
            && version("ld", &["--version"])
                .and_then(|s| s.lines().next().map(str::to_owned))
                .as_deref()
                == Some("GNU ld (GNU Binutils for Debian) 2.44")
            && version("getconf", &["GNU_LIBC_VERSION"]).as_deref() == Some("glibc 2.41")
    }) {
        return None;
    }
    let rel = format!("xtask/tag-reference/{TAG}/{name}");
    let want = git(root, &["rev-parse", "--verify", &format!("{DATA}:{name}")]).ok()?;
    (git(root, &["hash-object", "--", &rel]).ok()? == want).then(|| root.join(rel))
}
