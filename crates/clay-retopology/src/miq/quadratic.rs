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
        }
    }

    pub fn solve(&self, fixed: &BTreeMap<usize, f64>, budget: &mut Budget) -> Result<Vec<f64>> {
        budget.check()?;
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
        let settings = DefaultSettings {
            verbose: false,
            max_iter: 100,
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
        budget.check()?;
        if solver.solution.status != SolverStatus::Solved {
            return Err(format!(
                "Integer-grid relaxation did not converge: {:?}",
                solver.solution.status
            ));
        }
        let x = solver.solution.x;
        if x.iter().any(|x| !x.is_finite())
            || self
                .equalities
                .iter()
                .any(|(row, target)| (evaluate(row, &x) - target).abs() > 1e-6)
            || self
                .inequalities
                .iter()
                .any(|(row, target)| evaluate(row, &x) < target - 1e-7)
            || fixed.iter().any(|(&i, &v)| (x[i] - v).abs() > 1e-6)
        {
            return Err("Integer-grid relaxation failed independent residual validation".into());
        }
        Ok(x)
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
        let mut binding: std::collections::BTreeSet<_> = self
            .inequalities
            .iter()
            .enumerate()
            .filter(|(_, (row, b))| (evaluate(row, initial) - b).abs() < 1e-7)
            .map(|(i, _)| i)
            .collect();
        let mut initial = initial.to_vec();
        for _ in 0..8 {
            let x = self.update_active(fixed, &initial, &binding, budget)?;
            let violated: Vec<_> = self
                .inequalities
                .iter()
                .enumerate()
                .filter(|(_, (row, b))| evaluate(row, &x) < b - 1e-8)
                .map(|(i, _)| i)
                .collect();
            if violated.is_empty() {
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
                break;
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
        factor,
        indices: permutation.iter().map(|&i| free[i]).collect(),
        free: indices.iter().map(|&i| i != usize::MAX).collect(),
        variables,
    })
}

struct DirectFactor {
    factor: QDLDLFactorisation<f64>,
    indices: Vec<usize>,
    free: Vec<bool>,
    variables: usize,
}
impl DirectFactor {
    fn covers(&self, matrix: &[Vec<(usize, f64)>]) -> bool {
        self.variables == matrix.len()
            && matrix
                .iter()
                .zip(&self.free)
                .all(|(row, &free)| row.is_empty() || free)
    }

    fn solve(&mut self, rhs: &[f64], budget: &Budget) -> Result<Vec<f64>> {
        budget.check()?;
        let mut solution: Vec<_> = self.indices.iter().map(|&i| rhs[i]).collect();
        self.factor.solve(&mut solution);
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
        let Some(p) = row
            .keys()
            .copied()
            .find(|i| !integers.contains(i))
            .or_else(|| row.keys().next().copied())
        else {
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
