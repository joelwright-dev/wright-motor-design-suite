# Vendored: opencascade-rs

Source: https://github.com/bschwind/opencascade-rs
Commit: `32758df23a137f4e7c786c618b0317fc3badb3b2` (2026-08-24, the 0.3.0 release)
Licence: LGPL-2.1 (see `LICENSE`; OpenCASCADE itself is LGPL-2.1 with the OCCT exception)

Crates copied: `opencascade-sys`, `opencascade`, `kicad-parser` (only `Cargo.toml`, `build.rs`,
`src/`, `include/`, and the `OCCT/CMakeLists.txt` detection stub). The OpenCASCADE sources themselves are not vendored; they come from the
`occt-sys` crate on crates.io through the `builtin` feature.

## Why vendored

1. The crates.io package's build script includes headers as `opencascade-sys/include/...`
   relative to the parent of the crate directory. That works in a git checkout (where the
   directory is `crates/opencascade-sys`) but not in the cargo registry (where it is
   `opencascade-sys-0.3.0`), and the cxx fallback needs symlink permission on Windows.
2. On MSVC, OpenCASCADE defines each `Handle_X` as a class derived from
   `opencascade::handle<X>` rather than a typedef (`Standard_Handle.hxx`,
   `DEFINE_STANDARD_HANDLECLASS` for `_MSC_VER >= 1800`). The upstream headers construct
   `std::unique_ptr<Handle_X>` from `new opencascade::handle<X>(...)`, which does not compile
   there.

## Patch applied

In `crates/opencascade-sys/include/*.hxx`, every

```cpp
new opencascade::handle<X>(...)
```

was replaced with

```cpp
new_handle<Handle_X>(...)
```

where `new_handle` (added to `bindings_common.hxx`) builds the base `opencascade::handle<X>`
from the argument (so maker objects, const handle references and raw pointers all still work)
and then moves it into a heap-allocated `Handle_X`. On non-MSVC platforms `Handle_X` is the
typedef and this is a plain copy. Applied with:

```bash
sed -i -E 's/new opencascade::handle<([A-Za-z0-9_]+)>\(/new_handle<Handle_\1>(/g' crates/opencascade-sys/include/*.hxx
```

For the same reason, cxx cannot bind an OCCT member or static function directly when one of its
parameters is a handle, because the function-pointer types differ on MSVC. Two such bindings
were replaced with inline free-function wrappers declared in the crate headers:

| Binding | Wrapper |
|---------|---------|
| `BRepLib_ToolTriangulatedShape::ComputeNormals` (`b_rep_lib.hxx` / `.rs`) | `BRepLib_ToolTriangulatedShape_ComputeNormals` |
| `BRepOffsetAPI_MakePipeShell::SetLaw` (`b_rep_offset_api.hxx` / `.rs`) | `BRepOffsetAPI_MakePipeShell_SetLaw` |

The two call sites in `crates/opencascade/src/{mesh.rs,make_pipe_shell.rs}` were updated to
match. Nothing else was modified. Worth offering upstream as a pull request.

## Licence note for distribution

WMDS links OpenCASCADE statically. LGPL-2.1 with the OCCT exception permits this for
proprietary use provided the OCCT licence text accompanies distributions and modifications to
OCCT itself (none here) are published. Revisit before the first binary release.
