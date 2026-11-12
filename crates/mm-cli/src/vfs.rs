//! The OS host functions of `--host-fixtures`: Monty's datatest answers
//! `os.getenv`, `os.environ` and the `Path.*` operations from a small
//! virtual filesystem (`monty-datatest`'s `dispatch_os_call`), which the
//! `call-external` corpus cases expect. Writes change this run's copy; a
//! durable run saves it beside the snapshot.

use micro_monty::{HostError, HostErrorKind, HostValue};
use std::collections::{BTreeMap, BTreeSet};

/// The host functions this module answers.
pub const FUNCTIONS: [&str; 17] = [
    "os.getenv",
    "os.environ",
    "Path.exists",
    "Path.is_file",
    "Path.is_dir",
    "Path.is_symlink",
    "Path.read_text",
    "Path.read_bytes",
    "Path.write_text",
    "Path.write_bytes",
    "Path.stat",
    "Path.iterdir",
    "Path.mkdir",
    "Path.unlink",
    "Path.rmdir",
    "Path.rename",
    "Path.resolve",
];

const ENV: [(&str, &str); 3] = [
    ("VIRTUAL_HOME", "/virtual/home"),
    ("VIRTUAL_USER", "testuser"),
    ("VIRTUAL_EMPTY", ""),
];

/// Every file's mtime, as the datatest's.
const MTIME: f64 = 1_700_000_000.0;

/// Files (content, permission bits) and directories, by absolute path.
pub struct Vfs {
    files: BTreeMap<String, (Vec<u8>, i64)>,
    dirs: BTreeSet<String>,
}

impl Default for Vfs {
    fn default() -> Self {
        let files = [
            ("/virtual/file.txt", &b"hello world\n"[..], 0o644),
            ("/virtual/data.bin", b"\x00\x01\x02\x03", 0o644),
            ("/virtual/empty.txt", b"", 0o644),
            ("/virtual/subdir/nested.txt", b"nested content", 0o644),
            ("/virtual/subdir/deep/file.txt", b"deep", 0o644),
            ("/virtual/readonly.txt", b"readonly", 0o444),
        ];
        Self {
            files: files
                .iter()
                .map(|(p, c, m)| ((*p).to_owned(), (c.to_vec(), *m)))
                .collect(),
            dirs: ["/virtual", "/virtual/subdir", "/virtual/subdir/deep"]
                .iter()
                .map(|d| (*d).to_owned())
                .collect(),
        }
    }
}

fn error(kind: HostErrorKind, message: String) -> HostError {
    HostError {
        kind,
        message,
        data: None,
        class: None,
    }
}

fn not_found(path: &str) -> HostError {
    error(
        HostErrorKind::FileNotFoundError,
        format!("[Errno 2] No such file or directory: '{path}'"),
    )
}

fn parent(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => "",
    }
}

impl Vfs {
    /// Answers host function `name`, or `None` if it is not an OS one.
    pub fn call(&mut self, name: &str, args: &[HostValue]) -> Option<Result<HostValue, HostError>> {
        let arg = |i: usize| args.get(i).cloned().unwrap_or(HostValue::None);
        let path = match args.first() {
            Some(HostValue::Str(s)) => s.clone(),
            _ => String::new(),
        };
        let truthy = |i: usize| matches!(arg(i), HostValue::Bool(true));
        Some(match name {
            "os.getenv" => Ok(ENV
                .iter()
                .find(|(k, _)| *k == path)
                .map_or_else(|| arg(1), |(_, v)| HostValue::Str((*v).to_owned()))),
            "os.environ" => Ok(HostValue::Dict(
                ENV.iter()
                    .map(|(k, v)| ((*k).to_owned(), HostValue::Str((*v).to_owned())))
                    .collect(),
            )),
            "Path.exists" => Ok(HostValue::Bool(
                self.files.contains_key(&path) || self.dirs.contains(&path),
            )),
            "Path.is_file" => Ok(HostValue::Bool(self.files.contains_key(&path))),
            "Path.is_dir" => Ok(HostValue::Bool(self.dirs.contains(&path))),
            "Path.is_symlink" => Ok(HostValue::Bool(false)),
            "Path.read_text" => match self.files.get(&path) {
                Some((c, _)) => String::from_utf8(c.clone())
                    .map(HostValue::Str)
                    .map_err(|_| {
                        error(
                            HostErrorKind::ValueError,
                            "'utf-8' codec can't decode bytes".to_owned(),
                        )
                    }),
                None => Err(not_found(&path)),
            },
            "Path.read_bytes" => self
                .files
                .get(&path)
                .map(|(c, _)| HostValue::Bytes(c.clone()))
                .ok_or_else(|| not_found(&path)),
            "Path.write_text" | "Path.write_bytes" => {
                let data = match arg(1) {
                    HostValue::Str(s) => s.into_bytes(),
                    HostValue::Bytes(b) => b,
                    _ => Vec::new(),
                };
                let n = i64::try_from(data.len()).unwrap_or(i64::MAX);
                self.files.insert(path, (data, 0o644));
                Ok(HostValue::Int(n))
            }
            "Path.stat" => self.stat(&path),
            "Path.iterdir" => {
                if !self.dirs.contains(&path) {
                    return Some(Err(not_found(&path)));
                }
                let entries = self
                    .files
                    .keys()
                    .chain(self.dirs.iter())
                    .filter(|p| parent(p) == path)
                    .map(|p| HostValue::Str(p.clone()))
                    .collect();
                Ok(HostValue::List(entries))
            }
            "Path.mkdir" => self.mkdir(path, truthy(1), truthy(2)),
            "Path.unlink" => self
                .files
                .remove(&path)
                .map(|_| HostValue::None)
                .ok_or_else(|| not_found(&path)),
            "Path.rmdir" => {
                if self.dirs.remove(&path) {
                    Ok(HostValue::None)
                } else {
                    Err(not_found(&path))
                }
            }
            "Path.rename" => match arg(1) {
                HostValue::Str(dst) => self.rename(&path, dst),
                _ => Err(not_found(&path)),
            },
            "Path.resolve" => Ok(HostValue::Str(path)),
            _ => return None,
        })
    }

    fn stat(&self, path: &str) -> Result<HostValue, HostError> {
        let (mode, size) = if let Some((c, m)) = self.files.get(path) {
            (0o100_000 | m, i64::try_from(c.len()).unwrap_or(i64::MAX))
        } else if self.dirs.contains(path) {
            (0o040_755, 4096)
        } else {
            return Err(not_found(path));
        };
        let mut st: Vec<HostValue> = [mode, 0, 0, 1, 0, 0, size]
            .into_iter()
            .map(HostValue::Int)
            .collect();
        st.extend([const { HostValue::Float(MTIME) }; 3]);
        Ok(HostValue::List(st))
    }

    fn mkdir(
        &mut self,
        path: String,
        parents: bool,
        exist_ok: bool,
    ) -> Result<HostValue, HostError> {
        if self.dirs.contains(&path) {
            if exist_ok {
                return Ok(HostValue::None);
            }
            return Err(error(
                HostErrorKind::FileExistsError,
                format!("[Errno 17] File exists: '{path}'"),
            ));
        }
        let mut p = parent(&path).to_owned();
        if !p.is_empty() && !self.dirs.contains(&p) {
            if !parents {
                return Err(not_found(&path));
            }
            while !p.is_empty() && p != "/" && self.dirs.insert(p.clone()) {
                p = parent(&p).to_owned();
            }
        }
        self.dirs.insert(path);
        Ok(HostValue::None)
    }

    fn rename(&mut self, path: &str, dst: String) -> Result<HostValue, HostError> {
        if let Some(f) = self.files.remove(path) {
            self.files.insert(dst, f);
        } else if self.dirs.remove(path) {
            self.dirs.insert(dst);
        } else {
            return Err(not_found(path));
        }
        Ok(HostValue::None)
    }

    /// The filesystem as JSON, for a durable run's state file.
    pub fn to_json(&self) -> serde_json::Value {
        let files: serde_json::Map<String, serde_json::Value> = self
            .files
            .iter()
            .map(|(p, (c, m))| (p.clone(), serde_json::json!([c, m])))
            .collect();
        serde_json::json!({"files": files, "dirs": self.dirs})
    }

    /// The filesystem [`Vfs::to_json`] saved (the initial one if absent).
    pub fn from_json(v: &serde_json::Value) -> Self {
        let Some(files) = v["files"].as_object() else {
            return Self::default();
        };
        Self {
            files: files
                .iter()
                .map(|(p, e)| {
                    let content = e[0]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                        .collect();
                    (p.clone(), (content, e[1].as_i64().unwrap_or(0o644)))
                })
                .collect(),
            dirs: v["dirs"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|d| d.as_str().map(str::to_owned))
                .collect(),
        }
    }
}
