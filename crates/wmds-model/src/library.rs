//! Loading the primitive, assembly, chassis and port-type definition files from disk.
//!
//! A definition's id is its path under the library root, so
//! `library/suspension/arms/lca-wishbone-a.prim.kdl` declares
//! `primitive "suspension/arms/lca-wishbone-a"`. The loader checks that the declared id and the
//! file location agree, which keeps cross-references resolvable without an index file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use thiserror::Error;
use wmds_schema::{AssemblyDef, ChassisDef, PortTypeDef, PrimitiveDef};

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("{path}: {message}")]
    Load { path: String, message: String },
    #[error("no library found at {0}")]
    Missing(String),
}

/// Everything loaded from a library directory.
#[derive(Default)]
pub struct Library {
    pub root: PathBuf,
    pub primitives: BTreeMap<String, PrimitiveDef>,
    pub assemblies: BTreeMap<String, AssemblyDef>,
    pub chassis: BTreeMap<String, ChassisDef>,
    pub port_types: IndexMap<String, PortTypeDef>,
    /// Files that failed to parse, reported rather than fatal so one bad file does not stop work.
    pub failures: Vec<(PathBuf, String)>,
}

impl Library {
    /// Load `library/` (primitives, assemblies, `ports.kdl`) and `chassis/` if present, both
    /// relative to `project_root`.
    pub fn load(project_root: &Path) -> Result<Library, LibraryError> {
        let lib_root = project_root.join("library");
        if !lib_root.is_dir() {
            return Err(LibraryError::Missing(lib_root.display().to_string()));
        }
        let mut lib = Library { root: lib_root.clone(), ..Default::default() };

        let ports_file = lib_root.join("ports.kdl");
        if ports_file.is_file() {
            let src = read(&ports_file)?;
            match wmds_schema::parse_port_types("ports.kdl", &src) {
                Ok(p) => lib.port_types = p,
                Err(e) => lib.failures.push((ports_file, format!("{e:?}"))),
            }
        }

        let mut files = Vec::new();
        collect(&lib_root, &mut files);
        collect(&project_root.join("chassis"), &mut files);

        for f in files {
            let name = f.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let src = match std::fs::read_to_string(&f) {
                Ok(s) => s,
                Err(e) => {
                    lib.failures.push((f, e.to_string()));
                    continue;
                }
            };
            if name.ends_with(".prim.kdl") {
                match wmds_schema::parse_primitive(&name, &src) {
                    Ok(d) => {
                        check_id(&mut lib, &f, &lib_root, &d.id, ".prim.kdl");
                        lib.primitives.insert(d.id.clone(), d);
                    }
                    Err(e) => lib.failures.push((f, format!("{e:?}"))),
                }
            } else if name.ends_with(".asm.kdl") || name.ends_with(".veh.kdl") {
                match wmds_schema::parse_assembly(&name, &src) {
                    Ok(d) => {
                        lib.assemblies.insert(d.id.clone(), d);
                    }
                    Err(e) => lib.failures.push((f, format!("{e:?}"))),
                }
            } else if name.ends_with(".chassis.kdl") {
                match wmds_schema::parse_chassis(&name, &src) {
                    Ok(d) => {
                        lib.chassis.insert(d.id.clone(), d);
                    }
                    Err(e) => lib.failures.push((f, format!("{e:?}"))),
                }
            }
        }
        Ok(lib)
    }

    /// Load a single vehicle or assembly file that lives outside the library.
    pub fn load_assembly_file(path: &Path) -> Result<AssemblyDef, LibraryError> {
        let src = read(path)?;
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        wmds_schema::parse_assembly(&name, &src).map_err(|e| LibraryError::Load {
            path: path.display().to_string(),
            message: format!("{e:?}"),
        })
    }

    pub fn primitive(&self, id: &str) -> Option<&PrimitiveDef> {
        self.primitives.get(id)
    }

    pub fn assembly(&self, id: &str) -> Option<&AssemblyDef> {
        self.assemblies.get(id)
    }

    pub fn summary(&self) -> String {
        format!(
            "{} primitives, {} assemblies, {} chassis systems, {} port types",
            self.primitives.len(),
            self.assemblies.len(),
            self.chassis.len(),
            self.port_types.len()
        )
    }
}

fn read(p: &Path) -> Result<String, LibraryError> {
    std::fs::read_to_string(p).map_err(|e| LibraryError::Load {
        path: p.display().to_string(),
        message: e.to_string(),
    })
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if p.is_dir() {
            collect(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Warn when a definition's declared id does not match its path under the library root.
fn check_id(lib: &mut Library, file: &Path, root: &Path, id: &str, suffix: &str) {
    let Ok(rel) = file.strip_prefix(root) else { return };
    let expected = rel.to_string_lossy().replace('\\', "/");
    let expected = expected.trim_end_matches(suffix);
    if expected != id {
        lib.failures.push((
            file.to_path_buf(),
            format!("declared id `{id}` does not match its location, which implies `{expected}`"),
        ));
    }
}
