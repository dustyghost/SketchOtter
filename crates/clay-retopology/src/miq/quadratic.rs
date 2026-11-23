//! Bounded sparse quadratic relaxations and nearest-first integer fixing.
use crate::Result;
use clarabel::{
    algebra::CscMatrix,
    qdldl::{QDLDLFactorisation, QDLDLSettings},
    solver::*,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

pub(super) type Row = Vec<(usize, f64)>;

pub(super) struct Problem {
    pub variables: usize,
    pub energy: Vec<(Row, f64)>,
    pub equalities: Vec<(Row, f64)>,
    /// Each row is bounded below, not above.
    pub inequalities: Vec<(Row, f64)>,
    pub integers: Vec<usize>,
    factor: Option<DirectFactor>,
    binding: Vec<usize>,
    dual: Option<DualCache>,
}

pub(super) struct Budget {
    pub cancel: Arc<AtomicBool>,
    start: Instant,
    solves: usize,
    #[cfg(test)]
    direct_solves: std::cell::Cell<usize>,
    #[cfg(test)]
    factorizations: std::cell::Cell<usize>,
}

impl Budget {
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            cancel,
            start: Instant::now(),
            solves: 0,
            #[cfg(test)]
            direct_solves: std::cell::Cell::new(0),
            #[cfg(test)]
            factorizations: std::cell::Cell::new(0),
        }
    }
    pub fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("Retopology cancelled".into())
        } else if self.start.elapsed().as_secs_f64() >= 60. || self.solves >= 1024 {
            Err("Experimental integer-grid mapping exceeded its solve budget".into())
        } else {
            Ok(())
        }
    }
}

fn sparse(rows: usize, columns: usize, entries: BTreeMap<(usize, usize), f64>) -> CscMatrix<f64> {
    let mut pointers = vec![0; columns + 1];
    let mut indices = Vec::with_capacity(entries.len());
    let mut values = Vec::with_capacity(entries.len());
    for ((column, row), value) in entries {
        if value != 0. {
            pointers[column + 1] += 1;
            indices.push(row);
            values.push(value);
        }
    }
    for i in 0..columns {
        pointers[i + 1] += pointers[i];
    }
    CscMatrix::new(rows, columns, pointers, indices, values)
}

impl Problem {
    pub fn new(variables: usize) -> Self {
        Self {
            variables,
            energy: Vec::new(),
            equalities: Vec::new(),
            inequalities: Vec::new(),
            integers: Vec::new(),
            factor: None,
            binding: Vec::new(),
            dual: None,
        }
    }

    pub fn solve(&mut self, fixed: &BTreeMap<usize, f64>, budget: &mut Budget) -> Result<Vec<f64>> {
        budget.check()?;
        #[cfg(feature = "audit")]
        let started = Instant::now();
        budget.solves += 1;
        if self.variables > 100_000
            || self.energy.len() + self.equalities.len() + self.inequalities.len() > 500_000
        {
            return Err("Experimental integer-grid system exceeds its size budget".into());
        }
        let mut hessian = BTreeMap::new();
        let mut linear = vec![0.; self.variables];
        for (i, (row, target)) in self.energy.iter().enumerate() {
            if i % 1024 == 0 {
                budget.check()?;
            }
            for &(a, x) in row {
                linear[a] -= 2. * target * x;
                for &(b, y) in row {
                    if a <= b {
                        *hessian.entry((b, a)).or_insert(0.) += 2. * x * y;
                    }
                }
            }
        }
        let p = sparse(self.variables, self.variables, hessian);
        let mut entries = BTreeMap::new();
        let mut bounds = Vec::new();
        for (row, target) in &self.equalities {
            let i = bounds.len();
            for &(column, value) in row {
                *entries.entry((column, i)).or_insert(0.) += value;
            }
            bounds.push(*target);
        }
        for (&column, &value) in fixed {
            entries.insert((column, bounds.len()), 1.);
            bounds.push(value);
        }
        let equalities = bounds.len();
        for (row, target) in &self.inequalities {
            let i = bounds.len();
            for &(column, value) in row {
                *entries.entry((column, i)).or_insert(0.) -= value;
            }
            bounds.push(-target);
        }
        let a = sparse(bounds.len(), self.variables, entries);
        let cones = [
            ZeroConeT(equalities),
            NonnegativeConeT(self.inequalities.len()),
        ];
        let max_iter = 100;
        // Audit-only reduction makes near-converged stops replayable without
        // relying on machine load. It cannot raise the production work limit.
        #[cfg(feature = "audit")]
        let max_iter = match std::env::var("CLAY_MIQ_RELAXATION_ITERATIONS") {
            Ok(value) => value
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=max_iter).contains(n))
                .ok_or("Audit relaxation iterations must be between 1 and 100")?,
            Err(_) => max_iter,
        };
        let settings = DefaultSettings {
            verbose: false,
            max_iter,
            time_limit: (60. - budget.start.elapsed().as_secs_f64()).min(10.),
            tol_feas: 1e-9,
            tol_gap_abs: 1e-9,
            tol_gap_rel: 1e-9,
            ..Default::default()
        };
        budget.check()?;
        let mut solver = DefaultSolver::new(&p, &linear, &a, &bounds, &cones, settings)
            .map_err(|error| format!("Cannot construct integer-grid relaxation: {error}"))?;
        let cancel = budget.cancel.clone();
        solver.set_termination_callback(move |_: &DefaultInfo<f64>| cancel.load(Ordering::Relaxed));
        solver.solve();
        #[cfg(feature = "audit")]
        eprintln!(
            "MIQ relaxation {:.3}s; {} fixed, {} orientation bounds; {:?}, {} iterations, primal residual {:.3e}, dual residual {:.3e}",
            started.elapsed().as_secs_f64(),
            fixed.len(),
            self.inequalities.len(),
            solver.solution.status,
            solver.solution.iterations,
            solver.solution.r_prim,
            solver.solution.r_dual
        );
        budget.check()?;
        self.finish_relaxation(
            solver.solution.status,
            solver.solution.x,
            &solver.solution.z,
            fixed,
            budget,
        )
    }

    fn finish_relaxation(
        &mut self,
        status: SolverStatus,
        x: Vec<f64>,
        dual: &[f64],
        fixed: &BTreeMap<usize, f64>,
        budget: &mut Budget,
    ) -> Result<Vec<f64>> {
        budget.check()?;
        if status == SolverStatus::MaxTime {
            return Err("Experimental integer-grid mapping exceeded its solve budget".into());
        }
        if !matches!(status, SolverStatus::Solved | SolverStatus::AlmostSolved) {
            return Err(format!(
                "Integer-grid relaxation did not converge: {status:?}"
            ));
        }
        self.validate_solution(&x, fixed)?;
        if status == SolverStatus::AlmostSolved {
            // An interior-point iterate need not lie exactly on its active
            // bounds. Forcing all violated bounds of an unconstrained update
            // to equalities can manufacture an infeasible problem here.
            // Independently certify this intermediate relaxation instead;
            // integer rounding, orientation and final mesh gates still follow.
            self.validate_near_optimum(&x, dual, fixed)?;
        }
        budget.check()?;
        // A fresh constrained optimum can release the warm solver's bounds.
        // Do not carry an obsolete active set into the next rounding step.
        self.binding.clear();
        Ok(x)
    }

    fn validate_near_optimum(
        &self,
        x: &[f64],
        dual: &[f64],
        fixed: &BTreeMap<usize, f64>,
    ) -> Result<()> {
        let fail =
            || "Integer-grid relaxation failed independent optimality validation".to_string();
        if dual.len() != self.equalities.len() + fixed.len() + self.inequalities.len()
            || dual.iter().any(|v| !v.is_finite())
        {
            return Err(fail());
        }
        let mut gradient = vec![0.; self.variables];
        let mut linear = vec![0.; self.variables];
        let mut objective = 0.;
        for (row, target) in &self.energy {
            let value = evaluate(row, x);
            objective += value * (value - 2. * target);
            for &(i, a) in row {
                gradient[i] += 2. * a * (value - target);
                linear[i] += 2. * a * target;
            }
        }
        let mut gap = 0.;
        let mut cursor = 0;
        for (row, target) in &self.equalities {
            let z = dual[cursor];
            cursor += 1;
            gap += ((evaluate(row, x) - target) * z).abs();
            for &(i, a) in row {
                gradient[i] += a * z;
            }
        }
        for (&i, &target) in fixed {
            let z = dual[cursor];
            cursor += 1;
            gap += ((x[i] - target) * z).abs();
            gradient[i] += z;
        }
        for (row, target) in &self.inequalities {
            let z = dual[cursor];
            cursor += 1;
            if z < 0. {
                return Err(fail());
            }
            gap += ((evaluate(row, x) - target) * z).abs();
            for &(i, a) in row {
                gradient[i] -= a * z;
            }
        }
        // Recompute KKT stationarity from the original energy/constraints,
        // with Clarabel's unscaled relative-residual normalization and our
        // full (not reduced-accuracy) feasibility tolerance of 1e-9.
        let norm = |v: &[f64]| v.iter().map(|v| v * v).sum::<f64>().sqrt();
        let residual = norm(&gradient);
        let scale = (norm(&linear) + norm(x) + norm(dual)).max(1.);
        #[cfg(feature = "audit")]
        eprintln!(
            "MIQ near optimum: stationarity {:.3e}, complementarity {:.3e}",
            residual / scale,
            gap / objective.abs().max(1.)
        );
        if !residual.is_finite()
            || !scale.is_finite()
            || !gap.is_finite()
            || !objective.is_finite()
            || residual > 1e-9 * scale
            || gap > 5e-5 * objective.abs().max(1.)
        {
            return Err(fail());
        }
        Ok(())
    }

    pub(super) fn validate_solution(&self, x: &[f64], fixed: &BTreeMap<usize, f64>) -> Result<()> {
        if x.len() != self.variables
            || x.iter().any(|x| !x.is_finite())
            || self
                .equalities
                .iter()
                .any(|(row, target)| (evaluate(row, x) - target).abs() > 1e-6)
            || self
                .inequalities
                .iter()
                .any(|(row, target)| evaluate(row, x) < target - 1e-7)
            || fixed.iter().any(|(&i, &v)| (x[i] - v).abs() > 1e-6)
        {
            return Err("Integer-grid relaxation failed independent residual validation".into());
        }
        Ok(())
    }

    /// MIQ's warm iterative update after rounding. Eliminate seam and fixed
    /// equations exactly, then solve the remaining positive quadratic form.
    /// The caller checks orientation and uses a constrained relaxation if this
    /// unconstrained minimizer leaves the valid-map region.
    pub fn update(
        &mut self,
        fixed: &BTreeMap<usize, f64>,
        initial: &[f64],
        budget: &mut Budget,
    ) -> Result<Vec<f64>> {
        if self.dual.as_ref().is_some_and(|cached| {
            cached.fixed == *fixed
                && cached.rows.len() < self.inequalities.len()
                && self.inequalities.len() <= MAX_DUAL_BOUNDS
        }) {
            // New orientation bounds invalidate the previous active-set guess.
            // Try the bounded dual directly; its exact-system checks decide
            // whether cached factors/products remain reusable after elimination.
            if let Ok(x) = self.update_active(fixed, initial, &Default::default(), true, budget) {
                self.validate_solution(&x, fixed)?;
                return Ok(x);
            }
            budget.check()?;
        }
        match self.update_bounds(fixed, initial, budget) {
            Ok(x) => Ok(x),
            Err(error) => {
                budget.check()?;
                if self.inequalities.is_empty() || self.inequalities.len() > MAX_DUAL_BOUNDS {
                    return Err(error);
                }
                // A tentative active set can be inconsistent even when the
                // inequality problem is feasible. Solve its small dual before
                // falling back to the full chart/seam KKT system.
                let x = self.update_active(fixed, initial, &Default::default(), true, budget)?;
                self.validate_solution(&x, fixed)?;
                Ok(x)
            }
        }
    }

    fn update_bounds(
        &mut self,
        fixed: &BTreeMap<usize, f64>,
        initial: &[f64],
        budget: &mut Budget,
    ) -> Result<Vec<f64>> {
        let mut binding: std::collections::BTreeSet<_> = self
            .inequalities
            .iter()
            .enumerate()
            .filter(|(_, (row, b))| (evaluate(row, initial) - b).abs() < 1e-7)
            .map(|(i, _)| i)
            .collect();
        binding.extend(
            self.binding
                .iter()
                .copied()
                .filter(|&i| i < self.inequalities.len()),
        );
        let mut initial = initial.to_vec();
        for _ in 0..8 {
            let x = self.update_active(fixed, &initial, &binding, false, budget)?;
            let violated: Vec<_> = self
                .inequalities
                .iter()
                .enumerate()
                .filter(|(_, (row, b))| evaluate(row, &x) < b - 1e-8)
                .map(|(i, _)| i)
                .collect();
            if violated.is_empty() {
                self.binding = binding.into_iter().collect();
                return Ok(x);
            }
            let before = binding.len();
            binding.extend(violated);
            if binding.len() == before {
                break;
            }
            initial = x;
        }
        Err("Integer-grid active constraints require a full relaxation".into())
    }

    fn update_active(
        &mut self,
        fixed: &BTreeMap<usize, f64>,
        initial: &[f64],
        binding: &std::collections::BTreeSet<usize>,
        constrained: bool,
        budget: &mut Budget,
    ) -> Result<Vec<f64>> {
        budget.check()?;
        #[cfg(feature = "audit")]
        let started = Instant::now();
        let mut basis = Elimination::new(self.variables);
        for (i, (row, rhs)) in self.equalities.iter().enumerate() {
            if i % 128 == 0 {
                budget.check()?;
            }
            basis.add(row, *rhs, &self.integers)?;
        }
        for (&i, &value) in fixed {
            basis.add(&vec![(i, 1.)], value, &self.integers)?;
        }
        for &i in binding {
            let (row, b) = &self.inequalities[i];
            basis.add(row, *b, &self.integers)?;
        }
        basis.expand()?;
        #[cfg(feature = "audit")]
        let eliminated = started.elapsed().as_secs_f64();
        let mut matrix = vec![BTreeMap::<usize, f64>::new(); self.variables];
        let mut rhs = vec![0.; self.variables];
        let mut entries = 0;
        for (i, (row, target)) in self.energy.iter().enumerate() {
            if i % 1024 == 0 {
                budget.check()?;
            }
            let (row, constant) = basis.substitute(row)?;
            let target = target - constant;
            for (&a, &u) in &row {
                rhs[a] += u * target;
                for (&b, &v) in &row {
                    if let std::collections::btree_map::Entry::Vacant(e) = matrix[a].entry(b) {
                        e.insert(u * v);
                        entries += 1;
                    } else {
                        *matrix[a].get_mut(&b).unwrap() += u * v;
                    }
                }
            }
            if entries > 4_000_000 {
                return Err("Integer-grid iterative system exceeds its fill budget".into());
            }
        }
        let matrix: Vec<Vec<(usize, f64)>> = matrix
            .into_iter()
            .map(|r| r.into_iter().collect())
            .collect();
        let diagonal: Vec<_> = matrix
            .iter()
            .enumerate()
            .map(|(i, row)| {
                row.iter()
                    .find(|(j, _)| *j == i)
                    .map(|(_, a)| *a)
                    .unwrap_or(1.)
            })
            .collect();
        if diagonal.iter().any(|d| !d.is_finite() || *d <= 0.) {
            return Err("Integer-grid iterative system is not positive definite".into());
        }
        #[cfg(feature = "audit")]
        let assembled = started.elapsed().as_secs_f64();
        let multiply = |x: &[f64]| {
            matrix
                .iter()
                .map(|row| row.iter().map(|&(j, a)| a * x[j]).sum::<f64>())
                .collect::<Vec<_>>()
        };
        let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
        let mut x = initial.to_vec();
        let mut residual: Vec<_> = rhs.iter().zip(multiply(&x)).map(|(b, a)| b - a).collect();
        if let Some(cached) = &self.dual
            && cached.matrix == matrix
        {
            // This is the exact reduced system, not just a preconditioner.
            // Keep the same residual/refinement checks below.
            self.factor = Some(cached.factor.clone());
        }
        // A previous factor stays an SPD preconditioner when restricted to the
        // remaining free coordinates. Changed constraints need only a few CG
        // corrections; the independently checked residual still decides success.
        if self
            .factor
            .as_ref()
            .is_some_and(|factor| !factor.covers(&matrix))
        {
            self.factor = None;
        }
        let mut z = precondition(&mut self.factor, &matrix, &diagonal, &residual, budget)?;
        let mut direction = z.clone();
        let mut rho = dot(&residual, &z);
        let tolerance = 1e-16 * dot(&rhs, &rhs).max(1.);
        if !tolerance.is_finite() {
            return Err("Integer-grid linear system has a nonfinite residual scale".into());
        }
        #[cfg(feature = "audit")]
        let mut iterations = 0;
        for iteration in 0..1500 {
            if dot(&residual, &residual) <= tolerance {
                // Recurrence residuals drift on the thin triangles of a cut.
                // Restart from the actual residual inside the same iteration
                // budget; only the original recomputed tolerance can stop us.
                residual = rhs.iter().zip(multiply(&x)).map(|(b, a)| b - a).collect();
                if dot(&residual, &residual) <= tolerance * 4. {
                    break;
                }
                z = precondition(&mut self.factor, &matrix, &diagonal, &residual, budget)?;
                direction = z.clone();
                rho = dot(&residual, &z);
            }
            if iteration % 8 == 0 {
                budget.check()?;
            }
            // Most rounding changes are local. If a short warm iteration does
            // not settle them, factor the reduced system instead of repeatedly
            // sweeping the whole map (MIQ 2009, section 2.1).
            if iteration == 32 {
                match direct_factor(&matrix, budget) {
                    Ok(mut factor) => {
                        x = factor.solve(&rhs, budget)?;
                        // Bounded iterative refinement uses the same factor,
                        // without regularization or a looser acceptance bound.
                        for _ in 0..3 {
                            let residual: Vec<_> =
                                rhs.iter().zip(multiply(&x)).map(|(b, a)| b - a).collect();
                            if dot(&residual, &residual) <= tolerance {
                                break;
                            }
                            let correction = factor.solve(&residual, budget)?;
                            for (x, dx) in x.iter_mut().zip(correction) {
                                *x += dx;
                            }
                        }
                        self.factor = Some(factor);
                        break;
                    }
                    Err(error) => {
                        #[cfg(feature = "audit")]
                        eprintln!("MIQ direct fallback: {error}");
                        #[cfg(not(feature = "audit"))]
                        let _ = error;
                        budget.check()?; // retain bounded CG fallback
                    }
                }
            }
            #[cfg(feature = "audit")]
            {
                iterations = iteration + 1;
            }
            let image = multiply(&direction);
            let denominator = dot(&direction, &image);
            if !denominator.is_finite() || denominator <= 0. {
                return Err("Integer-grid iterative solve lost positive curvature".into());
            }
            let alpha = rho / denominator;
            for i in 0..x.len() {
                x[i] += alpha * direction[i];
                residual[i] -= alpha * image[i];
            }
            z = precondition(&mut self.factor, &matrix, &diagonal, &residual, budget)?;
            let next = dot(&residual, &z);
            for i in 0..x.len() {
                direction[i] = z[i] + (next / rho) * direction[i];
            }
            rho = next;
        }
        let residual: Vec<_> = rhs.iter().zip(multiply(&x)).map(|(b, a)| b - a).collect();
        #[cfg(feature = "audit")]
        eprintln!(
            "MIQ linear update: elimination {eliminated:.3}s, assembly {:.3}s, solve {:.3}s/{iterations} CG iterations, {entries} entries, {} active bounds",
            assembled - eliminated,
            started.elapsed().as_secs_f64() - assembled,
            binding.len()
        );
        let residual_norm = dot(&residual, &residual);
        if !residual_norm.is_finite() || residual_norm > tolerance * 4. {
            return Err("Integer-grid iterative solve exceeded its residual budget".into());
        }
        if constrained {
            self.factor = None;
            let (next, factor, binding) = dual_update(
                &matrix,
                &rhs,
                &basis,
                &x,
                &self.inequalities,
                &mut self.dual,
                budget,
            )?;
            x = next;
            self.dual.as_mut().unwrap().fixed = fixed.clone();
            self.factor = Some(factor);
            self.binding = binding;
        }
        for (i, expression) in basis.rows.iter().enumerate() {
            if let Some((row, constant)) = expression {
                x[i] = constant + row.iter().map(|(&j, &a)| a * x[j]).sum::<f64>();
            }
        }
        if x.iter().any(|v| !v.is_finite())
            || self
                .equalities
                .iter()
                .any(|(row, b)| (evaluate(row, &x) - b).abs() > 1e-6)
            || fixed.iter().any(|(&i, &v)| (x[i] - v).abs() > 1e-6)
        {
            return Err("Integer-grid iterative update failed seam validation".into());
        }
        Ok(x)
    }
}

const MAX_DUAL_BOUNDS: usize = 1024;
const MAX_DUAL_ACTIVE: usize = 512;

struct DualCache {
    // Scheduling hint only; numerical reuse checks H and every transformed row.
    fixed: BTreeMap<usize, f64>,
    matrix: Vec<Row>,
    rows: Vec<Row>,
    factor: DirectFactor,
    schur: Vec<Option<Vec<f64>>>,
}

/// For Hx=g, Cx>=b, eliminate x to solve the nonnegative dual:
/// min 1/2 λᵀ(C H⁻¹ Cᵀ)λ + (Cx₀-b)ᵀλ, then x=x₀+H⁻¹Cᵀλ.
/// At most 1024 bounds are considered, with <=512 in each small QP and
/// eight separation rounds. Mesh-sized inverse columns are never retained.
/// Only needed Schur columns are cached (at most 4 MiB). Every inverse solve
/// and every original inequality is checked before returning a candidate.
fn dual_update(
    matrix: &[Vec<(usize, f64)>],
    rhs: &[f64],
    basis: &Elimination,
    initial: &[f64],
    inequalities: &[(Row, f64)],
    cache: &mut Option<DualCache>,
    budget: &mut Budget,
) -> Result<(Vec<f64>, DirectFactor, Vec<usize>)> {
    budget.check()?;
    if inequalities.len() > MAX_DUAL_BOUNDS {
        return Err("Integer-grid dual exceeds its bound budget".into());
    }
    let started = Instant::now();
    let multiply = |x: &[f64]| {
        matrix
            .iter()
            .map(|row| row.iter().map(|&(j, a)| a * x[j]).sum::<f64>())
            .collect::<Vec<_>>()
    };
    let norm2 = |x: &[f64]| x.iter().map(|v| v * v).sum::<f64>();
    let solve = |factor: &mut DirectFactor, b: &[f64], budget: &Budget| -> Result<Vec<f64>> {
        let tolerance = 4e-16 * norm2(b).max(1.);
        let mut x = factor.solve(b, budget)?;
        for iteration in 0..=3 {
            let residual: Vec<_> = b.iter().zip(multiply(&x)).map(|(b, a)| b - a).collect();
            let error = norm2(&residual);
            if error.is_finite() && error <= tolerance {
                return Ok(x);
            }
            if iteration == 3 {
                break;
            }
            let correction = factor.solve(&residual, budget)?;
            for (x, dx) in x.iter_mut().zip(correction) {
                *x += dx;
            }
        }
        Err("Integer-grid dual inverse failed residual validation".into())
    };
    let constraints: Vec<_> = inequalities
        .iter()
        .map(|(row, b)| {
            let (row, constant) = basis.substitute(row)?;
            Ok((row.into_iter().collect::<Row>(), b - constant))
        })
        .collect::<Result<_>>()?;
    // Reuse only an exactly matching eliminated Hessian and unchanged
    // constraint-row prefix. RHS/bound constants are recomputed every time.
    let reusable = cache.as_ref().is_some_and(|cached| {
        cached.matrix.as_slice() == matrix
            && cached.rows.len() <= constraints.len()
            && cached
                .rows
                .iter()
                .zip(&constraints)
                .all(|(a, (b, _))| a == b)
    });
    if !reusable {
        *cache = None; // release the previous system before allocating another
        let factor = direct_factor(matrix, budget)?; // validate limits before copying
        *cache = Some(DualCache {
            fixed: BTreeMap::new(),
            matrix: matrix.to_vec(),
            rows: Vec::new(),
            factor,
            schur: Vec::new(),
        });
    }
    let cached = cache.as_mut().unwrap();
    cached.rows.extend(
        constraints[cached.rows.len()..]
            .iter()
            .map(|(row, _)| row.clone()),
    );
    cached.schur.resize_with(constraints.len(), || None);
    let factor = &mut cached.factor;
    let schur = &mut cached.schur;
    let linear: Vec<_> = constraints
        .iter()
        .map(|(row, b)| evaluate(row, initial) - b)
        .collect();
    let mut active: std::collections::BTreeSet<_> = linear
        .iter()
        .enumerate()
        .filter_map(|(i, &q)| (q < 1e-8).then_some(i))
        .collect();
    let mut vector = vec![0.; matrix.len()];
    for _ in 0..8 {
        budget.check()?;
        if active.len() > MAX_DUAL_ACTIVE {
            return Err("Integer-grid dual exceeds its active bound budget".into());
        }
        let ids: Vec<_> = active.iter().copied().collect();
        if ids.is_empty() {
            return Ok((initial.to_vec(), factor.clone(), Vec::new()));
        }
        let retained = schur.iter().filter(|column| column.is_some()).count();
        let missing = ids.iter().filter(|&&id| schur[id].is_none()).count();
        if retained + missing > MAX_DUAL_ACTIVE {
            // Bound retained columns across calls as well as within one QP.
            schur.iter_mut().for_each(|column| *column = None);
        }
        for &id in &ids {
            if schur[id].is_some() {
                continue;
            }
            vector.fill(0.);
            for &(i, a) in &constraints[id].0 {
                vector[i] = a;
            }
            let column = solve(factor, &vector, budget)?;
            schur[id] = Some(
                constraints
                    .iter()
                    .map(|(row, _)| evaluate(row, &column))
                    .collect(),
            );
        }
        let mut entries = BTreeMap::new();
        for (j, &id) in ids.iter().enumerate() {
            for (i, &other) in ids[..=j].iter().enumerate() {
                // Old columns need no new mesh-sized inverse solve when
                // rows are appended: use symmetry and the new column instead.
                let a = schur[id].as_ref().unwrap().get(other).copied();
                let b = schur[other].as_ref().unwrap().get(id).copied();
                let value = match (a, b) {
                    (Some(a), Some(b)) => (a + b) * 0.5,
                    (Some(value), None) | (None, Some(value)) => value,
                    (None, None) => return Err("Integer-grid dual cache is incomplete".into()),
                };
                if !value.is_finite() {
                    return Err("Integer-grid dual matrix is nonfinite".into());
                }
                entries.insert((j, i), value);
            }
        }
        let n = ids.len();
        let p = sparse(n, n, entries);
        let q: Vec<_> = ids.iter().map(|&i| linear[i]).collect();
        let a = sparse(n, n, (0..n).map(|i| ((i, i), -1.)).collect());
        budget.check()?;
        let mut solver = DefaultSolver::new(
            &p,
            &q,
            &a,
            &vec![0.; n],
            &[NonnegativeConeT(n)],
            DefaultSettings {
                verbose: false,
                max_iter: 100,
                time_limit: (60. - budget.start.elapsed().as_secs_f64()).min(10.),
                // The small dual needs a tighter gap to identify its active
                // bounds reliably for the next warm rounding update.
                tol_feas: 1e-10,
                tol_gap_abs: 1e-12,
                tol_gap_rel: 1e-10,
                ..Default::default()
            },
        )
        .map_err(|e| format!("Cannot construct integer-grid dual: {e}"))?;
        let cancel = budget.cancel.clone();
        solver.set_termination_callback(move |_: &DefaultInfo<f64>| cancel.load(Ordering::Relaxed));
        budget.solves += 1;
        solver.solve();
        budget.check()?;
        if solver.solution.status != SolverStatus::Solved
            || solver.solution.x.iter().any(|v| !v.is_finite() || *v < 0.)
        {
            return Err("Integer-grid dual requires a full relaxation".into());
        }
        vector.fill(0.);
        for (&id, &weight) in ids.iter().zip(&solver.solution.x) {
            for &(i, a) in &constraints[id].0 {
                vector[i] += a * weight;
            }
        }
        let correction = solve(factor, &vector, budget)?;
        let x: Vec<_> = initial
            .iter()
            .zip(correction)
            .map(|(x, dx)| x + dx)
            .collect();
        let image = multiply(&x);
        let residual: Vec<_> = rhs
            .iter()
            .zip(&vector)
            .zip(image)
            .map(|((b, c), a)| b + c - a)
            .collect();
        let error = norm2(&residual);
        let scale = (norm2(rhs) + norm2(&vector)).max(1.);
        if !error.is_finite() || !scale.is_finite() || error > 4e-16 * scale {
            return Err("Integer-grid dual failed independent residual validation".into());
        }
        let violated: Vec<_> = constraints
            .iter()
            .enumerate()
            .filter_map(|(i, (row, b))| (evaluate(row, &x) < b - 1e-8).then_some(i))
            .collect();
        if violated.is_empty() {
            // Seed the next warm update with the dual's active bounds. Scale
            // the multiplier by its Schur diagonal to compare primal units;
            // raw multiplier/slack comparisons misclassify thin-cap rows.
            let binding = ids
                .iter()
                .zip(&solver.solution.x)
                .filter_map(|(&id, &weight)| {
                    let slack = (evaluate(&constraints[id].0, &x) - constraints[id].1).abs();
                    (slack <= 1e-7 && weight * schur[id].as_ref().unwrap()[id] > slack.max(1e-8))
                        .then_some(id)
                })
                .collect();
            #[cfg(feature = "audit")]
            eprintln!(
                "MIQ dual update: {n}/{} bounds in {:.3}s",
                constraints.len(),
                started.elapsed().as_secs_f64()
            );
            #[cfg(not(feature = "audit"))]
            let _ = started;
            return Ok((x, factor.clone(), binding));
        }
        let before = active.len();
        active.extend(violated);
        if active.len() == before {
            break;
        }
    }
    Err("Integer-grid dual exceeded its separation budget".into())
}

/// Sparse LDL with explicit symbolic fill/work limits *before* factor allocation.
/// AMD is already the ordering used by Clarabel. Ordering and numeric factorization
/// have no cancellation callback, so only bounded systems enter those operations.
fn direct_factor(matrix: &[Vec<(usize, f64)>], budget: &Budget) -> Result<DirectFactor> {
    budget.check()?;
    let free: Vec<_> = (0..matrix.len())
        .filter(|&i| !matrix[i].is_empty())
        .collect();
    let n = free.len();
    if n == 0 || n > 100_000 || matrix.iter().map(Vec::len).sum::<usize>() > 4_000_000 {
        return Err("Integer-grid direct system exceeds its size budget".into());
    }
    let mut indices = vec![usize::MAX; matrix.len()];
    for (i, &old) in free.iter().enumerate() {
        indices[old] = i;
    }
    let mut pointers = vec![0];
    let mut rows = Vec::new();
    for &old in &free {
        for &(j, _) in &matrix[old] {
            if j <= old {
                rows.push(indices[j]);
            }
        }
        pointers.push(rows.len());
    }
    let (permutation, inverse, info) = amd::order(n, &pointers, &rows, &amd::Control::default())
        .map_err(|_| "Integer-grid direct ordering failed")?;
    #[cfg(feature = "audit")]
    eprintln!(
        "MIQ ordering estimate: {} entries, {} work",
        info.lnz, info.n_mult_subs_ldl
    );
    #[cfg(not(feature = "audit"))]
    let _ = info;
    budget.check()?;
    let mut columns = vec![Vec::new(); n];
    for &old in &free {
        let a = inverse[indices[old]];
        for &(j, value) in &matrix[old] {
            if j <= old {
                let b = inverse[indices[j]];
                columns[a.max(b)].push((a.min(b), value));
            }
        }
    }
    let mut pointers = vec![0];
    let mut rows = Vec::new();
    let mut values = Vec::new();
    for column in &mut columns {
        column.sort_unstable_by_key(|&(i, _)| i);
        for &(i, value) in column.iter() {
            rows.push(i);
            values.push(value);
        }
        pointers.push(rows.len());
    }
    let variables = matrix.len();
    let matrix = CscMatrix::new(n, n, pointers, rows, values);
    let fill = factor_work(&matrix, budget)?;
    budget.check()?;
    let factor = QDLDLFactorisation::new(
        &matrix,
        Some(QDLDLSettings {
            perm: Some((0..n).collect()),
            regularize_enable: false,
            ..Default::default()
        }),
    )
    .map_err(|error| format!("Integer-grid direct factorization failed: {error}"))?;
    budget.check()?;
    if factor.positive_inertia() != n
        || factor.nnzL() != fill
        || factor.D.iter().any(|d| !d.is_finite() || *d <= 0.)
    {
        return Err("Integer-grid direct factor failed positive-definite/fill validation".into());
    }
    #[cfg(test)]
    budget.factorizations.set(budget.factorizations.get() + 1);
    Ok(DirectFactor {
        factor: Arc::new(std::sync::Mutex::new(factor)),
        indices: Arc::new(permutation.iter().map(|&i| free[i]).collect()),
        free: Arc::new(indices.iter().map(|&i| i != usize::MAX).collect()),
        variables,
    })
}

#[derive(Clone)]
struct DirectFactor {
    // The solver mutates only its scratch workspace. Shared ownership lets a
    // warm preconditioner and the exact-system cache retain one factor safely.
    factor: Arc<std::sync::Mutex<QDLDLFactorisation<f64>>>,
    indices: Arc<Vec<usize>>,
    free: Arc<Vec<bool>>,
    variables: usize,
}
impl DirectFactor {
    fn covers(&self, matrix: &[Vec<(usize, f64)>]) -> bool {
        self.variables == matrix.len()
            && matrix
                .iter()
                .zip(self.free.iter())
                .all(|(row, &free)| row.is_empty() || free)
    }

    fn solve(&mut self, rhs: &[f64], budget: &Budget) -> Result<Vec<f64>> {
        budget.check()?;
        let mut solution: Vec<_> = self.indices.iter().map(|&i| rhs[i]).collect();
        self.factor
            .lock()
            .map_err(|_| "Integer-grid factor workspace was interrupted")?
            .solve(&mut solution);
        #[cfg(test)]
        budget.direct_solves.set(budget.direct_solves.get() + 1);
        let mut x = vec![0.; self.variables];
        for (&i, value) in self.indices.iter().zip(solution) {
            x[i] = value;
        }
        budget.check()?;
        Ok(x)
    }
}

fn precondition(
    factor: &mut Option<DirectFactor>,
    matrix: &[Vec<(usize, f64)>],
    diagonal: &[f64],
    residual: &[f64],
    budget: &Budget,
) -> Result<Vec<f64>> {
    if let Some(factor) = factor {
        let mut z = factor.solve(residual, budget)?;
        for (value, row) in z.iter_mut().zip(matrix) {
            if row.is_empty() {
                *value = 0.;
            }
        }
        Ok(z)
    } else {
        Ok(residual.iter().zip(diagonal).map(|(r, d)| r / d).collect())
    }
}

/// Walk the elimination tree's reach for each column. Each visited ancestor is
/// one strict-lower factor entry; column counts also bound numeric multiply-adds.
/// Stop while counting, rather than allocating a potentially dense factor first.
fn factor_work(matrix: &CscMatrix<f64>, budget: &Budget) -> Result<usize> {
    let n = matrix.n;
    let mut parent = vec![usize::MAX; n];
    let mut visited = vec![usize::MAX; n];
    let mut counts = vec![0usize; n];
    let (mut fill, mut work) = (0usize, 0usize);
    for column in 0..n {
        if column % 128 == 0 {
            budget.check()?;
        }
        visited[column] = column;
        for &row in &matrix.rowval[matrix.colptr[column]..matrix.colptr[column + 1]] {
            let mut node = row;
            while visited[node] != column {
                visited[node] = column;
                if parent[node] == usize::MAX {
                    parent[node] = column;
                }
                counts[node] += 1;
                fill += 1;
                work += counts[node];
                if fill > 4_000_000 || work > 500_000_000 {
                    return Err("Integer-grid direct factor exceeds its fill/work budget".into());
                }
                node = parent[node];
            }
        }
    }
    #[cfg(feature = "audit")]
    eprintln!("MIQ direct factor: {n} variables, {fill} entries, {work} work");
    Ok(fill)
}

type Expression = (BTreeMap<usize, f64>, f64);
struct Elimination {
    rows: Vec<Option<Expression>>,
    order: Vec<usize>,
}
impl Elimination {
    fn new(n: usize) -> Self {
        Self {
            rows: vec![None; n],
            order: Vec::new(),
        }
    }
    fn add(&mut self, input: &Row, mut rhs: f64, integers: &[usize]) -> Result<()> {
        let mut row = BTreeMap::new();
        for &(i, a) in input {
            *row.entry(i).or_insert(0.) += a;
        }
        loop {
            row.retain(|_, a| a.abs() > 1e-10);
            let Some(p) = row.keys().copied().find(|&i| self.rows[i].is_some()) else {
                break;
            };
            let a = row.remove(&p).unwrap();
            let (expression, constant) = self.rows[p].as_ref().unwrap();
            rhs -= a * constant;
            for (&j, &b) in expression {
                *row.entry(j).or_insert(0.) += a * b;
            }
            if row.len() > 4096 {
                return Err("Integer-grid seam elimination exceeds its fill budget".into());
            }
        }
        // Partial pivoting matters for thin-cap half-spaces: their first
        // coefficient can be orders of magnitude smaller than another one.
        // Prefer noninteger pivots, then the largest coefficient, with stable
        // index ties. Dividing by a tiny first coefficient loses the solution.
        let pivot = |avoid_integers: bool| {
            row.iter()
                .filter(|(i, _)| !avoid_integers || !integers.contains(i))
                .max_by(|(i, a), (j, b)| a.abs().total_cmp(&b.abs()).then(j.cmp(i)))
                .map(|(&i, _)| i)
        };
        let Some(p) = pivot(true).or_else(|| pivot(false)) else {
            return if rhs.abs() < 1e-6 {
                Ok(())
            } else {
                Err(
                    "Integer-grid relaxation did not converge: PrimalInfeasible (seam equations)"
                        .into(),
                )
            };
        };
        let a = row.remove(&p).unwrap();
        for b in row.values_mut() {
            *b /= -a;
        }
        self.rows[p] = Some((row, rhs / a));
        self.order.push(p);
        Ok(())
    }
    fn expand(&mut self) -> Result<()> {
        for &p in self.order.iter().rev() {
            let (row, constant) = self.rows[p].take().unwrap();
            let row: Row = row.into_iter().collect();
            let (row, added) = self.substitute(&row)?;
            self.rows[p] = Some((row, constant + added));
        }
        Ok(())
    }
    fn substitute(&self, input: &Row) -> Result<Expression> {
        let mut result = BTreeMap::new();
        let mut constant = 0.;
        for &(i, a) in input {
            if let Some((row, b)) = &self.rows[i] {
                constant += a * b;
                for (&j, &c) in row {
                    *result.entry(j).or_insert(0.) += a * c;
                }
            } else {
                *result.entry(i).or_insert(0.) += a;
            }
        }
        result.retain(|_, a| a.abs() > 1e-12);
        if result.len() > 4096 {
            return Err("Integer-grid substitution exceeds its fill budget".into());
        }
        Ok((result, constant))
    }
}

pub(super) fn evaluate(row: &Row, x: &[f64]) -> f64 {
    row.iter().map(|&(i, a)| a * x[i]).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_solved_relaxation_requires_independent_primal_dual_certificate() {
        let mut p = Problem::new(3);
        p.energy = vec![
            (vec![(0, 1.)], 0.2),
            (vec![(1, 1.)], 0.7),
            (vec![(2, 1.)], 2.),
        ];
        p.equalities.push((vec![(0, 1.), (1, -1.)], 0.));
        p.inequalities.push((vec![(0, 1.)], 0.4));
        let fixed = BTreeMap::from([(2, 1.)]);
        let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
        // Analytical optimum, with equality, fixed-coordinate and bound duals.
        let initial = vec![0.45, 0.45, 1.];
        let dual = [-0.5, 2., 0.];
        let x = p
            .finish_relaxation(
                SolverStatus::AlmostSolved,
                initial.clone(),
                &dual,
                &fixed,
                &mut budget,
            )
            .unwrap();
        assert_eq!(x, initial);
        for invalid in [
            vec![0.6, 0.6, 1.],
            vec![0.6, 0.7, 1.],
            vec![0.3, 0.3, 1.],
            vec![0.6, 0.6, 2.],
            vec![f64::NAN, 0.6, 1.],
            vec![0.6],
        ] {
            assert!(
                p.finish_relaxation(
                    SolverStatus::AlmostSolved,
                    invalid,
                    &dual,
                    &fixed,
                    &mut budget
                )
                .is_err()
            );
        }
        for invalid in [
            vec![],
            vec![0., 2., 0.],
            vec![-0.5, 2., -1.],
            vec![f64::NAN, 2., 0.],
        ] {
            assert!(
                p.finish_relaxation(
                    SolverStatus::AlmostSolved,
                    x.clone(),
                    &invalid,
                    &fixed,
                    &mut budget
                )
                .is_err()
            );
        }
        for status in [
            SolverStatus::PrimalInfeasible,
            SolverStatus::AlmostPrimalInfeasible,
            SolverStatus::MaxTime,
            SolverStatus::NumericalError,
        ] {
            assert!(
                p.finish_relaxation(status, x.clone(), &dual, &fixed, &mut budget)
                    .is_err()
            );
        }
        budget.cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            p.finish_relaxation(SolverStatus::AlmostSolved, x, &dual, &fixed, &mut budget)
                .unwrap_err(),
            "Retopology cancelled"
        );
    }

    #[test]
    fn near_solved_bounds_are_not_all_forced_to_equalities() {
        let mut p = Problem::new(1);
        p.energy.push((vec![(0, 1.)], 0.));
        p.inequalities = vec![(vec![(0, 1.)], 1.), (vec![(0, 1.)], 2.)];
        let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
        let fixed = BTreeMap::new();
        // Both bounds are violated by the unconstrained minimum x=0. Forcing
        // both equalities during cleanup falsely declares this feasible QP
        // inconsistent. The independent oracle and certificate agree at x=2.
        let oracle = p.solve(&fixed, &mut budget).unwrap();
        let x = 2. + 1e-9;
        let certified = p
            .finish_relaxation(
                SolverStatus::AlmostSolved,
                vec![x],
                &[0., 2. * x],
                &fixed,
                &mut budget,
            )
            .unwrap();
        assert!((certified[0] - oracle[0]).abs() < 1e-7);
        let updated = p.update(&fixed, &[3.], &mut budget).unwrap();
        assert!((updated[0] - oracle[0]).abs() < 1e-7);
        // A fresh full solve can release the cached active bound. A later
        // warm update must follow the changed energy, not remain pinned at 2.
        p.energy[0].1 = 3.;
        let released = p.solve(&fixed, &mut budget).unwrap();
        let released = p.update(&fixed, &released, &mut budget).unwrap();
        assert!((released[0] - 3.).abs() < 1e-7);
        p.energy[0].1 = 0.;
        // Stationarity alone is insufficient: this feasible point and dual
        // have a large complementarity gap and must be rejected.
        assert!(
            p.finish_relaxation(
                SolverStatus::AlmostSolved,
                vec![3.],
                &[0., 6.],
                &fixed,
                &mut budget
            )
            .is_err()
        );
    }

    #[test]
    fn dual_cache_reuses_appended_bounds_and_invalidates_changed_systems() {
        let mut matrix = vec![vec![(0, 1.)], vec![(1, 1.)]];
        let basis = Elimination::new(2);
        let mut bounds = vec![(vec![(0, 1.)], 1.)];
        let mut cache = None;
        let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
        let close = |actual: &[f64], expected: [f64; 2]| {
            for (a, b) in actual.iter().zip(expected) {
                assert!((a - b).abs() < 1e-7, "{actual:?} != {expected:?}");
            }
        };
        let (x, _, _) = dual_update(
            &matrix,
            &[0., 0.],
            &basis,
            &[0., 0.],
            &bounds,
            &mut cache,
            &mut budget,
        )
        .unwrap();
        close(&x, [1., 0.]);
        let factors = budget.factorizations.get();
        bounds.push((vec![(1, 1.)], 2.));
        let (x, _, _) = dual_update(
            &matrix,
            &[0., 0.],
            &basis,
            &[0., 0.],
            &bounds,
            &mut cache,
            &mut budget,
        )
        .unwrap();
        close(&x, [1., 2.]);
        assert_eq!(
            budget.factorizations.get(),
            factors,
            "appending a bound reuses the same system"
        );
        // Changed Hessian coefficients must invalidate the numerical factor.
        matrix[0][0].1 = 2.;
        let (x, _, _) = dual_update(
            &matrix,
            &[0., 0.],
            &basis,
            &[0., 0.],
            &bounds,
            &mut cache,
            &mut budget,
        )
        .unwrap();
        close(&x, [1., 2.]);
        assert_eq!(budget.factorizations.get(), factors + 1);
        // A changed prefix must not reuse old Schur products either.
        bounds[0] = (vec![(0, 1.), (1, 1.)], 6.);
        let (x, _, _) = dual_update(
            &matrix,
            &[0., 0.],
            &basis,
            &[0., 0.],
            &bounds,
            &mut cache,
            &mut budget,
        )
        .unwrap();
        close(&x, [2., 4.]);
        assert_eq!(budget.factorizations.get(), factors + 2);
        // Linear terms are not cached: H and C can remain identical while
        // the unconstrained minimizer and the constrained answer both change.
        let (x, _, _) = dual_update(
            &matrix,
            &[0., 3.],
            &basis,
            &[0., 3.],
            &bounds,
            &mut cache,
            &mut budget,
        )
        .unwrap();
        close(&x, [1., 5.]);
        assert_eq!(budget.factorizations.get(), factors + 2);
        bounds[0].1 = 3.6;
        let (x, _, _) = dual_update(
            &matrix,
            &[0., 0.],
            &basis,
            &[0., 0.],
            &bounds,
            &mut cache,
            &mut budget,
        )
        .unwrap();
        close(&x, [1.2, 2.4]);
        assert_eq!(
            budget.factorizations.get(),
            factors + 2,
            "bound constants are recomputed too"
        );
        budget.cancel.store(true, Ordering::Relaxed);
        assert!(
            dual_update(
                &matrix,
                &[0., 3.],
                &basis,
                &[0., 3.],
                &bounds,
                &mut cache,
                &mut budget
            )
            .err()
            .unwrap()
            .contains("cancelled")
        );
    }

    #[test]
    fn small_dual_update_matches_oracle_with_seams_fixed_values_and_redundant_bounds() {
        let mut p = Problem::new(4);
        p.energy = vec![
            (vec![(0, 1.)], 0.),
            (vec![(1, 1.)], 0.7),
            (vec![(2, 1.)], 0.7),
            (vec![(3, 1.)], 2.),
        ];
        p.equalities.push((vec![(1, 1.), (2, -1.)], 0.));
        p.inequalities = vec![
            (vec![(0, 1.)], 1.),
            (vec![(0, 1.)], 2.),
            (vec![(0, 1.), (1, -1.)], 1.5),
            (vec![(1, 1.)], 0.55),
        ];
        let fixed = BTreeMap::from([(3, 1.)]);
        let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
        let oracle = p.solve(&fixed, &mut budget).unwrap();
        let solves = budget.solves;
        let result = p.update(&fixed, &[3., 0.7, 0.7, 1.], &mut budget).unwrap();
        for ((actual, oracle), analytic) in result.iter().zip(oracle).zip([2.05, 0.55, 0.55, 1.]) {
            assert!((actual - oracle).abs() < 1e-7);
            assert!((actual - analytic).abs() < 1e-7);
        }
        assert!(
            budget.solves >= solves + 2,
            "add the initially satisfied y bound after the first small solve"
        );
        assert!(
            budget.factorizations.get() > 0,
            "exercise the small dual fallback"
        );
        let solves = budget.solves;
        let changed_fixed = BTreeMap::from([(3, 1.1)]);
        let reused = p.update(&changed_fixed, &result, &mut budget).unwrap();
        assert_eq!(
            budget.solves, solves,
            "reuse the dual's active set without another QP"
        );
        let factors = budget.factorizations.get();
        let solves = budget.solves;
        p.inequalities.push((vec![(0, 1.)], 3.));
        let appended = p.update(&fixed, &result, &mut budget).unwrap();
        for (actual, expected) in appended.iter().zip([3., 0.7, 0.7, 1.]) {
            assert!((actual - expected).abs() < 1e-7);
        }
        assert_eq!(budget.factorizations.get(), factors);
        assert!(
            budget.solves > solves,
            "new bounds take the cached dual path"
        );
        for (actual, expected) in reused.iter().zip([2.05, 0.55, 0.55, 1.1]) {
            assert!((actual - expected).abs() < 1e-7);
        }
        let before = budget.factorizations.get();
        let oversized = vec![(vec![(0, 1.)], 1.); MAX_DUAL_BOUNDS + 1];
        assert!(
            dual_update(
                &[vec![(0, 1.)]],
                &[0.],
                &Elimination::new(1),
                &[0.],
                &oversized,
                &mut None,
                &mut budget
            )
            .err()
            .unwrap()
            .contains("bound budget")
        );
        assert_eq!(budget.factorizations.get(), before);
        budget.cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            p.update(&fixed, &result, &mut budget).unwrap_err(),
            "Retopology cancelled"
        );
    }

    #[test]
    fn thin_half_space_elimination_preserves_the_small_solution_coordinate() {
        for epsilon in [1e-6, 1e-7] {
            let mut p = Problem::new(2);
            p.energy = vec![(vec![(0, 1.)], 0.), (vec![(1, 1.)], 0.)];
            p.inequalities.push((vec![(0, epsilon), (1, 1.)], 1.));
            let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
            let x = p.update(&BTreeMap::new(), &[0., 1.], &mut budget).unwrap();
            assert!((x[0] - epsilon / (1. + epsilon * epsilon)).abs() < 1e-12);
            assert!((x[1] - 1. / (1. + epsilon * epsilon)).abs() < 1e-12);
        }
    }

    #[test]
    fn sparse_rounding_update_matches_constrained_oracle() {
        // A weakly anchored, anisotropic grid makes plain diagonal CG expensive.
        // Compare the shortcut with a separate constrained solver, including a
        // seam equation, a rounded coordinate and an active lower bound.
        let side = 32;
        let n = side * side;
        let mut p = Problem::new(n);
        for y in 0..side {
            for x in 0..side {
                let i = y * side + x;
                for (j, weight) in [(i + 1, 1.), (i + side, 0.3)] {
                    if (j == i + 1 && x + 1 < side) || (j == i + side && y + 1 < side) {
                        p.energy.push((vec![(i, weight), (j, -weight)], 0.));
                    }
                }
            }
        }
        p.energy.push((vec![(n - 1, 0.1)], 0.));
        p.equalities.push((vec![(side - 1, 1.), (n - 1, -1.)], 0.));
        p.inequalities.push((vec![(n - 1, 1.)], 1.5));
        p.integers.push(0);
        let fixed = BTreeMap::from([(0, 1.)]);
        let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
        let expected = p.solve(&fixed, &mut budget).unwrap();
        let updated = p.update(&fixed, &vec![0.; n], &mut budget).unwrap();
        assert!(
            budget.direct_solves.get() > 0,
            "must exercise the fast path"
        );
        for (a, b) in expected.iter().zip(&updated) {
            assert!((a - b).abs() < 1e-6, "oracle {a}, update {b}");
        }
        assert_eq!(updated[0], 1.);
        assert!((updated[n - 1] - 1.5).abs() < 1e-8);
        assert!((updated[side - 1] - updated[n - 1]).abs() < 1e-8);

        let factorizations = budget.factorizations.get();
        let more_fixed = BTreeMap::from([(0, 1.), (n / 2, 0.)]);
        let expected = p.solve(&more_fixed, &mut budget).unwrap();
        let updated = p.update(&more_fixed, &updated, &mut budget).unwrap();
        assert_eq!(
            budget.factorizations.get(),
            factorizations,
            "reuse the existing factor"
        );
        for (a, b) in expected.iter().zip(&updated) {
            assert!((a - b).abs() < 1e-6, "oracle {a}, reused factor {b}");
        }

        // Releasing an originally eliminated coordinate must invalidate a
        // preconditioner that cannot represent it (e.g. changed active bounds).
        let expected = p.solve(&BTreeMap::new(), &mut budget).unwrap();
        let updated = p.update(&BTreeMap::new(), &updated, &mut budget).unwrap();
        assert!(budget.factorizations.get() > factorizations);
        for (a, b) in expected.iter().zip(updated) {
            assert!((a - b).abs() < 1e-6, "oracle {a}, rebuilt factor {b}");
        }
    }

    #[test]
    fn direct_factor_is_bounded_before_allocation_and_honours_cancellation() {
        let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
        // Eliminating the hub first turns this sparse star into a dense factor.
        // Its numerical work is rejected during symbolic counting.
        let n = 2_000;
        let entries = (0..n).flat_map(|i| [((i, i), 2.), ((i, 0), 1.)]).collect();
        let star = sparse(n, n, entries);
        assert!(
            factor_work(&star, &budget)
                .unwrap_err()
                .contains("fill/work budget")
        );
        assert_eq!(budget.direct_solves.get(), 0);
        // Never perturb a nonpositive system into a seemingly valid solution.
        assert!(direct_factor(&[vec![(0, -1.)]], &budget).is_err());
        assert!(direct_factor(&[vec![(0, f64::INFINITY)]], &budget).is_err());
        budget.cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            direct_factor(&[vec![(0, 1.)]], &budget).err().unwrap(),
            "Retopology cancelled"
        );
        assert_eq!(
            factor_work(&star, &budget).unwrap_err(),
            "Retopology cancelled"
        );
        assert!(
            Problem::new(1)
                .update(&BTreeMap::new(), &[0.], &mut budget)
                .is_err()
        );
    }

    #[test]
    fn coupled_relaxation_honours_equalities_bounds_and_fixed_integers() {
        let mut p = Problem::new(3);
        p.energy = vec![
            (vec![(0, 1.)], 0.2),
            (vec![(1, 1.)], 0.7),
            (vec![(2, 1.)], 2.),
        ];
        p.equalities.push((vec![(0, 1.), (1, -1.)], 0.));
        p.inequalities.push((vec![(0, 1.)], 0.4));
        let mut budget = Budget::new(Arc::new(AtomicBool::new(false)));
        let x = p.solve(&BTreeMap::from([(2, 1.)]), &mut budget).unwrap();
        assert!((x[0] - 0.45).abs() < 1e-6);
        assert!((x[1] - 0.45).abs() < 1e-6);
        assert!((x[2] - 1.).abs() < 1e-6);
        let updated = p
            .update(&BTreeMap::from([(2, 1.)]), &[0., 0., 2.], &mut budget)
            .unwrap();
        for (a, b) in x.iter().zip(updated) {
            assert!((a - b).abs() < 1e-6);
        }
        assert!(p.solve(&BTreeMap::from([(0, 0.)]), &mut budget).is_err());
        budget.cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            p.solve(&BTreeMap::new(), &mut budget).unwrap_err(),
            "Retopology cancelled"
        );
    }
}
