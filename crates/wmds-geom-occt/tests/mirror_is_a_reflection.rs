//! Mirroring a handed part must reflect it, not turn it round.
//!
//! This test exists because the two things look identical on a symmetric shape and are
//! completely different on an asymmetric one. OpenCASCADE has two operations called mirror: one
//! about a line, which is a 180 degree rotation and keeps handedness, and one about a plane,
//! which is a reflection and reverses it. The adapter used the first for a year of library
//! development, so every right-hand suspension arm in the vehicle reached inboard instead of
//! outboard. Nothing caught it because every other test runs on the pure-Rust kernel, which does
//! it correctly, and because a bounding box cannot tell a reflection from a rotation on a part
//! that is symmetric front to rear.
//!
//! So: an L-shaped solid, deliberately asymmetric in all three directions, through both kernels.

use wmds_geom::{GeomKernel, MeshKernel};
use wmds_geom_occt::OcctKernel;

/// An L, with a long leg in +y and a short one in +x, sitting above z = 0.
///
/// Nothing about it is symmetric, so any transform that is not the one asked for shows up.
fn ell<K: GeomKernel>(k: &K) -> K::Solid {
    let long = k
        .make_box([0.04, 0.30, 0.02], [0.0, 0.15, 0.05])
        .expect("box");
    let short = k
        .make_box([0.20, 0.04, 0.02], [0.10, 0.0, 0.05])
        .expect("box");
    k.union(&long, &short).expect("union")
}

fn bounds<K: GeomKernel>(k: &K, s: &K::Solid) -> ([f64; 3], [f64; 3]) {
    k.tessellate(s, 1e-4)
        .expect("tessellates")
        .bounds()
        .expect("has bounds")
}

fn check<K: GeomKernel>(k: &K, name: &str) {
    let solid = ell(k);
    let (lo, hi) = bounds(k, &solid);

    // Mirror in the xz plane, which is the plane a left and right part are reflected in.
    let mirrored = k.mirrored(&solid, [0.0; 3], [0.0, 1.0, 0.0]).expect("mirror");
    let (mlo, mhi) = bounds(k, &mirrored);

    let tol = 1e-3;
    assert!(
        (mlo[1] + hi[1]).abs() < tol && (mhi[1] + lo[1]).abs() < tol,
        "{name}: y should be reflected. The original spans {:.3} to {:.3} and the mirror spans \
         {:.3} to {:.3}, where it should span {:.3} to {:.3}.",
        lo[1],
        hi[1],
        mlo[1],
        mhi[1],
        -hi[1],
        -lo[1]
    );
    assert!(
        (mlo[0] - lo[0]).abs() < tol && (mhi[0] - hi[0]).abs() < tol,
        "{name}: x must not move when mirroring about the xz plane. It went from {:.3}..{:.3} \
         to {:.3}..{:.3}, which is what a 180 degree rotation about the y axis does rather than \
         a reflection.",
        lo[0],
        hi[0],
        mlo[0],
        mhi[0]
    );
    assert!(
        (mlo[2] - lo[2]).abs() < tol && (mhi[2] - hi[2]).abs() < tol,
        "{name}: z must not move either. It went from {:.3}..{:.3} to {:.3}..{:.3}.",
        lo[2],
        hi[2],
        mlo[2],
        mhi[2]
    );

    // And mirroring twice must give back what you started with.
    let back = k
        .mirrored(&mirrored, [0.0; 3], [0.0, 1.0, 0.0])
        .expect("mirror");
    let (blo, bhi) = bounds(k, &back);
    for i in 0..3 {
        assert!(
            (blo[i] - lo[i]).abs() < tol && (bhi[i] - hi[i]).abs() < tol,
            "{name}: mirroring twice should return the original shape"
        );
    }
}

#[test]
fn the_mesh_kernel_reflects() {
    check(&MeshKernel::default(), "mesh kernel");
}

#[test]
fn opencascade_reflects() {
    check(&OcctKernel::default(), "OpenCASCADE");
}

#[test]
fn both_kernels_agree_about_a_mirrored_shape() {
    // The bug was invisible because the two kernels were never compared. They are now.
    let mesh = MeshKernel::default();
    let occt = OcctKernel::default();
    let a = ell(&mesh);
    let b = ell(&occt);
    let am = mesh.mirrored(&a, [0.0; 3], [0.0, 1.0, 0.0]).expect("mirror");
    let bm = occt.mirrored(&b, [0.0; 3], [0.0, 1.0, 0.0]).expect("mirror");
    let (alo, ahi) = bounds(&mesh, &am);
    let (blo, bhi) = bounds(&occt, &bm);
    for i in 0..3 {
        assert!(
            (alo[i] - blo[i]).abs() < 2e-3 && (ahi[i] - bhi[i]).abs() < 2e-3,
            "the kernels disagree on axis {i}: mesh gives {:.4}..{:.4}, OpenCASCADE gives \
             {:.4}..{:.4}",
            alo[i],
            ahi[i],
            blo[i],
            bhi[i]
        );
    }
}
