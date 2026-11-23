//! Experimental seamless parameterization. The mapping energy and greedy
//! integer rounding follow Bommes et al. (2009); conservative orientation
//! half-spaces follow the general construction in Bommes et al. (2013), §3.1.
//! This is not the latter paper's global branch-and-cut optimizer.
mod atlas;
mod extract;
mod quadratic;

use crate::{Diagnostics, Layout, Options, Result, Stage, Surface, field};
use quadratic::Budget;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, atomic::AtomicBool},
};

/// Experimental MIQ parameterization with independently checked IGM conditions.
/// Keeps callback ownership with the caller; only the sparse numerical solve
/// uses a scoped worker, whose cancellation flag is checked every QP iteration.
pub fn reconstruct(
    source: &Surface,
    options: &Options,
    cancel: &dyn Fn() -> bool,
    progress: &dyn Fn(Stage),
) -> Result<Layout> {
    crate::validate_request(source, options, cancel)?;
    if options.preserve_sharp {
        return Err(
            "Experimental integer-grid mapping does not yet support Protect sharp bends".into(),
        );
    }
    let flag = Arc::new(AtomicBool::new(false));
    let result = std::thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::channel();
        let worker_flag = flag.clone();
        let worker = scope.spawn(move || {
            run(source, options, worker_flag, &|s| {
                let _ = tx.send(s);
            })
        });
        loop {
            if cancel() {
                flag.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            match rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(stage) => progress(stage),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        worker
            .join()
            .unwrap_or_else(|_| Err("Experimental integer-grid worker failed".into()))
    });
    crate::check(cancel)?;
    result.map_err(|error| {
        #[cfg(feature="audit")]
        eprintln!("Integer-grid diagnostic: {error}");
        if error.contains("relaxation did not converge") || error.contains("relaxation failed independent") {
            "Integer grid could not find a valid layout with these settings. Try a different target or Surface field.".into()
        } else if error.contains("exceeded its solve budget") {
            "Integer grid reached its solve limit. Try a different target or Surface field.".into()
        } else {error}
    })
}

fn run(
    source: &Surface,
    options: &Options,
    flag: Arc<AtomicBool>,
    progress: &dyn Fn(Stage),
) -> Result<Layout> {
    let cancel = || flag.load(std::sync::atomic::Ordering::Relaxed);
    let mut budget = Budget::new(flag.clone());
    // Parameterize the source domain directly, as in MIQ. The shared field
    // sampler may refine long triangles but does not redistribute the sculpt.
    let field = field::solve_soft_directions(source, options, &cancel, progress)?;
    let (atlas, x) = parameterize(&field, &mut budget, progress)?;
    progress(Stage::Extraction);
    let (mut positions, quads) =
        extract::extract(&field.surface, &atlas, &x, options.max_vertices, &cancel)?;
    let triangles = crate::surface::triangulate(&quads, &cancel)?;
    let topology = crate::topology::validate(source)?;
    if crate::topology::validate_connectivity(&Surface {
        positions: positions.clone(),
        triangles,
    })? != topology
    {
        return Err("Integer-grid extraction changed surface topology".into());
    }
    progress(Stage::SurfaceFit);
    let fit = crate::surface::fit(
        &field.surface,
        &field.spacing,
        &mut positions,
        &quads,
        &cancel,
    )?;
    let mut layout = Layout {
        positions,
        quads,
        triangles: fit.triangles,
        diagnostics: Diagnostics {
            input_vertices: source.positions.len(),
            samples: field.surface.positions.len(),
            max_fitting_distance: fit.displacement,
            max_surface_error: fit.error,
            ..Default::default()
        },
    };
    progress(Stage::OriginalSurfaceValidation);
    crate::validate_layout(source, options, &mut layout, &cancel)?;
    if crate::topology::validate(&Surface {
        positions: layout.positions.clone(),
        triangles: layout.triangles.clone(),
    })? != topology
    {
        return Err("Integer-grid result changed surface topology".into());
    }
    budget.check()?;
    Ok(layout)
}

fn relax(
    atlas: &mut atlas::Atlas,
    fixed: &BTreeMap<usize, f64>,
    active: &mut BTreeSet<usize>,
    budget: &mut Budget,
) -> Result<Vec<f64>> {
    let mut x = atlas.problem.solve(fixed, budget)?;
    for iteration in 0..16 {
        if constrain_orientation(atlas, &x, active)? == 0 {
            return Ok(x);
        }
        if iteration == 15 {
            break;
        }
        // Newly activated orientation rows can reuse the warm linear solve,
        // just as they do during rounding, before restarting the full QP.
        x = match atlas.problem.update(fixed, &x, budget) {
            Ok(next) => next,
            Err(_) => {
                budget.check()?;
                atlas.problem.solve(fixed, budget)?
            }
        };
    }
    Err("Integer-grid orientation solve exceeded its iteration budget".into())
}

fn constrain_orientation(
    atlas: &mut atlas::Atlas,
    x: &[f64],
    active: &mut BTreeSet<usize>,
) -> Result<usize> {
    let mut added = 0;
    for f in 0..atlas.corners.len() {
        let [a, b, c] = atlas.coordinates(x, f);
        if !positive_orientation([a, b, c]) {
            if !active.insert(f) {
                #[cfg(feature = "audit")]
                eprintln!(
                    "MIQ active orientation: face {f}, signed area2 {:.17e}, reference area2 {:.17e}",
                    (b - a).perp_dot(c - a),
                    (atlas.reference[f][1] - atlas.reference[f][0])
                        .perp_dot(atlas.reference[f][2] - atlas.reference[f][0])
                );
                return Err("Integer-grid orientation constraints failed validation".into());
            }
            atlas
                .problem
                .inequalities
                .extend(atlas.orientation_constraints(f)?);
            added += 1;
        }
    }
    Ok(added)
}

/// Filter the sign, not the triangle's absolute area. Shewchuk's orient2d
/// first-stage error bound is below 4*EPSILON times the determinant sum.
/// Ambiguous/zero/nonfinite signs still require repair; there is no exact-
/// arithmetic fallback here. This retains tiny but provably positive cap faces.
fn positive_orientation([a, b, c]: [glam::DVec2; 3]) -> bool {
    let [u, v] = [b - a, c - a];
    let left = u.x * v.y;
    let right = u.y * v.x;
    let determinant = left - right;
    let magnitude = left.abs() + right.abs();
    let error = 4. * f64::EPSILON * magnitude;
    magnitude.is_normal() && determinant.is_finite() && determinant > error
}

fn parameterize(
    field: &field::Field,
    budget: &mut Budget,
    progress: &dyn Fn(Stage),
) -> Result<(atlas::Atlas, Vec<f64>)> {
    let mut atlas = atlas::Atlas::new(field, &|| {
        budget.cancel.load(std::sync::atomic::Ordering::Relaxed)
    })?;
    let mut fixed = BTreeMap::new();
    let mut active = BTreeSet::new();
    let mut x = relax(&mut atlas, &fixed, &mut active, budget)?;
    atlas.complete_singularities(&field.surface, &x, &|| {
        budget.cancel.load(std::sync::atomic::Ordering::Relaxed)
    })?;
    let mut remaining: BTreeSet<_> = atlas.problem.integers.iter().copied().collect();
    while !remaining.is_empty() {
        budget.check()?;
        progress(Stage::Parameterization {
            remaining: remaining.len(),
        });
        // A quarter-turn seam cycle can force its singular point onto a
        // half-integer even when every translation is integral. Fix singular
        // coordinates first so seam rounding cannot lock in that parity trap.
        let singular_phase = remaining.iter().any(|&i| i < atlas.uv_variables);
        // Exact integers in the relaxation can be fixed together without
        // changing its minimizer. Other variables are fixed one at a time.
        let exact: Vec<_> = remaining
            .iter()
            .copied()
            .filter(|&i| {
                (!singular_phase || i < atlas.uv_variables) && (x[i] - x[i].round()).abs() < 1e-7
            })
            .collect();
        if !exact.is_empty() {
            for i in exact {
                remaining.remove(&i);
                fixed.insert(i, x[i].round());
            }
            x = update(&mut atlas, &fixed, &x, &mut active, budget)?;
            continue;
        }
        let i = *remaining
            .iter()
            .filter(|&&i| !singular_phase || i < atlas.uv_variables)
            .min_by(|&&a, &&b| {
                (x[a] - x[a].round())
                    .abs()
                    .total_cmp(&(x[b] - x[b].round()).abs())
                    .then(a.cmp(&b))
            })
            .unwrap();
        let continuous = x[i];
        fixed.insert(i, continuous.round());
        match update(&mut atlas, &fixed, &x, &mut active, budget) {
            Ok(next) => x = next,
            Err(error) if error.contains("PrimalInfeasible") => {
                // One bounded alternate rounding, never an unbounded integer
                // search. An infeasible map is a rejection, not partial output.
                fixed.insert(
                    i,
                    if continuous.round() > continuous {
                        continuous.floor()
                    } else {
                        continuous.ceil()
                    },
                );
                x = update(&mut atlas, &fixed, &x, &mut active, budget)?;
            }
            Err(error) => return Err(error),
        }
        remaining.remove(&i);
    }
    budget.check()?;
    progress(Stage::MapValidation {
        faces: atlas.corners.len(),
    });
    for &i in &atlas.problem.integers {
        if (x[i] - x[i].round()).abs() > 1e-6 {
            return Err("Integer-grid seam or singularity is not integral".into());
        }
    }
    Ok((atlas, x))
}

fn update(
    atlas: &mut atlas::Atlas,
    fixed: &BTreeMap<usize, f64>,
    initial: &[f64],
    active: &mut BTreeSet<usize>,
    budget: &mut Budget,
) -> Result<Vec<f64>> {
    #[cfg(feature = "audit")]
    let start = std::time::Instant::now();
    let mut initial = initial.to_vec();
    // Use the same lazy orientation conditions as a full relaxation, but first
    // update the warm system with them. A newly inverted triangle need not
    // discard the reusable factor and restart the whole constrained optimizer.
    for _ in 0..16 {
        let x = atlas.problem.update(fixed, &initial, budget);
        budget.check()?;
        match x {
            Ok(x) => {
                let flipped = (0..atlas.corners.len())
                    .filter(|&f| !positive_orientation(atlas.coordinates(&x, f)))
                    .count();
                #[cfg(feature = "audit")]
                eprintln!(
                    "MIQ update {:.3}s; {} fixed; {flipped} nonpositive triangles",
                    start.elapsed().as_secs_f64(),
                    fixed.len()
                );
                if flipped == 0 {
                    return Ok(x);
                }
                if constrain_orientation(atlas, &x, active).is_err() {
                    break;
                }
                initial = x;
            }
            Err(error) => {
                #[cfg(feature = "audit")]
                eprintln!(
                    "MIQ update {:.3}s; {} fixed; {error}",
                    start.elapsed().as_secs_f64(),
                    fixed.len()
                );
                #[cfg(not(feature = "audit"))]
                let _ = error;
                break;
            }
        }
    }
    relax(atlas, fixed, active, budget)
}

#[cfg(test)]
pub(crate) fn test_map(source: &Surface, options: &Options) -> Result<usize> {
    let field = field::solve_soft_directions(source, options, &|| false, &|_| {})?;
    let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
    let (atlas, x) = parameterize(&field, &mut budget, &|_| {})?;
    for f in 0..atlas.corners.len() {
        let [a, b, c] = atlas.coordinates(&x, f);
        assert!((b - a).perp_dot(c - a) > 0.);
    }
    let (positions, quads) =
        extract::extract(&field.surface, &atlas, &x, options.max_vertices, &|| false)?;
    assert_eq!(
        crate::topology::validate(&Surface {
            positions: positions.clone(),
            triangles: crate::surface::triangulate(&quads, &|| false)?
        })?,
        crate::topology::validate(source)?
    );
    Ok(quads.len())
}

#[cfg(test)]
mod orientation_tests {
    use super::positive_orientation;
    use glam::DVec2;

    #[test]
    fn tiny_positive_maps_pass_but_flipped_collapsed_and_uncertain_maps_do_not() {
        for scale in [1., 1e-5] {
            let t = [DVec2::ZERO, DVec2::new(1., 0.), DVec2::new(0.3, 1e-7)].map(|p| p * scale);
            for offset in [DVec2::ZERO, DVec2::new(-35., 12.)] {
                let p = t.map(|p| p + offset);
                assert!(positive_orientation(p));
                assert!(!positive_orientation([p[0], p[2], p[1]]));
                assert!(!positive_orientation([p[0], p[1], p[1]]));
            }
        }
        let almost_line = [
            DVec2::ZERO,
            DVec2::ONE,
            DVec2::new(2., 2. + f64::EPSILON * 2.),
        ];
        assert!(!positive_orientation(almost_line));
        assert!(!positive_orientation([
            DVec2::ZERO,
            DVec2::X,
            DVec2::splat(f64::NAN)
        ]));
        assert!(!positive_orientation([
            DVec2::ZERO,
            DVec2::new(1e-160, 0.),
            DVec2::new(0., 1e-160)
        ]));
    }
}
