//! A mesh-only kernel: no B-rep, no booleans, no STEP.
//!
//! Solids are triangle meshes. Primitives are tessellated directly; `union` concatenates meshes,
//! which is correct for display and approximately right for mass properties (overlapping volume
//! is counted twice). Use it for previews, tests and builds without a C++ toolchain. Anything
//! that needs real booleans or STEP reports `Unsupported`.

use std::path::Path;

use crate::mesh::box_mesh;
use crate::{GeomError, GeomKernel, Mesh, Result, Vec3, add, normalize, scale, sub};

/// Number of segments around a circle.
const SEGMENTS: usize = 48;

#[derive(Default, Clone, Copy)]
pub struct MeshKernel;

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Two unit vectors perpendicular to `n` and to each other.
fn basis(n: Vec3) -> (Vec3, Vec3) {
    let helper = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = normalize(cross(n, helper)).unwrap_or([0.0, 1.0, 0.0]);
    let v = cross(n, u);
    (u, v)
}

/// Ring of points of radius `r` in the plane through `centre` perpendicular to `n`.
fn ring(centre: Vec3, n: Vec3, r: f64) -> Vec<Vec3> {
    let (u, v) = basis(n);
    (0..SEGMENTS)
        .map(|i| {
            let t = i as f64 / SEGMENTS as f64 * std::f64::consts::TAU;
            add(centre, add(scale(u, r * t.cos()), scale(v, r * t.sin())))
        })
        .collect()
}

/// Closed cylindrical wall between two rings with matching segment order. `outward` selects the
/// winding so normals face away from the axis (true) or toward it (false, for inner walls).
fn wall(mesh: &mut Mesh, ring_a: &[Vec3], ring_b: &[Vec3], outward: bool) {
    let base = mesh.positions.len() as u32;
    mesh.positions.extend_from_slice(ring_a);
    mesh.positions.extend_from_slice(ring_b);
    let n = ring_a.len() as u32;
    for i in 0..n {
        let j = (i + 1) % n;
        let (a0, a1, b0, b1) = (base + i, base + j, base + n + i, base + n + j);
        // Rings are counter-clockwise seen from +n, so (a0 -> a1) x (a0 -> b0) points outward.
        if outward {
            mesh.triangles.push([a0, a1, b0]);
            mesh.triangles.push([a1, b1, b0]);
        } else {
            mesh.triangles.push([a0, b0, a1]);
            mesh.triangles.push([a1, b0, b1]);
        }
    }
}

/// Flat annular (or full) cap between an outer ring and an inner ring (or a centre point).
fn cap(mesh: &mut Mesh, outer: &[Vec3], inner: Option<&[Vec3]>, centre: Vec3, flip: bool) {
    let base = mesh.positions.len() as u32;
    let n = outer.len() as u32;
    mesh.positions.extend_from_slice(outer);
    match inner {
        Some(inner) => {
            mesh.positions.extend_from_slice(inner);
            for i in 0..n {
                let j = (i + 1) % n;
                let (o0, o1, i0, i1) = (base + i, base + j, base + n + i, base + n + j);
                if flip {
                    mesh.triangles.push([o0, i0, o1]);
                    mesh.triangles.push([o1, i0, i1]);
                } else {
                    mesh.triangles.push([o0, o1, i0]);
                    mesh.triangles.push([o1, i1, i0]);
                }
            }
        }
        None => {
            mesh.positions.push(centre);
            let c = base + n;
            for i in 0..n {
                let j = (i + 1) % n;
                if flip {
                    mesh.triangles.push([base + i, c, base + j]);
                } else {
                    mesh.triangles.push([base + i, base + j, c]);
                }
            }
        }
    }
}

/// Fix winding so that the mesh has positive volume (outward normals), and clear normals so
/// consumers flat-shade it.
fn finish(mut mesh: Mesh) -> Mesh {
    if mesh.mass_props().volume < 0.0 {
        for t in &mut mesh.triangles {
            t.swap(1, 2);
        }
    }
    mesh.normals.clear();
    mesh
}

impl GeomKernel for MeshKernel {
    type Solid = Mesh;

    fn make_box(&self, size: Vec3, centre: Vec3) -> Result<Mesh> {
        let half = scale(size, 0.5);
        Ok(finish(box_mesh(sub(centre, half), add(centre, half))))
    }

    fn cylinder_between(&self, a: Vec3, b: Vec3, r: f64) -> Result<Mesh> {
        if r <= 0.0 {
            return Err(GeomError::Kernel(format!("cylinder radius {r} must be positive")));
        }
        let n = normalize(sub(b, a))?;
        let ra = ring(a, n, r);
        let rb = ring(b, n, r);
        let mut m = Mesh::default();
        wall(&mut m, &ra, &rb, true);
        cap(&mut m, &ra, None, a, true);
        cap(&mut m, &rb, None, b, false);
        Ok(finish(m))
    }

    fn tube_between(&self, a: Vec3, b: Vec3, od: f64, wall_t: f64) -> Result<Mesh> {
        if wall_t <= 0.0 || wall_t * 2.0 >= od {
            return Err(GeomError::Kernel(format!("tube wall {wall_t} is not valid for od {od}")));
        }
        let n = normalize(sub(b, a))?;
        let (ro, ri) = (od / 2.0, od / 2.0 - wall_t);
        let (oa, ob, ia, ib) = (ring(a, n, ro), ring(b, n, ro), ring(a, n, ri), ring(b, n, ri));
        let mut m = Mesh::default();
        wall(&mut m, &oa, &ob, true);
        wall(&mut m, &ia, &ib, false);
        cap(&mut m, &oa, Some(&ia), a, true);
        cap(&mut m, &ob, Some(&ib), b, false);
        Ok(finish(m))
    }

    fn union(&self, a: &Mesh, b: &Mesh) -> Result<Mesh> {
        let mut m = a.clone();
        m.extend(b);
        Ok(m)
    }

    fn subtract(&self, _a: &Mesh, _b: &Mesh) -> Result<Mesh> {
        Err(GeomError::Unsupported("subtract (mesh kernel has no booleans)".into()))
    }

    fn intersect(&self, _a: &Mesh, _b: &Mesh) -> Result<Mesh> {
        Err(GeomError::Unsupported("intersect (mesh kernel has no booleans)".into()))
    }

    fn translated(&self, s: &Mesh, offset: Vec3) -> Result<Mesh> {
        let mut m = s.clone();
        for p in &mut m.positions {
            *p = add(*p, offset);
        }
        Ok(m)
    }

    fn mirrored(&self, s: &Mesh, origin: Vec3, normal: Vec3) -> Result<Mesh> {
        let n = normalize(normal)?;
        let mut m = s.clone();
        for p in &mut m.positions {
            let d = sub(*p, origin);
            let k = d[0] * n[0] + d[1] * n[1] + d[2] * n[2];
            *p = sub(*p, scale(n, 2.0 * k));
        }
        for t in &mut m.triangles {
            t.swap(1, 2);
        }
        m.normals.clear();
        Ok(m)
    }

    fn tessellate(&self, s: &Mesh, _tolerance: f64) -> Result<Mesh> {
        Ok(s.clone())
    }

    fn write_step(&self, _s: &Mesh, _path: &Path) -> Result<()> {
        Err(GeomError::Unsupported("STEP export (mesh kernel)".into()))
    }

    fn read_step(&self, _path: &Path) -> Result<Mesh> {
        Err(GeomError::Unsupported("STEP import (mesh kernel)".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1e-12)
    }

    #[test]
    fn cylinder_volume_and_centroid() {
        let k = MeshKernel;
        let c = k.cylinder_between([0.0; 3], [0.0, 0.0, 0.5], 0.1).unwrap();
        let mp = c.mass_props();
        let expected = std::f64::consts::PI * 0.01 * 0.5;
        assert!(close(mp.volume, expected, 0.01), "{} vs {}", mp.volume, expected);
        assert!(close(mp.centroid[2], 0.25, 1e-6));
    }

    #[test]
    fn tube_volume_matches_analytic() {
        let k = MeshKernel;
        let t = k.tube_between([0.0; 3], [0.3, 0.0, 0.0], 0.028, 0.0025).unwrap();
        let mp = t.mass_props();
        let (ro, ri): (f64, f64) = (0.014, 0.0115);
        let expected = std::f64::consts::PI * (ro * ro - ri * ri) * 0.3;
        assert!(close(mp.volume, expected, 0.01), "{} vs {}", mp.volume, expected);
        assert!(close(mp.centroid[0], 0.15, 1e-6));
    }

    #[test]
    fn mirrored_box_keeps_positive_volume() {
        let k = MeshKernel;
        let b = k.make_box([0.1, 0.2, 0.3], [1.0, 0.0, 0.0]).unwrap();
        let m = k.mirrored(&b, [0.0; 3], [1.0, 0.0, 0.0]).unwrap();
        let mp = m.mass_props();
        assert!(close(mp.volume, 0.006, 1e-9));
        assert!(close(mp.centroid[0], -1.0, 1e-9));
    }
}
