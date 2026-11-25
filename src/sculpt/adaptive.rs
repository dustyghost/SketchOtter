//! Local, conforming edge splits and conservative manifold edge collapses.
use super::{
    Alpha, Brush, Dab, Kind, Mesh, Vec3,
    alpha::Projection,
    math::closest_triangle,
    subdivision::{max_triangles, max_vertices},
};
mod quads;
mod relief;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

// Face index followed by its directed edge and opposite vertex.
type OrientedFaceEdge = (usize, usize, usize, usize);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdaptiveSettings {
    #[serde(default = "super::Topology::triangles")]
    pub topology: super::Topology,
    pub unmasked_only: bool,
    pub enabled: bool,
    pub detail: f32,
    pub relative: bool,
    pub refine: bool,
    pub simplify: bool,
    pub collapse_ratio: f32,
}
impl Default for AdaptiveSettings {
    fn default() -> Self {
        Self {
            topology: super::Topology::Triangles,
            unmasked_only: false,
            enabled: false,
            detail: 8.,
            relative: false,
            refine: true,
            simplify: true,
            collapse_ratio: 0.4,
        }
    }
}
impl AdaptiveSettings {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.detail.is_finite()
            || !(1. ..=32.).contains(&self.detail)
            || !self.collapse_ratio.is_finite()
            || !(0.1..=0.8).contains(&self.collapse_ratio)
        {
            return Err("Invalid adaptive sculpt settings".into());
        }
        Ok(())
    }
    pub(crate) fn collapse_fraction(&self, kind: Kind) -> f32 {
        if matches!(kind, Kind::Decimate | Kind::Smooth) {
            (self.collapse_ratio * 2.).min(0.95)
        } else {
            self.collapse_ratio
        }
    }
    pub fn edge_length(&self, brush: &Brush, dab: Dab) -> f32 {
        if self.relative {
            dab.radius * self.detail / 100.
        } else {
            let pixels = brush.radius
                * if brush.pressure_radius {
                    dab.pressure.powf(brush.pressure_curve).max(0.05)
                } else {
                    1.
                };
            dab.radius * self.detail / pixels
        }
    }
}
#[derive(Clone, Copy, Default)]
struct SplitOptions {
    relief: bool,
    unmasked_only: bool,
}
#[derive(Clone, Copy)]
struct CollapseOptions {
    threshold: f32,
    limit: usize,
    unmasked_only: bool,
    preserve_surface: bool,
}

/// Bound topology work per rendered input frame, shared by all interpolated dabs.
pub(crate) struct Budget {
    pub splits: usize,
    pub collapses: usize,
    fan_splits: usize,
    passes: usize,
    flips: usize,
    flip_passes: usize,
    relax: bool,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            splits: 256,
            collapses: 256,
            fan_splits: 8,
            passes: 8,
            flips: 128,
            flip_passes: 2,
            relax: true,
        }
    }
}

/// Symmetry dabs and their matching projected masks, in the same order.
#[derive(Clone, Copy)]
pub(crate) struct Region<'a> {
    pub dabs: &'a [Dab],
    pub stencils: &'a [Projection],
}
impl Region<'_> {
    fn permits_reconnection(&self, brush: &Brush, points: &[Vec3]) -> bool {
        self.stencils.is_empty()
            || self.stencils.iter().zip(self.dabs).any(|(stencil, dab)| {
                // A different symmetry projector must not grant permission unless
                // its corresponding brush dab actually covers the same region.
                (!brush.pressure || dab.pressure > 0.)
                    && points.iter().all(|p| {
                        let offset = *p - dab.center;
                        offset.len() <= dab.radius * 1.5
                            && brush.weight(
                                offset.dot(dab.right) / dab.radius,
                                offset.dot(dab.up) / dab.radius,
                            ) > 0.001
                    })
                    && stencil.coverage_bounds(points).0 >= 0.999
            })
    }
}

impl Mesh {
    fn adaptive_protected_vertices(&self, unmasked_only: bool) -> Vec<bool> {
        let mut protected = vec![false; self.vertices.len()];
        for &v in self
            .loose_edges
            .iter()
            .flatten()
            .chain(self.loose_vertices.iter())
        {
            protected[v] = true;
        }
        for (fi, t) in self.triangles.iter().enumerate() {
            if !self.face_visible(fi) || (unmasked_only && t.iter().any(|&v| self.mask[v] > 0.)) {
                for &v in t {
                    protected[v] = true;
                }
            }
        }
        if unmasked_only {
            for q in &self.quads {
                if q.iter().any(|&v| self.mask[v] > 0.) {
                    for &v in q {
                        protected[v] = true;
                    }
                }
            }
        }
        protected
    }
    fn retain_adaptive_quads(&mut self) {
        if self.quads.is_empty() {
            return;
        }
        let key = |mut t: [usize; 3]| {
            t.sort_unstable();
            t
        };
        let faces: HashSet<_> = self.triangles.iter().copied().map(key).collect();
        self.quads.retain(|&[a, b, c, d]| {
            a != b
                && a != c
                && a != d
                && b != c
                && b != d
                && c != d
                && ((faces.contains(&key([a, b, c])) && faces.contains(&key([a, c, d])))
                    || (faces.contains(&key([a, b, d])) && faces.contains(&key([b, c, d]))))
        });
    }
    /// A placed stamp has no later dabs to finish refinement. Resolve its footprint
    /// on the undeformed surface so new vertices sample the alpha, rather than
    /// merely subdividing an already jagged displacement afterwards.
    #[cfg(test)]
    pub(crate) fn prepare_drag_stamp(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        settings: &AdaptiveSettings,
    ) -> bool {
        self.prepare_stenciled_stamp(brush, dabs, &[], settings)
    }
    pub(crate) fn prepare_stenciled_stamp(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        settings: &AdaptiveSettings,
    ) -> bool {
        self.prepare_stenciled_stamp_cancellable(brush, dabs, stencils, settings, &|| false)
    }
    pub(crate) fn prepare_stenciled_stamp_cancellable(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        settings: &AdaptiveSettings,
        cancelled: &impl Fn() -> bool,
    ) -> bool {
        if !self.creases.is_empty()
            || !settings.enabled
            || !settings.refine
            || !matches!(
                brush.effective_stroke_mode(),
                super::StrokeMode::DragRect | super::StrokeMode::DragDot
            )
            || matches!(
                brush.kind,
                Kind::Mask | Kind::Paint | Kind::Smooth | Kind::Relax | Kind::Decimate
            )
            || brush.strength <= 0.
            || dabs.is_empty()
        {
            return false;
        }
        let target = settings.edge_length(brush, dabs[0]);
        if !target.is_finite() || target <= 1e-7 {
            return false;
        }
        if settings.topology == super::Topology::Quads {
            return self.prepare_quad_stamp(brush, Region { dabs, stencils }, settings, cancelled);
        }
        let relief = brush.kind == Kind::Standard
            && (matches!(brush.alpha, Alpha::Image { .. }) || !stencils.is_empty());
        // Relief error needs more local samples than the edge-length heuristic.
        // Keep an explicit work ceiling and larger batches for interactive previews.
        let mut remaining = if relief { 16384 } else { 8192 };
        let mut changed = false;
        for _ in 0..32 {
            if cancelled() {
                break;
            }
            let count = self.split_local_impl::<true>(
                brush,
                dabs,
                stencils,
                target,
                remaining.min(if relief { 1024 } else { 512 }),
                SplitOptions {
                    relief: true,
                    unmasked_only: settings.unmasked_only,
                },
            );
            changed |= count > 0;
            let flips = if relief {
                // Reconnect on the undeformed surface so quality improvement
                // cannot flatten the relief or interpolate its heights afterwards.
                self.improve_edges(brush, dabs, stencils, 512, settings.unmasked_only, false)
            } else {
                0
            };
            changed |= flips > 0;
            remaining -= count;
            if (count == 0 && flips == 0) || remaining == 0 {
                break;
            }
        }
        changed
    }
    #[cfg(test)]
    pub(crate) fn adapt(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        settings: &AdaptiveSettings,
        budget: &mut Budget,
        collapse: bool,
    ) -> bool {
        self.adapt_stenciled(
            brush,
            Region {
                dabs,
                stencils: &[],
            },
            settings,
            budget,
            collapse,
        )
    }
    pub(crate) fn adapt_stenciled(
        &mut self,
        brush: &Brush,
        region: Region<'_>,
        settings: &AdaptiveSettings,
        budget: &mut Budget,
        collapse: bool,
    ) -> bool {
        let Region { dabs, stencils } = region;
        if !self.creases.is_empty()
            || !settings.enabled
            || matches!(brush.kind, Kind::Mask | Kind::Paint | Kind::Relax)
            || brush.strength <= 0.
            || dabs.is_empty()
            || matches!(&brush.alpha,Alpha::Image { pixels,.. } if pixels.iter().all(|&p|p==0.))
        {
            return false;
        }
        let target = settings.edge_length(brush, dabs[0]);
        if !target.is_finite() || target <= 1e-7 {
            return false;
        }
        if settings.topology == super::Topology::Quads {
            return self.adapt_quads(brush, region, settings, budget, collapse);
        }
        let cleanup = brush.kind == Kind::Decimate;
        let smoothing = brush.kind == Kind::Smooth;
        let mut changed = false;
        let mut topology_changed = false;
        // Smooth must relax the surface before considering any contractions.
        // Collapsing first can remove the very geometry that needs to move.
        let collapse_now = if smoothing { !collapse } else { collapse };
        if smoothing && !collapse && settings.simplify && budget.fan_splits > 0 {
            let count = self.collapse_short_detail_cycles(
                brush,
                dabs,
                stencils,
                target * settings.collapse_fraction(brush.kind),
                budget.fan_splits,
                settings.unmasked_only,
            );
            budget.fan_splits -= count;
            topology_changed = count > 0;
            changed |= count > 0;
        }
        if collapse_now
            && !topology_changed
            && (settings.simplify || cleanup)
            && budget.collapses > 0
        {
            let count = self.collapse_local(
                brush,
                dabs,
                stencils,
                CollapseOptions {
                    threshold: (if brush.kind == Kind::Detail {
                        target.min(dabs[0].radius * super::detail::CORE_EDGE_FRACTION)
                    } else {
                        target
                    }) * settings.collapse_fraction(brush.kind),
                    // Cleanup can remove more independent edges in the same mesh
                    // scan. Deformation brushes retain their smaller allowance.
                    limit: budget.collapses.min(if cleanup { 256 } else { 32 }),
                    unmasked_only: settings.unmasked_only,
                    preserve_surface: cleanup || smoothing,
                },
            );
            budget.collapses = 0;
            changed |= count > 0;
        }
        if settings.refine && !cleanup && !smoothing && budget.splits > 0 {
            for _ in 0..4 {
                if budget.passes == 0 {
                    break;
                }
                budget.passes -= 1;
                let count = self.split_local(
                    brush,
                    dabs,
                    stencils,
                    target,
                    budget.splits.min(64),
                    settings.unmasked_only,
                );
                budget.splits -= count;
                changed |= count > 0;
                if count == 0 || budget.splits == 0 {
                    break;
                }
            }
        }
        let smooth_remesh = smoothing && !collapse && settings.simplify;
        if ((!cleanup && !smoothing && settings.refine) || smooth_remesh)
            && budget.flips > 0
            && budget.flip_passes > 0
        {
            budget.flip_passes -= 1;
            let count = self.improve_edges(
                brush,
                dabs,
                stencils,
                budget.flips.min(64),
                settings.unmasked_only,
                smooth_remesh,
            );
            budget.flips -= count;
            changed |= count > 0;
        }
        let smooth_rebalance = smooth_remesh && budget.relax;
        let deformation_relax = matches!(brush.kind, Kind::Snake | Kind::Detail)
            && !collapse
            && settings.refine
            && budget.relax;
        if smooth_rebalance || deformation_relax {
            budget.relax = false;
            changed |= self.relax_surface(brush, dabs, stencils, target, settings.unmasked_only);
        }
        changed
    }
    /// Separate closed face fans that have become pinched together at one
    /// vertex. This is a local topology event: unlike an edge collapse it
    /// repairs a disconnected vertex link without opening either surface.
    #[cfg(test)]
    fn split_disconnected_fans(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        limit: usize,
        unmasked_only: bool,
    ) -> usize {
        if limit == 0 || self.triangles.is_empty() {
            return 0;
        }
        let protected = self.adaptive_protected_vertices(unmasked_only);
        let nearby_faces = self.patch_candidates(brush, dabs);
        let mut in_region = vec![false; self.triangles.len()];
        let mut candidates = vec![false; self.vertices.len()];
        for (fi, &face) in self.triangles.iter().enumerate() {
            if nearby_faces[fi]
                && self.face_visible(fi)
                && self.topology_face_in_region(fi, face, brush, dabs, stencils)
            {
                in_region[fi] = true;
                for v in face {
                    candidates[v] = true;
                }
            }
        }

        let mut incident = vec![Vec::new(); self.vertices.len()];
        for (fi, face) in self.triangles.iter().enumerate() {
            for &v in face {
                if candidates[v] {
                    incident[v].push(fi);
                }
            }
        }
        let mut splits = Vec::<(usize, Vec<Vec<usize>>)>::new();
        let mut split_count = 0;
        for vertex in 0..self.vertices.len() {
            if !candidates[vertex] || protected[vertex] || split_count >= limit {
                continue;
            }
            let star = &incident[vertex];
            if star.len() < 4 || star.iter().any(|&fi| !in_region[fi]) {
                continue;
            }

            // Faces in one fan are adjacent across an edge incident to this
            // vertex. A closed fan has exactly two faces on each such edge.
            let mut radial = BTreeMap::<usize, Vec<usize>>::new();
            for &fi in star {
                for &other in &self.triangles[fi] {
                    if other != vertex {
                        radial.entry(other).or_default().push(fi);
                    }
                }
            }
            if radial.values().any(|faces| faces.len() != 2) {
                continue;
            }
            let mut adjacency = HashMap::<usize, Vec<usize>>::new();
            for faces in radial.values() {
                adjacency.entry(faces[0]).or_default().push(faces[1]);
                adjacency.entry(faces[1]).or_default().push(faces[0]);
            }
            let mut unseen: HashSet<_> = star.iter().copied().collect();
            let mut fans = Vec::<Vec<usize>>::new();
            while let Some(&root) = unseen.iter().min() {
                unseen.remove(&root);
                let mut fan = vec![root];
                let mut pending = vec![root];
                while let Some(face) = pending.pop() {
                    let mut neighbors = adjacency.get(&face).cloned().unwrap_or_default();
                    neighbors.sort_unstable();
                    for next in neighbors {
                        if unseen.remove(&next) {
                            fan.push(next);
                            pending.push(next);
                        }
                    }
                }
                fan.sort_unstable();
                fans.push(fan);
            }
            if fans.len() < 2 {
                continue;
            }
            fans.sort_by_key(|fan| fan[0]);
            let duplicates = fans.len().saturating_sub(1).min(limit - split_count);
            split_count += duplicates;
            splits.push((vertex, fans.into_iter().skip(1).take(duplicates).collect()));
        }
        if split_count == 0 {
            return 0;
        }

        // Clone only after a disconnected fan has actually been found. Ordinary
        // Smooth dabs otherwise avoid copying the complete mesh for this check.
        let mut trial = self.clone();
        let mut face_replacements = HashMap::<(usize, usize), usize>::new();
        for (vertex, fans) in splits {
            for fan in fans {
                let duplicate = trial.vertices.len();
                trial.vertices.push(trial.vertices[vertex]);
                trial.mask.push(trial.mask[vertex]);
                if !trial.materials.is_empty() {
                    trial.materials.push(trial.material(vertex));
                }
                for group in &mut trial.vertex_groups {
                    group.weights.push(group.weights[vertex]);
                }
                for &fi in &fan {
                    face_replacements.insert((fi, vertex), duplicate);
                    for corner in &mut trial.triangles[fi] {
                        if *corner == vertex {
                            *corner = duplicate;
                        }
                    }
                }
            }
        }

        // UVs are face-corner data, so their values remain attached to their
        // faces. Retarget seam edges onto each newly separated copy.
        if trial.uv.current(self) {
            let old_seams = trial.uv.seams.clone();
            let mut seams = Vec::new();
            for seam in old_seams {
                let mut mapped = Vec::new();
                for (fi, face) in self.triangles.iter().enumerate() {
                    if seam.iter().all(|v| face.contains(v)) {
                        let edge =
                            seam.map(|v| face_replacements.get(&(fi, v)).copied().unwrap_or(v));
                        mapped.push(super::uv::edge(edge[0], edge[1]));
                    }
                }
                if mapped.is_empty() {
                    seams.push(seam);
                } else {
                    seams.extend(mapped);
                }
            }
            seams.sort_unstable();
            seams.dedup();
            trial.uv.seams = seams;
            trial.uv.topology = trial.triangles.clone();
        }
        trial.shading_seams = None;
        trial.retain_adaptive_quads();
        if trial.validate().is_err() {
            return 0;
        }
        *self = trial;
        split_count
    }
    /// Split a collapsed loop's vertex without building adjacency for the
    /// rest of the mesh. The caller already works on a disposable trial mesh.
    fn split_vertex_fan_at(
        &mut self,
        vertex: usize,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        limit: usize,
        unmasked_only: bool,
    ) -> usize {
        if vertex >= self.vertices.len() || limit == 0 {
            return 0;
        }
        let star: Vec<_> = self
            .triangles
            .iter()
            .enumerate()
            .filter(|(fi, face)| {
                face.contains(&vertex)
                    && self.face_visible(*fi)
                    && self.topology_face_in_region(*fi, **face, brush, dabs, stencils)
                    && (!unmasked_only || face.iter().all(|&v| self.mask[v] == 0.))
            })
            .map(|(fi, _)| fi)
            .collect();
        let complete_star = self
            .triangles
            .iter()
            .filter(|face| face.contains(&vertex))
            .count();
        if star.len() < 4 || star.len() != complete_star {
            return 0;
        }
        let mut radial = BTreeMap::<usize, Vec<usize>>::new();
        for &fi in &star {
            for &other in &self.triangles[fi] {
                if other != vertex {
                    radial.entry(other).or_default().push(fi);
                }
            }
        }
        if radial.values().any(|faces| faces.len() != 2) {
            return 0;
        }
        let mut adjacency = HashMap::<usize, Vec<usize>>::new();
        for faces in radial.values() {
            adjacency.entry(faces[0]).or_default().push(faces[1]);
            adjacency.entry(faces[1]).or_default().push(faces[0]);
        }
        let mut unseen: HashSet<_> = star.iter().copied().collect();
        let mut fans = Vec::<Vec<usize>>::new();
        while let Some(&root) = unseen.iter().min() {
            unseen.remove(&root);
            let mut fan = vec![root];
            let mut pending = vec![root];
            while let Some(face) = pending.pop() {
                for &next in adjacency.get(&face).into_iter().flatten() {
                    if unseen.remove(&next) {
                        fan.push(next);
                        pending.push(next);
                    }
                }
            }
            fan.sort_unstable();
            fans.push(fan);
        }
        if fans.len() < 2 {
            return 0;
        }
        fans.sort_by_key(|fan| fan[0]);

        let uv_current = self.uv.current(self);
        let old_seams = self.uv.seams.clone();
        let mut replacements = HashMap::<(usize, usize), usize>::new();
        let mut split_count = 0;
        for fan in fans.iter().skip(1).take(limit) {
            let duplicate = self.vertices.len();
            self.vertices.push(self.vertices[vertex]);
            self.mask.push(self.mask[vertex]);
            if !self.materials.is_empty() {
                self.materials.push(self.material(vertex));
            }
            for group in &mut self.vertex_groups {
                group.weights.push(group.weights[vertex]);
            }
            for &fi in fan {
                replacements.insert((fi, vertex), duplicate);
            }
            split_count += 1;
        }
        if uv_current {
            let mut seams = Vec::new();
            for seam in old_seams {
                let mut mapped = Vec::new();
                for (fi, face) in self.triangles.iter().enumerate() {
                    if seam.iter().all(|v| face.contains(v)) {
                        let edge = seam.map(|v| replacements.get(&(fi, v)).copied().unwrap_or(v));
                        mapped.push(super::uv::edge(edge[0], edge[1]));
                    }
                }
                if mapped.is_empty() {
                    seams.push(seam);
                } else {
                    seams.extend(mapped);
                }
            }
            seams.sort_unstable();
            seams.dedup();
            self.uv.seams = seams;
        }
        for (&(fi, old_vertex), &new_vertex) in &replacements {
            for corner in &mut self.triangles[fi] {
                if *corner == old_vertex {
                    *corner = new_vertex;
                }
            }
        }
        if uv_current {
            self.uv.topology = self.triangles.clone();
        }
        self.shading_seams = None;
        self.retain_adaptive_quads();
        split_count
    }
    /// Find a compact loop of sub-detail edges and contract it as one event.
    /// A pinched neck separates into closed fans; a terminal fan collapses
    /// into the adjacent surface. Trials must remain closed and non-branching.
    fn collapse_short_detail_cycles(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        threshold: f32,
        limit: usize,
        unmasked_only: bool,
    ) -> usize {
        if limit == 0 || !threshold.is_finite() || threshold <= 1e-7 {
            return 0;
        }
        let nearby_faces = self.patch_candidates(brush, dabs);
        let mut edge_faces = HashMap::<(usize, usize), Vec<usize>>::new();
        for (fi, &face) in self.triangles.iter().enumerate() {
            if !nearby_faces[fi]
                || !self.face_visible(fi)
                || !self.topology_face_in_region(fi, face, brush, dabs, stencils)
            {
                continue;
            }
            let [a, b, c] = face;
            for (u, v) in [(a, b), (b, c), (c, a)] {
                edge_faces.entry(edge(u, v)).or_default().push(fi);
            }
        }
        if edge_faces.is_empty() {
            return 0;
        }
        let mut vertex_edges = vec![Vec::<f32>::new(); self.vertices.len()];
        for &(a, b) in edge_faces.keys() {
            let length = (self.vertices[a] - self.vertices[b]).len();
            vertex_edges[a].push(length);
            vertex_edges[b].push(length);
        }
        let local_scale: Vec<_> = vertex_edges
            .iter()
            .map(|edges| {
                let mut lengths = edges.clone();
                lengths.sort_by(f32::total_cmp);
                lengths
                    .get(lengths.len() / 2)
                    .copied()
                    .unwrap_or(f32::INFINITY)
            })
            .collect();
        let influenced_in_same_dab = |a: usize, b: usize| {
            dabs.iter().any(|dab| {
                let pressure = if brush.pressure {
                    dab.pressure.powf(brush.pressure_curve)
                } else {
                    1.
                };
                [a, b].into_iter().all(|i| {
                    let offset = self.vertices[i] - dab.center;
                    offset.len() <= dab.radius
                        && brush.weight(
                            offset.dot(dab.right) / dab.radius,
                            offset.dot(dab.up) / dab.radius,
                        ) * pressure
                            * (1. - self.mask[i])
                            > 0.001
                })
            })
        };
        let mut adjacency = BTreeMap::<usize, Vec<usize>>::new();
        for (&(a, b), faces) in &edge_faces {
            let length = (self.vertices[a] - self.vertices[b]).len();
            if faces.len() != 2
                || self.mask[a] > 0.95
                || self.mask[b] > 0.95
                || (self.mask[a] - self.mask[b]).abs() > 0.05
                || (unmasked_only && (self.mask[a] > 0. || self.mask[b] > 0.))
                || length >= threshold
                // A contact loop is made from locally collapsed edges, not
                // merely from an ordinarily dense patch below target size.
                || length * 2. >= local_scale[a]
                || length * 2. >= local_scale[b]
                || !influenced_in_same_dab(a, b)
            {
                continue;
            }
            if !self.polygroups.is_empty() && self.polygroups[faces[0]] != self.polygroups[faces[1]]
            {
                continue;
            }
            if !stencils.is_empty() {
                let points: Vec<_> = faces
                    .iter()
                    .flat_map(|&fi| self.triangles[fi].map(|v| self.vertices[v]))
                    .collect();
                if !(Region { dabs, stencils }).permits_reconnection(brush, &points) {
                    continue;
                }
            }
            adjacency.entry(a).or_default().push(b);
            adjacency.entry(b).or_default().push(a);
        }
        for neighbors in adjacency.values_mut() {
            neighbors.sort_unstable();
        }
        let mut unseen: HashSet<_> = adjacency.keys().copied().collect();
        let mut cycles = Vec::<Vec<usize>>::new();
        while let Some(&root) = unseen.iter().min() {
            unseen.remove(&root);
            let mut component = vec![root];
            let mut pending = vec![root];
            while let Some(v) = pending.pop() {
                for &next in &adjacency[&v] {
                    if unseen.remove(&next) {
                        component.push(next);
                        pending.push(next);
                    }
                }
            }
            if component.len() < 4 || component.iter().any(|v| adjacency[v].len() != 2) {
                continue;
            }
            let start = *component.iter().min().unwrap();
            let mut cycle = vec![start];
            let mut previous = start;
            let mut current = adjacency[&start][0];
            while current != start && cycle.len() <= component.len() {
                cycle.push(current);
                let [a, b] = adjacency[&current].as_slice() else {
                    break;
                };
                let next = if *a == previous { *b } else { *a };
                previous = current;
                current = next;
            }
            if current != start || cycle.len() != component.len() {
                continue;
            }
            let diameter = cycle
                .iter()
                .flat_map(|&a| cycle.iter().map(move |&b| (a, b)))
                .map(|(a, b)| (self.vertices[a] - self.vertices[b]).len())
                .fold(0_f32, f32::max);
            if diameter <= threshold * 2. {
                cycles.push(cycle);
            }
        }
        let stroke_distance = |cycle: &[usize]| {
            let center = cycle
                .iter()
                .map(|&v| self.vertices[v])
                .fold(Vec3::default(), |sum, p| sum + p)
                * (1. / cycle.len() as f32);
            dabs.iter()
                .map(|dab| (center - dab.center).len())
                .fold(f32::INFINITY, f32::min)
        };
        cycles.sort_by(|a, b| {
            stroke_distance(a)
                .total_cmp(&stroke_distance(b))
                .then(a.len().cmp(&b.len()))
                .then(a.cmp(b))
        });
        if cycles.is_empty() {
            return 0;
        }
        let protected = self.adaptive_protected_vertices(unmasked_only);
        cycles.retain(|cycle| cycle.iter().all(|&vertex| !protected[vertex]));
        if cycles.is_empty() {
            return 0;
        }

        // A failed transactional trial still needs mesh validation. Keep that
        // expensive fallback to the nearest candidate for each dab.
        for cycle in cycles.into_iter().take(1) {
            let mut trial = self.clone();
            let Some((pinch_position, pinch_vertex)) = trial.contract_short_cycle(&cycle) else {
                continue;
            };
            let split = trial.split_vertex_fan_at(
                pinch_vertex,
                brush,
                dabs,
                stencils,
                limit,
                unmasked_only,
            );
            if split == 0 {
                trial = super::fusion::compact(trial);
            }
            let closed = if split > 0 {
                if trial.remove_sub_detail_pinched_component(
                    pinch_position,
                    threshold,
                    brush,
                    dabs,
                    stencils,
                ) {
                    true
                } else {
                    trial.validate_closed().is_ok()
                }
            } else {
                trial.validate_closed().is_ok()
            };
            if !closed {
                continue;
            }
            *self = trial;
            return 1;
        }
        0
    }
    #[cfg(test)]
    fn edge_connected_face_components(&self) -> usize {
        self.edge_connected_face_groups().len()
    }
    fn edge_connected_face_groups(&self) -> Vec<Vec<usize>> {
        let mut edge_faces = HashMap::<(usize, usize), Vec<usize>>::new();
        for (fi, &[a, b, c]) in self.triangles.iter().enumerate() {
            for (u, v) in [(a, b), (b, c), (c, a)] {
                edge_faces.entry(edge(u, v)).or_default().push(fi);
            }
        }
        let mut neighbors = vec![Vec::new(); self.triangles.len()];
        for faces in edge_faces.values() {
            for &a in faces {
                neighbors[a].extend(faces.iter().copied().filter(|&b| b != a));
            }
        }
        let mut visited = vec![false; self.triangles.len()];
        let mut components = Vec::new();
        for root in 0..visited.len() {
            if visited[root] {
                continue;
            }
            visited[root] = true;
            let mut pending = vec![root];
            let mut component = Vec::new();
            while let Some(face) = pending.pop() {
                component.push(face);
                for &next in &neighbors[face] {
                    if !visited[next] {
                        visited[next] = true;
                        pending.push(next);
                    }
                }
            }
            component.sort_unstable();
            components.push(component);
        }
        components
    }
    /// Remove a detached fragment only when its entire surface is below one
    /// adaptive detail cell and the stroke covers every face it would delete.
    fn remove_sub_detail_pinched_component(
        &mut self,
        pinch: Vec3,
        threshold: f32,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
    ) -> bool {
        let groups = self.edge_connected_face_groups();
        if groups.len() < 2 {
            return false;
        }
        let areas: Vec<_> = groups
            .iter()
            .map(|faces| {
                faces
                    .iter()
                    .map(|&fi| {
                        let [a, b, c] = self.triangles[fi].map(|v| self.vertices[v]);
                        (b - a).cross(c - a).len() * 0.5
                    })
                    .sum::<f32>()
            })
            .collect();
        let candidate = groups
            .iter()
            .enumerate()
            .filter(|(i, faces)| {
                areas[*i] <= threshold * threshold
                    && faces.iter().any(|&fi| {
                        self.triangles[fi]
                            .iter()
                            .any(|&v| self.vertices[v] == pinch)
                    })
                    && faces.iter().all(|&fi| {
                        self.topology_face_in_region(fi, self.triangles[fi], brush, dabs, stencils)
                            && self.triangles[fi].iter().all(|&v| self.mask[v] <= 0.001)
                    })
            })
            .min_by(|(a, _), (b, _)| areas[*a].total_cmp(&areas[*b]))
            .map(|(i, _)| i);
        let Some(candidate) = candidate else {
            return false;
        };
        if areas
            .iter()
            .enumerate()
            .any(|(i, area)| i != candidate && *area <= areas[candidate])
        {
            return false;
        }

        let removed: HashSet<_> = groups[candidate].iter().copied().collect();
        let kept: Vec<_> = (0..self.triangles.len())
            .filter(|fi| !removed.contains(fi))
            .collect();
        let mut trial = self.clone();
        let uv_current = trial.uv.current(self);
        trial.triangles = kept.iter().map(|&fi| trial.triangles[fi]).collect();
        if !trial.polygroups.is_empty() {
            trial.polygroups = kept.iter().map(|&fi| trial.polygroups[fi]).collect();
        }
        if !trial.hidden_faces.is_empty() {
            trial.hidden_faces = kept.iter().map(|&fi| trial.hidden_faces[fi]).collect();
        }
        if uv_current {
            trial.uv.corners = kept.iter().map(|&fi| trial.uv.corners[fi]).collect();
            if !trial.uv.masks.is_empty() {
                trial.uv.masks = kept.iter().map(|&fi| trial.uv.masks[fi]).collect();
            }
            if !trial.uv.removed.is_empty() {
                trial.uv.removed = kept.iter().map(|&fi| trial.uv.removed[fi]).collect();
            }
            trial.uv.topology = trial.triangles.clone();
        }
        trial.shading_seams = None;
        trial.retain_adaptive_quads();
        trial = super::fusion::compact(trial);
        if trial.validate_closed().is_err() {
            return false;
        }
        *self = trial;
        true
    }
    fn topology_face_in_region(
        &self,
        face: usize,
        triangle: [usize; 3],
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
    ) -> bool {
        let points = triangle.map(|v| self.vertices[v]);
        (Region { dabs, stencils }).permits_reconnection(brush, &points)
            && self.face_visible(face)
            && triangle.iter().all(|&v| self.mask[v] < 0.999)
    }
    // Redistribute dense sampling tangentially after smoothing, or vertices
    // compressed by dragging/pinching. Snake retains its depth; Detail also
    // damps small normal ripples between fresh triangles.
    fn relax_surface(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        target: f32,
        unmasked_only: bool,
    ) -> bool {
        let protected = self.adaptive_protected_vertices(unmasked_only);
        let mut changed = false;
        let sculptable = self.sculptable_vertices();
        for _ in 0..2 {
            let mut active = vec![false; self.vertices.len()];
            let nearby_faces = self.patch_candidates(brush, dabs);
            for (fi, &t) in self.triangles.iter().enumerate() {
                if nearby_faces[fi] && self.in_patch(fi, t, brush, dabs, stencils) {
                    for i in t {
                        active[i] = true;
                    }
                }
            }
            let (normals, sums, counts) =
                self.relaxation_neighborhood(&active, brush.kind == Kind::Smooth);
            let mut moves = vec![Vec3::default(); self.vertices.len()];
            for i in 0..moves.len() {
                if !sculptable[i]
                    || protected[i]
                    || !active[i]
                    || self.mask[i] >= 0.999
                    || counts[i] == 0.
                    || (brush.kind == Kind::Snake
                        && dabs
                            .iter()
                            .any(|dab| (self.vertices[i] - dab.center).len() < dab.radius * 0.35))
                {
                    continue;
                }
                let offset = sums[i] * (1. / counts[i]) - self.vertices[i];
                let tangent = offset - normals[i] * offset.dot(normals[i]);
                let influence = dabs
                    .iter()
                    .map(|dab| {
                        let offset = self.vertices[i] - dab.center;
                        let weight = if brush.kind == Kind::Snake {
                            brush.snake_weight(*dab, offset)
                        } else {
                            brush.weight(
                                offset.dot(dab.right) / dab.radius,
                                offset.dot(dab.up) / dab.radius,
                            )
                        };
                        if brush.kind == Kind::Smooth {
                            weight * brush.dab_pressure(dab) * brush.strength * (1. - self.mask[i])
                        } else {
                            weight
                        }
                    })
                    .fold(0f32, f32::max);
                let motion = if brush.kind == Kind::Detail {
                    // Regularize triangle-scale ridges left by refinement while
                    // keeping most of the incision's normal displacement.
                    tangent + normals[i] * (offset.dot(normals[i]) * 0.3)
                } else {
                    tangent
                };
                moves[i] = motion
                    * if brush.kind == Kind::Smooth {
                        0.5 * influence
                    } else {
                        0.5 * influence * (1. - self.mask[i])
                    };
                let length = moves[i].len();
                if length > target * 0.5 {
                    moves[i] = moves[i] * (target * 0.5 / length);
                }
            }
            self.constrain_surface_moves::<true>(&mut moves);
            for (position, delta) in self.vertices.iter_mut().zip(moves) {
                changed |= delta.len() > 0.;
                *position = *position + delta;
            }
        }
        changed
    }
    fn relaxation_neighborhood(
        &self,
        active: &[bool],
        area_weighted: bool,
    ) -> (Vec<Vec3>, Vec<Vec3>, Vec<f32>) {
        let mut normals = vec![Vec3::default(); self.vertices.len()];
        let mut sums = vec![Vec3::default(); self.vertices.len()];
        let mut counts = vec![0.; self.vertices.len()];
        let mut areas = area_weighted.then(|| vec![0.; self.vertices.len()]);
        if let Some(areas) = &mut areas {
            for &[a, b, c] in &self.triangles {
                let area = (self.vertices[b] - self.vertices[a])
                    .cross(self.vertices[c] - self.vertices[a])
                    .len()
                    / 6.;
                areas[a] += area;
                areas[b] += area;
                areas[c] += area;
            }
        }
        // Include all incident faces, even outside the brush. Retain the exact
        // face/addition order used by Mesh::normals and the full neighbor scan.
        for &[a, b, c] in &self.triangles {
            if !active[a] && !active[b] && !active[c] {
                continue;
            }
            let normal =
                (self.vertices[b] - self.vertices[a]).cross(self.vertices[c] - self.vertices[a]);
            for (i, j, k) in [(a, b, c), (b, c, a), (c, a, b)] {
                if active[i] {
                    normals[i] = normals[i] + normal;
                    if let Some(areas) = &areas {
                        sums[i] =
                            sums[i] + self.vertices[j] * areas[j] + self.vertices[k] * areas[k];
                        counts[i] += areas[j] + areas[k];
                    } else {
                        sums[i] = sums[i] + self.vertices[j] + self.vertices[k];
                        counts[i] += 2.;
                    }
                }
            }
        }
        for (i, normal) in normals.iter_mut().enumerate() {
            if active[i] {
                *normal = normal.norm();
            }
        }
        (normals, sums, counts)
    }
    fn constrain_surface_moves<const LOCAL: bool>(&self, moves: &mut [Vec3]) {
        // Simultaneous proposals must not invert or flatten any incident face.
        // Later passes only zero proposals, so no additional face can become
        // affected. Preserve face order while avoiding repeated remote scans.
        let affected: Vec<_> = self
            .triangles
            .iter()
            .copied()
            .filter(|t| !LOCAL || t.iter().any(|&i| moves[i].len() != 0.))
            .collect();
        for pass in 0..5 {
            let mut unsafe_vertices = HashSet::new();
            for &t in &affected {
                if t.iter().all(|&i| moves[i].len() == 0.) {
                    continue;
                }
                let [a, b, c] = t.map(|i| self.vertices[i]);
                let [x, y, z] = t.map(|i| self.vertices[i] + moves[i]);
                let old = (b - a).cross(c - a);
                let new = (y - x).cross(z - x);
                if new.len() < old.len() * 0.3 || new.norm().dot(old.norm()) < 0.8 {
                    unsafe_vertices.extend(t);
                }
            }
            if unsafe_vertices.is_empty() {
                break;
            }
            if pass == 4 {
                moves.fill(Vec3::default());
                break;
            }
            for i in unsafe_vertices {
                moves[i] = Vec3::default();
            }
        }
    }
    // Reconnect diagonals instead of repeatedly subdividing skinny triangles.
    // No vertex positions or mask values change, and distant faces stay intact.
    fn improve_edges(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        limit: usize,
        unmasked_only: bool,
        optimize_valence: bool,
    ) -> usize {
        self.improve_edges_impl::<true>(
            brush,
            dabs,
            stencils,
            limit,
            unmasked_only,
            optimize_valence,
        )
    }
    fn improve_edges_impl<const FLAT: bool>(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        limit: usize,
        unmasked_only: bool,
        optimize_valence: bool,
    ) -> usize {
        let protected = self.adaptive_protected_vertices(unmasked_only);
        let mut edges: Vec<((usize, usize), OrientedFaceEdge)> = Vec::new();
        let mut local_vertices = vec![false; self.vertices.len()];
        let mut valences = optimize_valence.then(|| vec![0; self.vertices.len()]);
        let nearby_faces = self.patch_candidates(brush, dabs);
        for (i, &[a, b, c]) in self.triangles.iter().enumerate() {
            if let Some(valences) = &mut valences {
                for v in [a, b, c] {
                    valences[v] += 1;
                }
            }
            if !nearby_faces[i] || !self.in_patch(i, [a, b, c], brush, dabs, stencils) {
                continue;
            }
            for (a, b, c) in [(a, b, c), (b, c, a), (c, a, b)] {
                local_vertices[a] = true;
                edges.push((edge(a, b), (i, a, b, c)));
            }
        }
        // Stable sorting retains the original face order within each edge,
        // matching the former ordered map without per-edge vector allocations.
        if FLAT {
            edges.sort_by_key(|(key, _)| *key);
        } else {
            let mut reference: BTreeMap<_, Vec<_>> = BTreeMap::new();
            for (key, record) in edges {
                reference.entry(key).or_default().push(record);
            }
            edges = reference
                .into_iter()
                .flat_map(|(key, records)| records.into_iter().map(move |record| (key, record)))
                .collect();
        }
        // Every queried replacement diagonal has two local endpoints. Scan all
        // faces so an edge outside the patch still prevents a duplicate, but do
        // not populate the hash table with unrelated distant edges.
        let existing = self.existing_patch_edges(&local_vertices);
        let quality = |t: [usize; 3]| {
            let [a, b, c] = t.map(|i| self.vertices[i]);
            let sum = (b - a).dot(b - a) + (c - b).dot(c - b) + (a - c).dot(a - c);
            2. * 3f32.sqrt() * (b - a).cross(c - a).len() / sum.max(1e-20)
        };
        let normal = |t: [usize; 3]| {
            let [a, b, c] = t.map(|i| self.vertices[i]);
            (b - a).cross(c - a).norm()
        };
        let mut used = HashSet::new();
        let mut replacements = Vec::new();
        for pair in edges.chunk_by(|a, b| a.0 == b.0) {
            let [(_, (i, a, b, c)), (_, (j, x, y, d))] = pair else {
                continue;
            };
            if !self.polygroups.is_empty() && self.polygroups[*i] != self.polygroups[*j] {
                continue;
            }
            if replacements.len() >= limit {
                break;
            }
            if a != y
                || b != x
                || c == d
                || [*a, *b, *c, *d].iter().any(|&v| protected[v])
                || used.contains(i)
                || used.contains(j)
                || existing.contains(&edge(*c, *d))
                || [*a, *b, *c, *d]
                    .iter()
                    .any(|&v| self.mask[v] > 0.95 || (self.mask[v] - self.mask[*a]).abs() > 0.05)
            {
                continue;
            }
            if !(Region { dabs, stencils })
                .permits_reconnection(brush, &[*a, *b, *c, *d].map(|v| self.vertices[v]))
            {
                continue;
            }
            let old = [self.triangles[*i], self.triangles[*j]];
            let new = [[*c, *d, *b], [*d, *c, *a]];
            let n = normal(old[0]);
            let old_quality = quality(old[0]).min(quality(old[1]));
            let new_quality = quality(new[0]).min(quality(new[1]));
            let shape_improves = new_quality > old_quality + 0.03;
            let quad = [*a, *b, *c, *d];
            let valence_improves = optimize_valence
                && valences
                    .as_ref()
                    .is_some_and(|valences| valence_flip_improves(valences, quad));
            let objective_improves = if optimize_valence {
                valence_improves
            } else {
                shape_improves
            };
            if n.dot(normal(old[1])) < 0.85
                || new.iter().any(|&t| normal(t).dot(n) < 0.85)
                || (optimize_valence && (new_quality < old_quality * 0.8 || new_quality < 0.02))
                || !objective_improves
            {
                continue;
            }
            // Block the whole quadrilateral so simultaneous flips cannot
            // introduce a duplicate diagonal or conflicting connectivity.
            if [*a, *b, *c, *d]
                .iter()
                .any(|v| used.contains(&(self.triangles.len() + v)))
            {
                continue;
            }
            used.extend([*i, *j]);
            used.extend([*a, *b, *c, *d].map(|v| self.triangles.len() + v));
            replacements.push((*i, *j, new));
        }
        let count = replacements.len();
        for (i, j, [a, b]) in replacements {
            self.triangles[i] = a;
            self.triangles[j] = b;
        }
        if count > 0 {
            self.retain_adaptive_quads();
        }
        count
    }
    fn existing_patch_edges(&self, local_vertices: &[bool]) -> HashSet<(usize, usize)> {
        let mut existing = HashSet::new();
        for &[a, b, c] in &self.triangles {
            for (a, b) in [(a, b), (b, c), (c, a)] {
                if local_vertices[a] && local_vertices[b] {
                    existing.insert(edge(a, b));
                }
            }
        }
        existing
    }
    fn split_local(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        target: f32,
        limit: usize,
        unmasked_only: bool,
    ) -> usize {
        self.split_local_impl::<true>(
            brush,
            dabs,
            stencils,
            target,
            limit,
            SplitOptions {
                relief: false,
                unmasked_only,
            },
        )
    }
    fn split_local_impl<const LOCAL_MIDPOINTS: bool>(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        target: f32,
        limit: usize,
        options: SplitOptions,
    ) -> usize {
        if limit == 0 {
            return 0;
        }
        let relief = options.relief
            && brush.kind == Kind::Standard
            && (matches!(brush.alpha, Alpha::Image { .. }) || !stencils.is_empty());
        let mut sampling =
            relief.then(|| relief::Sampling::new(brush, Region { dabs, stencils }, target));
        let mut candidates = BTreeMap::new();
        let nearby_faces = self.patch_candidates(brush, dabs);
        for (fi, &t) in self.triangles.iter().enumerate() {
            if !nearby_faces[fi] || !self.in_patch(fi, t, brush, dabs, stencils) {
                continue;
            }
            let [a, b, c] = t;
            for (x, y) in [(a, b), (b, c), (c, a)] {
                let length =
                    (self.vertices[x] - self.vertices[y]).dot(self.vertices[x] - self.vertices[y]);
                // Spend extra resolution where a placed alpha changes sharply.
                // The midpoint also catches a narrow feature between two endpoints.
                let stamp = matches!(
                    brush.effective_stroke_mode(),
                    super::StrokeMode::DragRect | super::StrokeMode::DragDot
                );
                let boundary = (stamp || !stencils.is_empty())
                    && dabs.iter().enumerate().any(|(index, dab)| {
                        let weights = [
                            self.vertices[x],
                            self.vertices[y],
                            (self.vertices[x] + self.vertices[y]) * 0.5,
                        ]
                        .map(|p| {
                            let d = p - dab.center;
                            brush.weight(d.dot(dab.right) / dab.radius, d.dot(dab.up) / dab.radius)
                                * stencils.get(index).map_or(1., |s| s.weight(p))
                        });
                        let low = weights.into_iter().fold(f32::INFINITY, f32::min);
                        let high = weights.into_iter().fold(0_f32, f32::max);
                        high - low > 0.15
                    });
                let mut edge_target = if boundary { target * 0.5 } else { target };
                if brush.kind == Kind::Detail {
                    for dab in dabs {
                        let a = self.vertices[x] - dab.center;
                        let edge = self.vertices[y] - self.vertices[x];
                        let t = (-a.dot(edge) / edge.dot(edge).max(1e-20)).clamp(0., 1.);
                        // Resolve the narrow incision and shoulders before they
                        // move. Prioritize these edges over the broad outer falloff.
                        if (a + edge * t).len()
                            < dab.radius * super::detail::FEATURE_RADIUS_FRACTION
                        {
                            edge_target =
                                edge_target.min(dab.radius * super::detail::CORE_EDGE_FRACTION);
                        }
                    }
                }
                let ratio = length / (edge_target * edge_target);
                if ratio > 1. {
                    candidates
                        .entry(edge(x, y))
                        .and_modify(|old: &mut f32| *old = old.max(ratio))
                        .or_insert(ratio);
                }
            }
            if let Some((key, ratio)) = sampling
                .as_mut()
                .and_then(|sampling| sampling.candidate(self, t))
            {
                candidates
                    .entry(key)
                    .and_modify(|old| *old = old.max(ratio))
                    .or_insert(ratio);
            }
        }
        if options.unmasked_only || self.has_hidden_faces() {
            let protected = self.adaptive_protected_vertices(options.unmasked_only);
            candidates.retain(|&(a, b), _| !protected[a] && !protected[b]);
        }
        let mut candidates: Vec<_> = candidates.into_iter().collect();
        // Each manifold edge adds one vertex and two faces. Leave headroom for
        // malformed/nonmanifold inputs as well; adaptive mode validates closure.
        let capacity = limit
            .min(max_vertices().saturating_sub(self.vertices.len()))
            .min(max_triangles().saturating_sub(self.triangles.len()) / 2);
        retain_split_candidates(&mut candidates, capacity);
        if candidates.is_empty() {
            return 0;
        }
        candidates.sort_by_key(|(edge, _)| *edge);
        let mut mids = HashMap::new();
        let mut split_vertices = vec![false; self.vertices.len()];
        for &((a, b), _) in &candidates {
            split_vertices[a] = true;
            split_vertices[b] = true;
            let index = self.vertices.len();
            self.vertices
                .push((self.vertices[a] + self.vertices[b]) * 0.5);
            self.mask.push((self.mask[a] + self.mask[b]) * 0.5);
            if !self.materials.is_empty() {
                self.materials.push(super::Material::blend([
                    (self.material(a), 0.5),
                    (self.material(b), 0.5),
                ]));
            }
            for g in &mut self.vertex_groups {
                g.weights.push((g.weights[a] + g.weights[b]) * 0.5);
            }
            mids.insert((a, b), index);
        }
        let mut faces = Vec::with_capacity(self.triangles.len() + candidates.len() * 2);
        let mut new_groups = Vec::new();
        let mut new_hidden = Vec::new();
        for (fi, &[a, b, c]) in self.triangles.iter().enumerate() {
            let start = faces.len();
            let m = [(a, b), (b, c), (c, a)].map(|(a, b)| {
                if !LOCAL_MIDPOINTS || (split_vertices[a] && split_vertices[b]) {
                    mids.get(&edge(a, b)).copied()
                } else {
                    None
                }
            });
            match m.iter().flatten().count() {
                0 => faces.push([a, b, c]),
                1 => {
                    for i in 0..3 {
                        if let Some(mid) = m[i] {
                            let v = [a, b, c];
                            let (x, y, z) = (v[i], v[(i + 1) % 3], v[(i + 2) % 3]);
                            faces.extend([[x, mid, z], [mid, y, z]]);
                        }
                    }
                }
                2 => {
                    let i = (0..3)
                        .find(|&i| m[i].is_some() && m[(i + 1) % 3].is_some())
                        .unwrap();
                    let v = [a, b, c];
                    let (x, y, z) = (v[i], v[(i + 1) % 3], v[(i + 2) % 3]);
                    let p = m[i].unwrap();
                    let q = m[(i + 1) % 3].unwrap();
                    faces.push([y, q, p]);
                    if (self.vertices[x] - self.vertices[q]).len()
                        < (self.vertices[p] - self.vertices[z]).len()
                    {
                        faces.extend([[x, p, q], [x, q, z]]);
                    } else {
                        faces.extend([[x, p, z], [p, q, z]]);
                    }
                }
                _ => {
                    let [p, q, r] = m.map(Option::unwrap);
                    faces.extend([[a, p, r], [p, b, q], [r, q, c], [p, q, r]]);
                }
            }
            if !self.hidden_faces.is_empty() {
                new_hidden.resize(faces.len(), self.hidden_faces[fi]);
            }
            if !self.polygroups.is_empty() {
                new_groups.resize(faces.len(), self.polygroups[fi]);
                debug_assert!(faces.len() > start);
            }
        }
        self.polygroups = new_groups;
        self.hidden_faces = new_hidden;
        self.triangles = faces;
        self.retain_adaptive_quads();
        candidates.len()
    }
    fn collapse_local(
        &mut self,
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
        options: CollapseOptions,
    ) -> usize {
        let CollapseOptions {
            threshold,
            limit,
            unmasked_only,
            preserve_surface,
        } = options;
        let protected = self.adaptive_protected_vertices(unmasked_only);
        let mut candidates = BTreeMap::new();
        let nearby_faces = self.patch_candidates(brush, dabs);
        for (fi, &t) in self.triangles.iter().enumerate() {
            let [a, b, c] = t;
            if nearby_faces[fi] && self.in_patch(fi, t, brush, dabs, stencils) {
                for (a, b) in [(a, b), (b, c), (c, a)] {
                    let length = (self.vertices[a] - self.vertices[b]).len();
                    if length < threshold {
                        candidates.insert(edge(a, b), length);
                    }
                }
            }
        }
        if candidates.is_empty() {
            return 0;
        }
        let mut relevant = vec![false; self.vertices.len()];
        for &(a, b) in candidates.keys() {
            relevant[a] = true;
            relevant[b] = true;
        }
        if preserve_surface {
            let endpoints = relevant.clone();
            for t in &self.triangles {
                if t.iter().any(|&i| endpoints[i]) {
                    for &i in t {
                        relevant[i] = true;
                    }
                }
            }
        }
        let (neighbors, incident) = self.collapse_adjacency(&relevant);
        let mut candidates: Vec<_> = candidates.into_iter().collect();
        candidates.sort_by(|(a, x), (b, y)| x.total_cmp(y).then(a.cmp(b)));
        let mut blocked = HashSet::new();
        let mut merged = HashMap::new();
        for ((a, b), _) in candidates {
            if merged.len() >= limit {
                break;
            }
            if brush.kind == Kind::Snake
                && dabs.iter().any(|dab| {
                    (self.vertices[a] - dab.center).len() < dab.radius * 0.35
                        || (self.vertices[b] - dab.center).len() < dab.radius * 0.35
                })
            {
                continue;
            }
            if protected[a]
                || protected[b]
                || blocked.contains(&a)
                || blocked.contains(&b)
                || self.mask[a] > 0.95
                || self.mask[b] > 0.95
                || (self.mask[a] - self.mask[b]).abs() > 0.05
            {
                continue;
            }
            if brush.kind == Kind::Smooth {
                // Smooth uses edge size to decide coarsening, not the amount
                // of positional motion. Require both endpoints to be inside
                // the brush's influenced area, then retain the surface-error
                // and fold guards below.
                let influenced = dabs.iter().any(|dab| {
                    let pressure = if brush.pressure {
                        dab.pressure.powf(brush.pressure_curve)
                    } else {
                        1.
                    };
                    [a, b].into_iter().all(|i| {
                        let offset = self.vertices[i] - dab.center;
                        offset.len() <= dab.radius
                            && brush.weight(
                                offset.dot(dab.right) / dab.radius,
                                offset.dot(dab.up) / dab.radius,
                            ) * pressure
                                * (1. - self.mask[i])
                                > 0.001
                    })
                });
                if !influenced || (self.vertices[a] - self.vertices[b]).len() >= threshold {
                    continue;
                }
            } else if brush.kind == Kind::Decimate {
                // Both endpoints must be inside the same brush copy. A tiny
                // falloff/pressure must not remove full-size edges at the rim.
                let influence = dabs
                    .iter()
                    .map(|dab| {
                        let pressure = if brush.pressure {
                            dab.pressure.powf(brush.pressure_curve)
                        } else {
                            1.
                        };
                        [a, b]
                            .map(|i| {
                                let offset = self.vertices[i] - dab.center;
                                if offset.len() > dab.radius {
                                    return 0.;
                                }
                                brush.weight(
                                    offset.dot(dab.right) / dab.radius,
                                    offset.dot(dab.up) / dab.radius,
                                ) * pressure
                                    * brush.strength
                                    * (1. - self.mask[i])
                            })
                            .into_iter()
                            .fold(f32::INFINITY, f32::min)
                    })
                    .fold(0f32, f32::max);
                if (self.vertices[a] - self.vertices[b]).len()
                    >= threshold * (influence * 4.).clamp(0., 1.).sqrt()
                {
                    continue;
                }
            }
            if incident[a]
                .iter()
                .chain(&incident[b])
                .any(|&i| !self.face_visible(i))
            {
                continue;
            }
            if !stencils.is_empty() {
                let points: Vec<_> = incident[a]
                    .iter()
                    .chain(&incident[b])
                    .flat_map(|&i| self.triangles[i].map(|v| self.vertices[v]))
                    .collect();
                if !(Region { dabs, stencils }).permits_reconnection(brush, &points) {
                    continue;
                }
            }
            if !self.polygroups.is_empty() {
                let a_groups: std::collections::HashSet<_> =
                    incident[a].iter().map(|&i| self.polygroups[i]).collect();
                let b_groups: std::collections::HashSet<_> =
                    incident[b].iter().map(|&i| self.polygroups[i]).collect();
                if a_groups != b_groups {
                    continue;
                }
            }
            let minimum_quality = if brush.kind == Kind::Smooth {
                0.02
            } else {
                0.2
            };
            if !self.safe_edge_collapse(
                [a, b],
                &neighbors,
                &incident,
                threshold,
                preserve_surface,
                minimum_quality,
            ) {
                continue;
            }
            blocked.extend(neighbors[a].iter().copied());
            blocked.extend(neighbors[b].iter().copied());
            if preserve_surface {
                // Adjacent changed faces must not pass the fold guard against
                // each other's old positions and then fold in combination.
                for &i in neighbors[a].iter().chain(&neighbors[b]) {
                    blocked.extend(neighbors[i].iter().copied());
                }
            }
            blocked.extend([a, b]);
            merged.insert(b, a);
        }
        if merged.is_empty() {
            return 0;
        }
        self.apply_edge_collapses(&merged);
        merged.len()
    }
    /// Callers choose disjoint one-rings and protect boundary and crease endpoints.
    pub(super) fn apply_edge_collapses(&mut self, merged: &HashMap<usize, usize>) {
        for (&b, &a) in merged {
            self.vertices[a] = (self.vertices[a] + self.vertices[b]) * 0.5;
            self.mask[a] = (self.mask[a] + self.mask[b]) * 0.5;
            if !self.materials.is_empty() {
                self.materials[a] =
                    super::Material::blend([(self.material(a), 0.5), (self.material(b), 0.5)]);
            }
            for g in &mut self.vertex_groups {
                g.weights[a] = (g.weights[a] + g.weights[b]) * 0.5;
            }
        }
        self.compact_vertex_merges(merged);
    }
    fn compact_vertex_merges(&mut self, merged: &HashMap<usize, usize>) {
        let mut indices = vec![0; self.vertices.len()];
        let mut vertices = Vec::with_capacity(self.vertices.len() - merged.len());
        let mut mask = Vec::with_capacity(vertices.capacity());
        for (i, &v) in self.vertices.iter().enumerate() {
            if !merged.contains_key(&i) {
                indices[i] = vertices.len();
                vertices.push(v);
                mask.push(self.mask[i]);
            }
        }
        for (&b, &a) in merged {
            indices[b] = indices[a];
        }
        let old_faces: Vec<_> = self
            .triangles
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                let [a, b, c] = t.map(|i| indices[i]);
                a != b && b != c && a != c
            })
            .map(|(i, _)| i)
            .collect();
        let triangles: Vec<[usize; 3]> = old_faces
            .iter()
            .map(|&i| self.triangles[i].map(|v| indices[v]))
            .collect();
        let uv_current = self.uv.current(self);
        if uv_current {
            self.uv.corners = old_faces.iter().map(|&i| self.uv.corners[i]).collect();
            if !self.uv.masks.is_empty() {
                self.uv.masks = old_faces.iter().map(|&i| self.uv.masks[i]).collect();
            }
            if !self.uv.removed.is_empty() {
                self.uv.removed = old_faces.iter().map(|&i| self.uv.removed[i]).collect();
            }
            self.uv.topology = triangles.clone();
            self.uv.seams = self
                .uv
                .seams
                .iter()
                .map(|&e| e.map(|v| indices[v]))
                .filter(|e| e[0] != e[1])
                .map(|e| super::uv::edge(e[0], e[1]))
                .collect();
            self.uv.seams.sort_unstable();
            self.uv.seams.dedup();
        }
        if !self.polygroups.is_empty() {
            self.polygroups = old_faces.iter().map(|&i| self.polygroups[i]).collect();
        }
        if !self.hidden_faces.is_empty() {
            self.hidden_faces = old_faces.iter().map(|&i| self.hidden_faces[i]).collect();
        }
        for g in &mut self.vertex_groups {
            g.weights = g
                .weights
                .iter()
                .enumerate()
                .filter(|(i, _)| !merged.contains_key(i))
                .map(|(_, w)| *w)
                .collect();
        }
        self.materials =
            self.reindex_materials((0..self.vertices.len()).filter(|i| !merged.contains_key(i)));
        self.vertices = vertices;
        self.mask = mask;
        for q in &mut self.quads {
            *q = q.map(|v| indices[v]);
        }
        for crease in &mut self.creases {
            *crease = crease.map(|v| indices[v]);
        }
        for v in &mut self.loose_vertices {
            *v = indices[*v];
        }
        for e in &mut self.loose_edges {
            *e = e.map(|v| indices[v]);
            e.sort_unstable();
        }
        self.loose_edges.sort_unstable();
        self.triangles = triangles;
        self.shading_seams = None;
        self.retain_adaptive_quads();
    }
    /// Contract a short closed edge loop as one event. The subsequent vertex
    /// fan split turns the resulting pinch into separate closed components.
    fn contract_short_cycle(&mut self, cycle: &[usize]) -> Option<(Vec3, usize)> {
        if cycle.len() < 4 {
            return None;
        }
        let mut vertices = cycle.to_vec();
        vertices.sort_unstable();
        vertices.dedup();
        if vertices.len() != cycle.len() {
            return None;
        }
        let representative = vertices[0];
        let count = vertices.len() as f32;
        let position = vertices
            .iter()
            .map(|&v| self.vertices[v])
            .fold(Vec3::default(), |sum, p| sum + p)
            * (1. / count);
        let mask = vertices.iter().map(|&v| self.mask[v]).sum::<f32>() / count;
        self.vertices[representative] = position;
        self.mask[representative] = mask;
        if !self.materials.is_empty() {
            self.materials[representative] =
                super::Material::blend(vertices.iter().map(|&v| (self.material(v), 1. / count)));
        }
        for group in &mut self.vertex_groups {
            group.weights[representative] =
                vertices.iter().map(|&v| group.weights[v]).sum::<f32>() / count;
        }
        let merged: HashMap<_, _> = vertices
            .iter()
            .copied()
            .filter(|&v| v != representative)
            .map(|v| (v, representative))
            .collect();
        self.compact_vertex_merges(&merged);
        Some((position, representative))
    }
    pub(super) fn safe_edge_collapse(
        &self,
        [a, b]: [usize; 2],
        neighbors: &[Vec<usize>],
        incident: &[Vec<usize>],
        threshold: f32,
        preserve_surface: bool,
        minimum_quality: f32,
    ) -> bool {
        let shared: Vec<_> = incident[a]
            .iter()
            .copied()
            .filter(|&i| self.triangles[i].contains(&b))
            .collect();
        if shared.len() != 2 {
            return false;
        }
        let opposite: Vec<_> = shared
            .iter()
            .filter_map(|&i| {
                self.triangles[i]
                    .iter()
                    .copied()
                    .find(|&v| v != a && v != b)
            })
            .collect();
        let common: Vec<_> = neighbors[a]
            .iter()
            .copied()
            .filter(|n| neighbors[b].binary_search(n).is_ok())
            .collect();
        if common.len() != 2
            || opposite.len() != 2
            || opposite[0] == opposite[1]
            || !common.iter().all(|v| opposite.contains(v))
        {
            return false;
        }
        let normal = |t: [usize; 3]| {
            let [x, y, z] = t.map(|i| self.vertices[i]);
            (y - x).cross(z - x).norm()
        };
        if normal(self.triangles[shared[0]]).dot(normal(self.triangles[shared[1]])) < 0.85 {
            return false;
        }
        let position = (self.vertices[a] + self.vertices[b]) * 0.5;
        let mut local_keys = HashSet::new();
        let mut affected = incident[a].clone();
        affected.extend(&incident[b]);
        affected.sort_unstable();
        affected.dedup();
        for index in affected {
            let old = self.triangles[index];
            if preserve_surface
                && (position - self.vertices[old[0]]).dot(normal(old)).abs() > threshold * 0.08
            {
                // Local plane-error guard: short edges on a ridge can still
                // carry significant shape. This is not a Hausdorff bound.
                return false;
            }
            if old.contains(&a) && old.contains(&b) {
                continue;
            }
            let new = old.map(|i| if i == b { a } else { i });
            let mut key = new;
            key.sort_unstable();
            if !local_keys.insert(key) {
                return false;
            }
            let [x, y, z] = new.map(|i| if i == a { position } else { self.vertices[i] });
            let cross = (y - x).cross(z - x);
            if preserve_surface {
                let quality = |x: Vec3, y: Vec3, z: Vec3| {
                    let sum = (y - x).dot(y - x) + (z - y).dot(z - y) + (x - z).dot(x - z);
                    2. * 3f32.sqrt() * (y - x).cross(z - x).len() / sum.max(1e-20)
                };
                let [u, v, w] = old.map(|i| self.vertices[i]);
                let minimum = (quality(u, v, w) * 0.8).clamp(minimum_quality, 0.35);
                if quality(x, y, z) < minimum {
                    return false;
                }
            }
            if cross.len() < threshold * threshold * 1e-5 || cross.norm().dot(normal(old)) < 0.8 {
                return false;
            }
        }
        !preserve_surface || self.collapse_preserves_folds(a, b, position, neighbors, incident)
    }
    fn collapse_preserves_folds(
        &self,
        a: usize,
        b: usize,
        position: Vec3,
        neighbors: &[Vec<usize>],
        incident: &[Vec<usize>],
    ) -> bool {
        let mut faces: Vec<_> = neighbors[a]
            .iter()
            .chain(&neighbors[b])
            .chain([&a, &b])
            .flat_map(|&i| incident[i].iter().copied())
            .collect();
        faces.sort_unstable();
        faces.dedup();
        let mut edges = HashMap::new();
        for face in faces {
            let old = self.triangles[face];
            if old.contains(&a) && old.contains(&b) {
                continue;
            }
            let new = old.map(|i| if i == b { a } else { i });
            let [x, y, z] = old.map(|i| self.vertices[i]);
            let old_normal = (y - x).cross(z - x).norm();
            let [x, y, z] = new.map(|i| if i == a { position } else { self.vertices[i] });
            let normal = (y - x).cross(z - x).norm();
            for (u, v) in [(new[0], new[1]), (new[1], new[2]), (new[2], new[0])] {
                if let Some((other, before, after)) =
                    edges.insert(edge(u, v), (old, old_normal, normal))
                {
                    let were_neighbors = old.iter().filter(|i| other.contains(i)).count() >= 2;
                    let minimum = if were_neighbors {
                        old_normal.dot(before).min(0.)
                    } else {
                        0.
                    };
                    if normal.dot(after) < minimum - 1e-6 {
                        return false;
                    }
                }
            }
        }
        true
    }
    pub(super) fn collapse_adjacency(
        &self,
        relevant: &[bool],
    ) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
        let mut neighbors = vec![Vec::new(); self.vertices.len()];
        let mut incident = vec![Vec::new(); self.vertices.len()];
        for (index, &[a, b, c]) in self.triangles.iter().enumerate() {
            for (i, j, k) in [(a, b, c), (b, c, a), (c, a, b)] {
                if relevant[i] {
                    neighbors[i].extend([j, k]);
                    incident[i].push(index);
                }
            }
        }
        for (i, ns) in neighbors.iter_mut().enumerate() {
            if relevant[i] {
                ns.sort_unstable();
                ns.dedup();
            }
        }
        (neighbors, incident)
    }
    /// Classify each shared vertex once before scanning faces. An outcode's
    /// common bit means all three corners lie beyond the same brush-box plane.
    /// This only rejects distant faces; the exact footprint/normal/visibility
    /// checks still decide membership. Rebuild after every topology/position edit.
    fn patch_candidates(&self, brush: &Brush, dabs: &[Dab]) -> Vec<bool> {
        let reach = if matches!(
            brush.effective_stroke_mode(),
            super::StrokeMode::DragRect | super::StrokeMode::DragDot
        ) {
            1.5
        } else {
            1.
        };
        let magnitude = self
            .vertices
            .iter()
            .fold(0_f32, |m, p| m.max(p.0.abs()).max(p.1.abs()).max(p.2.abs()));
        let mut nearby = vec![false; self.triangles.len()];
        for dab in dabs {
            if brush.pressure && dab.pressure <= 0. {
                continue;
            }
            let magnitude = magnitude
                .max(dab.center.0.abs())
                .max(dab.center.1.abs())
                .max(dab.center.2.abs());
            let radius = dab.radius * reach + 16. * f32::EPSILON * (magnitude + dab.radius + 1.);
            let low = dab.center - Vec3(radius, radius, radius);
            let high = dab.center + Vec3(radius, radius, radius);
            let codes: Vec<u8> = self
                .vertices
                .iter()
                .map(|p| {
                    u8::from(p.0 < low.0)
                        | (u8::from(p.0 > high.0) << 1)
                        | (u8::from(p.1 < low.1) << 2)
                        | (u8::from(p.1 > high.1) << 3)
                        | (u8::from(p.2 < low.2) << 4)
                        | (u8::from(p.2 > high.2) << 5)
                })
                .collect();
            for (near, &[a, b, c]) in nearby.iter_mut().zip(&self.triangles) {
                *near |= codes[a] & codes[b] & codes[c] == 0;
            }
        }
        nearby
    }
    fn in_patch(
        &self,
        face: usize,
        t: [usize; 3],
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
    ) -> bool {
        self.in_patch_impl::<true>(face, t, brush, dabs, stencils)
    }
    fn in_patch_impl<const BROAD_PHASE: bool>(
        &self,
        face: usize,
        t: [usize; 3],
        brush: &Brush,
        dabs: &[Dab],
        stencils: &[Projection],
    ) -> bool {
        if !self.face_visible(face) || t.iter().all(|&i| self.mask[i] >= 0.999) {
            return false;
        }
        let [a, b, c] = t.map(|i| self.vertices[i]);
        let bounds = [
            (a.0.min(b.0).min(c.0), a.0.max(b.0).max(c.0)),
            (a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1)),
            (a.2.min(b.2).min(c.2), a.2.max(b.2).max(c.2)),
        ];
        // Drag stamps use the same depth reach as the displacement sampler;
        // on curved surfaces the projected alpha edge can lie beyond radius.
        let reach = if matches!(
            brush.effective_stroke_mode(),
            super::StrokeMode::DragRect | super::StrokeMode::DragDot
        ) {
            1.5
        } else {
            1.
        };
        let mut normal = None;
        dabs.iter().enumerate().any(|(index, dab)| {
            if brush.pressure && dab.pressure <= 0. {
                return false;
            }
            if BROAD_PHASE
                && bounds
                    .iter()
                    .zip([dab.center.0, dab.center.1, dab.center.2])
                    .any(|(&(low, high), center)| {
                        // Expand conservatively for the arithmetic in the exact
                        // closest-point test, including large translated models.
                        let padding = 16.
                            * f32::EPSILON
                            * (low.abs().max(high.abs()).max(center.abs()) + dab.radius + 1.);
                        let radius = dab.radius * reach + padding;
                        center < low - radius || center > high + radius
                    })
            {
                return false;
            }
            if brush.kind != Kind::Snake
                && normal
                    .get_or_insert_with(|| (b - a).cross(c - a).norm())
                    .dot(dab.normal)
                    < 0.1
            {
                return false;
            }
            let p = closest_triangle(dab.center, a, b, c);
            let offset = p - dab.center;
            if offset.len() > dab.radius * reach {
                return false;
            }
            if stencils
                .get(index)
                .is_some_and(|s| s.coverage_bounds(&[a, b, c]).1 <= 0.001)
            {
                return false;
            }
            [p, a, b, c, (a + b + c) * (1. / 3.)].into_iter().any(|p| {
                let d = p - dab.center;
                brush.weight(d.dot(dab.right) / dab.radius, d.dot(dab.up) / dab.radius) > 0.001
            })
        })
    }
}
fn retain_split_candidates(candidates: &mut Vec<((usize, usize), f32)>, capacity: usize) {
    if capacity < candidates.len() {
        // Edge keys break every ratio tie, giving precisely the same retained
        // edges as a full sort, without sorting discarded candidates.
        candidates.select_nth_unstable_by(capacity, |(a, x), (b, y)| y.total_cmp(x).then(a.cmp(b)));
        candidates.truncate(capacity);
    }
}
fn edge(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}
fn valence_flip_improves(valences: &[usize], [a, b, c, d]: [usize; 4]) -> bool {
    let error = |valence: usize| (valence as isize - 6).pow(2) as usize;
    let before = error(valences[a]) + error(valences[b]) + error(valences[c]) + error(valences[d]);
    let after = error(valences[a].saturating_sub(1))
        + error(valences[b].saturating_sub(1))
        + error(valences[c] + 1)
        + error(valences[d] + 1);
    after < before
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod tests {
    use super::*;
    #[test]
    fn reactor_relief_sampling_error() {
        let mut brush = Brush::new(Kind::Standard);
        brush.stroke_mode = super::super::StrokeMode::DragRect;
        brush.radius = 252.10057;
        brush.rotation = 55.801712;
        brush.alpha = super::super::alpha::builtins()
            .iter()
            .find(|a| a.name == "Reactor rings")
            .unwrap()
            .image
            .clone();
        let d = Dab {
            center: Vec3(-0.18910993, 0.22439388, 0.9556),
            normal: Vec3(-0.18930681, 0.22462495, 0.955_88),
            radius: 0.8365707,
            ..dab()
        };
        let settings = AdaptiveSettings {
            enabled: true,
            ..Default::default()
        };
        let mut mesh = Mesh::sphere();
        let original = mesh.clone();
        let mut budget = Budget::default();
        mesh.adapt(&brush, &[d], &settings, &mut budget, true);
        mesh.prepare_drag_stamp(&brush, &[d], &settings);
        assert!(mesh.vertices.len() <= original.vertices.len() + 256 + 16384);
        let surface = mesh.clone();
        assert!(
            mesh.stamp_many(&brush, &[d]),
            "refinement must not make the guard reject the stamp"
        );
        let mut errors = Vec::new();
        for (fi, &t) in surface.triangles.iter().enumerate() {
            if !surface.in_patch(fi, t, &brush, &[d], &[]) {
                continue;
            }
            let [a, b, c] = t.map(|i| surface.vertices[i]);
            let [x, y, z] = t.map(|i| mesh.vertices[i]);
            let lengths = [(b - a).len(), (c - b).len(), (a - c).len()];
            let quality = 2. * 3_f32.sqrt() * (b - a).cross(c - a).len()
                / lengths.map(|v| v * v).into_iter().sum::<f32>();
            assert!(quality > 0.05, "refinement generated a sliver: {quality}");
            // Independent seventh-spaced samples cover triangle interiors and
            // edges, not just the midpoint/quarters/centroid used for sizing.
            for i in 0..=7 {
                for j in 0..=7 - i {
                    let u = i as f32 / 7.;
                    let v = j as f32 / 7.;
                    let w = 1. - u - v;
                    let p = a * u + b * v + c * w;
                    let offset = p - d.center;
                    let height = brush
                        .weight(offset.dot(d.right) / d.radius, offset.dot(d.up) / d.radius)
                        * brush.strength
                        * d.radius
                        * 0.6;
                    let expected = p + d.normal * height;
                    let actual = x * u + y * v + z * w;
                    errors.push((expected - actual).len() * brush.radius / d.radius);
                }
            }
        }
        errors.sort_by(f32::total_cmp);
        let worst = *errors.last().unwrap();
        let p95 = errors[errors.len() * 95 / 100];
        eprintln!(
            "Reactor rings: {} vertices, sampled error pixels p95 {p95}, max {worst}",
            mesh.vertices.len()
        );
        assert!(worst < 1., "unresolved relief between vertices: {worst} px");
        assert!(p95 < 0.35, "roughness across the footprint: {p95} px");
        mesh.validate_closed().unwrap();
    }
    #[test]
    fn reactor_ring_stamp_preserves_steep_relief_on_adaptive_sphere() {
        use super::super::StrokeMode;
        // Reproduce the reported 298 px Reactor rings stamp on the seed sphere.
        // The inner ring is steep enough to turn past the old surface normal,
        // but remains a height field in the stamp direction.
        let mut mesh = Mesh::sphere();
        let mut brush = Brush::new(Kind::Standard);
        brush.stroke_mode = StrokeMode::DragRect;
        brush.radius = 298.28677;
        brush.rotation = 59.30068;
        brush.alpha = super::super::alpha::builtins()
            .iter()
            .find(|a| a.name == "Reactor rings")
            .unwrap()
            .image
            .clone();
        let d = Dab {
            center: Vec3(-0.0591254, 0.07994466, 0.9947),
            normal: Vec3(-0.05919629, 0.08001776, 0.99503415),
            radius: 0.98983807,
            ..dab()
        };
        let settings = AdaptiveSettings {
            enabled: true,
            ..Default::default()
        };
        let mut budget = Budget::default();
        mesh.adapt(&brush, &[d], &settings, &mut budget, true);
        mesh.prepare_drag_stamp(&brush, &[d], &settings);
        assert!(mesh.vertices.len() > 10_000);
        let normals = mesh.normals();
        for mode in [StrokeMode::DragRect, StrokeMode::DragDot] {
            brush.stroke_mode = mode;
            for invert in [false, true] {
                let d = Dab { invert, ..d };
                let mut stamped = mesh.clone();
                assert!(stamped.stamp_many(&brush, &[d]));
                let mut steep_faces = 0;
                for &t in &mesh.triangles {
                    let [a, b, c] = t.map(|i| mesh.vertices[i]);
                    let [x, y, z] = t.map(|i| stamped.vertices[i]);
                    let old = (b - a).cross(c - a);
                    let new = (y - x).cross(z - x);
                    steep_faces += usize::from(old.dot(new) < 0.);
                    if old.dot(d.normal) > old.len() * 0.1 {
                        assert!(new.dot(d.normal) >= old.dot(d.normal) * 0.999);
                    }
                }
                assert!(steep_faces > 100, "exercise the old false fold detection");
                for (i, &p) in mesh.vertices.iter().enumerate() {
                    let offset = p - d.center;
                    let weight = if offset.len() <= d.radius * 1.5
                        && normals[i].dot(d.normal) >= 0.1
                    {
                        brush.weight(offset.dot(d.right) / d.radius, offset.dot(d.up) / d.radius)
                    } else {
                        0.
                    };
                    let sign = if invert { -1. } else { 1. };
                    let expected = p + d.normal * (d.radius * 0.6 * brush.strength * weight * sign);
                    assert!(
                        (stamped.vertices[i] - expected).len() < 1e-5,
                        "{mode:?}, invert {invert}: vertex {i} was clipped unevenly"
                    );
                }
                stamped.validate_closed().unwrap();
            }
        }
    }
    fn dab() -> Dab {
        Dab {
            center: Vec3(0., 0., 1.),
            normal: Vec3(0., 0., 1.),
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 1., 0.),
            radius: 0.35,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        }
    }
    #[test]
    fn adaptive_patch_broad_phase_matches_precise_membership() {
        use crate::sculpt::{StrokeMode, Symmetry, alpha::Frame};
        let mut primitive =
            crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Icosphere);
        primitive.subdivisions = 1;
        let mut mesh = primitive.mesh().unwrap();
        let stencil = Projection {
            image: Alpha::Image {
                width: 2,
                height: 1,
                pixels: vec![0., 1.].into(),
            },
            frame: Frame {
                origin: Vec3::default(),
                right: Vec3(1., 0., 0.),
                up: Vec3(0., 1., 0.),
            },
            view: Vec3(0., 0., 1.),
            distance: 0.,
            position: [0., 0.],
            size: [1., 1.],
            rotation: 20.,
        };
        let symmetry = Symmetry {
            enabled: true,
            mirror: [true, true, false],
            ..Default::default()
        };
        let stencils = stencil.copies(&symmetry);
        let mut accepted = 0;
        let mut rejected = 0;
        for protection in 0..3 {
            mesh.hidden_faces = vec![false; mesh.triangles.len()];
            mesh.mask.fill(0.);
            if protection == 1 {
                mesh.hidden_faces
                    .iter_mut()
                    .step_by(3)
                    .for_each(|v| *v = true);
            }
            if protection == 2 {
                mesh.mask.iter_mut().step_by(2).for_each(|v| *v = 1.);
            }
            for kind in [Kind::Detail, Kind::Standard, Kind::Snake] {
                for stroke_mode in [
                    StrokeMode::Freehand,
                    StrokeMode::DragDot,
                    StrokeMode::DragRect,
                ] {
                    let brush = Brush {
                        stroke_mode,
                        ..Brush::new(kind)
                    };
                    for center in [Vec3(0.2, 0.1, 1.), Vec3(2., -0.2, 0.), Vec3(0., 0., 0.)] {
                        for radius in [0.05, 0.35, 2.] {
                            let dabs = symmetry.dabs(Dab {
                                center,
                                radius,
                                ..dab()
                            });
                            let candidates = mesh.patch_candidates(&brush, &dabs);
                            for projected in [&[][..], stencils.as_slice()] {
                                for (face, &t) in mesh.triangles.iter().enumerate() {
                                    let precise = mesh
                                        .in_patch_impl::<false>(face, t, &brush, &dabs, projected);
                                    assert_eq!(
                                        candidates[face]
                                            && mesh.in_patch(face, t, &brush, &dabs, projected),
                                        precise
                                    );
                                    accepted += usize::from(precise);
                                    rejected += usize::from(!precise);
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(accepted > 0 && rejected > 0);
    }

    #[test]
    fn adaptive_patch_broad_phase_preserves_reach_boundaries_and_translations() {
        use crate::sculpt::StrokeMode;
        for shift in [Vec3::default(), Vec3(10000., -10000., 10000.)] {
            let mesh = Mesh {
                vertices: vec![
                    Vec3(-1., -1., 0.) + shift,
                    Vec3(1., -1., 0.) + shift,
                    Vec3(0., 1., 0.) + shift,
                ],
                triangles: vec![[0, 1, 2]],
                mask: vec![0.; 3],
                ..Default::default()
            };
            for stroke_mode in [StrokeMode::Freehand, StrokeMode::DragDot] {
                let brush = Brush {
                    alpha: Alpha::Hard,
                    stroke_mode,
                    ..Brush::new(Kind::Standard)
                };
                let reach = if stroke_mode == StrokeMode::Freehand {
                    1.
                } else {
                    1.5
                };
                for delta in [-0.001, -f32::EPSILON, 0., f32::EPSILON, 0.001] {
                    for pressure in [0., 1.] {
                        let d = Dab {
                            center: Vec3(0., 0., 0.35 * reach + delta) + shift,
                            pressure,
                            ..dab()
                        };
                        assert_eq!(
                            mesh.patch_candidates(&brush, &[d])[0]
                                && mesh.in_patch(0, [0, 1, 2], &brush, &[d], &[]),
                            mesh.in_patch_impl::<false>(0, [0, 1, 2], &brush, &[d], &[])
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn partial_split_selection_matches_full_sort_including_ratio_ties() {
        let candidates: Vec<_> = (0..257)
            .rev()
            .map(|i| {
                (
                    (i, i + 1),
                    if i % 31 == 0 {
                        f32::INFINITY
                    } else {
                        (i % 7) as f32
                    },
                )
            })
            .collect();
        for capacity in [0, 1, 23, 64, 256, 257, 300] {
            let mut expected = candidates.clone();
            expected.sort_by(|(a, x), (b, y)| y.total_cmp(x).then(a.cmp(b)));
            expected.truncate(capacity);
            expected.sort_by_key(|(edge, _)| *edge);
            let mut actual = candidates.clone();
            retain_split_candidates(&mut actual, capacity);
            actual.sort_by_key(|(edge, _)| *edge);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn local_relaxation_safety_matches_full_face_passes() {
        let mesh = Mesh::sphere();
        for scale in [0., 0.01, 0.2, 2.] {
            let moves: Vec<_> = mesh
                .vertices
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    if p.2 < 0.3 {
                        Vec3::default()
                    } else {
                        Vec3((i % 3) as f32 - 1., 0.3, -1.) * scale
                    }
                })
                .collect();
            let mut expected = moves.clone();
            mesh.constrain_surface_moves::<false>(&mut expected);
            let mut actual = moves;
            mesh.constrain_surface_moves::<true>(&mut actual);
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn active_relaxation_neighborhood_matches_full_accumulation_exactly() {
        let mesh = Mesh::sphere();
        let normals = mesh.normals();
        let mut sums = vec![Vec3::default(); mesh.vertices.len()];
        let mut counts = vec![0.; mesh.vertices.len()];
        for &[a, b, c] in &mesh.triangles {
            for (i, j, k) in [(a, b, c), (b, c, a), (c, a, b)] {
                sums[i] = sums[i] + mesh.vertices[j] + mesh.vertices[k];
                counts[i] += 2.;
            }
        }
        for divisor in [1, 7, 23] {
            let active: Vec<_> = (0..mesh.vertices.len()).map(|i| i % divisor == 0).collect();
            let (local_normals, local_sums, local_counts) =
                mesh.relaxation_neighborhood(&active, false);
            for i in 0..active.len() {
                if active[i] {
                    assert_eq!(local_normals[i], normals[i]);
                    assert_eq!(local_sums[i], sums[i]);
                    assert_eq!(local_counts[i], counts[i]);
                } else {
                    assert_eq!(local_normals[i], Vec3::default());
                    assert_eq!(local_sums[i], Vec3::default());
                    assert_eq!(local_counts[i], 0.);
                }
            }
        }
    }

    #[test]
    fn valence_flip_objective_moves_toward_regular_triangle_valence() {
        assert!(valence_flip_improves(&[7, 7, 5, 5], [0, 1, 2, 3]));
        assert!(!valence_flip_improves(&[5, 5, 7, 7], [0, 1, 2, 3]));
        assert!(!valence_flip_improves(&[6, 6, 6, 6], [0, 1, 2, 3]));
    }

    #[test]
    fn smooth_tangential_rebalance_reduces_uneven_vertex_area() {
        let n = 24;
        let mut mesh = Mesh::default();
        for y in 0..=n {
            for x in 0..=n {
                let u = (x as f32 / n as f32).powf(1.6) * 2. - 1.;
                let v = y as f32 / n as f32 * 2. - 1.;
                mesh.vertices.push(Vec3(u, v, 1.));
                mesh.mask.push(0.);
            }
        }
        for y in 0..n {
            for x in 0..n {
                let a = y * (n + 1) + x;
                mesh.triangles
                    .extend([[a, a + 1, a + n + 2], [a, a + n + 2, a + n + 1]]);
            }
        }
        let dab = Dab {
            center: Vec3(0., 0., 1.),
            normal: Vec3(0., 0., 1.),
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 1., 0.),
            radius: 0.8,
            ..dab()
        };
        let brush = Brush::new(Kind::Smooth);
        let center = Vec3(0., 0., 1.);
        let local_areas = |mesh: &Mesh| {
            let mut areas = vec![0.; mesh.vertices.len()];
            for &[a, b, c] in &mesh.triangles {
                let area = (mesh.vertices[b] - mesh.vertices[a])
                    .cross(mesh.vertices[c] - mesh.vertices[a])
                    .len()
                    / 6.;
                for i in [a, b, c] {
                    areas[i] += area;
                }
            }
            areas
                .into_iter()
                .zip(&mesh.vertices)
                .filter(|(_, p)| (**p - center).len() < 0.3)
                .map(|(area, _)| area)
                .collect::<Vec<_>>()
        };
        let variance = |values: &[f32]| {
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32
        };
        let before = local_areas(&mesh);
        let original_positions = mesh.vertices.clone();
        assert!(mesh.relax_surface(&brush, &[dab], &[], 0.1, false));
        let after = local_areas(&mesh);
        assert!(
            variance(&after) < variance(&before),
            "area-weighted tangential redistribution should reduce clustering"
        );
        for (before, after) in original_positions.iter().zip(&mesh.vertices) {
            assert_eq!(
                before.2, after.2,
                "tangential relaxation must keep a plane flat"
            );
        }
    }

    #[test]
    fn candidate_collapse_adjacency_includes_every_incident_face() {
        let mesh = Mesh::sphere();
        let mut neighbors = vec![Vec::new(); mesh.vertices.len()];
        let mut incident = vec![Vec::new(); mesh.vertices.len()];
        for (face, &[a, b, c]) in mesh.triangles.iter().enumerate() {
            for (i, j, k) in [(a, b, c), (b, c, a), (c, a, b)] {
                neighbors[i].extend([j, k]);
                incident[i].push(face);
            }
        }
        for ns in &mut neighbors {
            ns.sort_unstable();
            ns.dedup();
        }
        let relevant: Vec<_> = mesh.vertices.iter().map(|p| p.2 > 0.7).collect();
        let (local_neighbors, local_incident) = mesh.collapse_adjacency(&relevant);
        for i in 0..relevant.len() {
            if relevant[i] {
                assert_eq!(local_neighbors[i], neighbors[i]);
                assert_eq!(local_incident[i], incident[i]);
            } else {
                assert!(local_neighbors[i].is_empty());
                assert!(local_incident[i].is_empty());
            }
        }
        assert!(
            local_neighbors.iter().flatten().any(|&i| !relevant[i]),
            "candidate neighborhoods must include off-patch neighbors"
        );
    }

    #[test]
    fn flat_flip_records_match_ordered_map_geometry() {
        let original = Mesh {
            vertices: vec![
                Vec3(0., 0., 1.),
                Vec3(0.6, 0., 1.),
                Vec3(0.6, 0.2, 1.),
                Vec3(0., 0.4, 1.),
            ],
            triangles: vec![[0, 1, 3], [1, 2, 3]],
            mask: vec![0.; 4],
            ..Default::default()
        };
        let brush = Brush::new(Kind::Detail);
        let d = Dab {
            radius: 2.,
            ..dab()
        };
        let mut actual = original.clone();
        let mut expected = original.clone();
        let count = actual.improve_edges_impl::<true>(&brush, &[d], &[], 64, false, false);
        assert_eq!(
            count,
            expected.improve_edges_impl::<false>(&brush, &[d], &[], 64, false, false)
        );
        assert_eq!(
            count, 1,
            "fixture must exercise a successful diagonal replacement"
        );
        assert_eq!(actual, expected);

        let mut original = Mesh::sphere();
        original.split_local(&brush, &[dab()], &[], 0.02, 64, false);
        for protected in [false, true] {
            let mut actual = original.clone();
            if protected {
                actual.mask.iter_mut().step_by(7).for_each(|m| *m = 1.);
            }
            let mut expected = actual.clone();
            for limit in [1, 16, 64] {
                assert_eq!(
                    actual.improve_edges_impl::<true>(&brush, &[dab()], &[], limit, false, false),
                    expected.improve_edges_impl::<false>(
                        &brush,
                        &[dab()],
                        &[],
                        limit,
                        false,
                        false
                    )
                );
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn split_endpoint_filter_preserves_exact_mesh_and_attributes() {
        let mut original = Mesh::sphere();
        original
            .create_vertex_group("Surface".into(), 0.5, true)
            .unwrap();
        original.mask.iter_mut().step_by(7).for_each(|m| *m = 0.4);
        let brush = Brush::new(Kind::Detail);
        let mut actual = original.clone();
        let mut expected = original;
        for limit in [1, 23, 64] {
            assert_eq!(
                actual.split_local_impl::<true>(
                    &brush,
                    &[dab()],
                    &[],
                    0.02,
                    limit,
                    SplitOptions::default()
                ),
                expected.split_local_impl::<false>(
                    &brush,
                    &[dab()],
                    &[],
                    0.02,
                    limit,
                    SplitOptions::default()
                )
            );
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn local_edge_membership_still_finds_a_diagonal_in_a_remote_face() {
        // The first two faces propose diagonal (2,3). Its existing face has a
        // remote third vertex and is not among the local oriented-face pairs.
        let mesh = Mesh {
            vertices: vec![Vec3::default(); 6],
            triangles: vec![[0, 1, 2], [1, 0, 3], [2, 3, 4], [3, 4, 5]],
            mask: vec![0.; 6],
            ..Default::default()
        };
        let local = [true, true, true, true, false, false];
        let filtered = mesh.existing_patch_edges(&local);
        let all: HashSet<_> = mesh
            .triangles
            .iter()
            .flat_map(|&[a, b, c]| [edge(a, b), edge(b, c), edge(c, a)])
            .collect();
        assert!(filtered.contains(&edge(2, 3)));
        for a in 0..4 {
            for b in a + 1..4 {
                assert_eq!(filtered.contains(&edge(a, b)), all.contains(&edge(a, b)));
            }
        }
        assert!(!filtered.contains(&edge(4, 5)));
    }

    #[test]
    fn drag_stamp_refines_alpha_boundary_before_displacement_and_preserves_protection() {
        for alpha in [
            Alpha::Hard,
            Alpha::Square,
            Alpha::Image {
                width: 2,
                height: 2,
                pixels: vec![0., 1., 0., 1.].into(),
            },
        ] {
            let mut mesh = Mesh::sphere();
            let original = mesh.clone();
            let mut brush = Brush::new(Kind::Standard);
            brush.stroke_mode = super::super::StrokeMode::DragRect;
            brush.radius = 180.;
            brush.alpha = alpha;
            let settings = AdaptiveSettings {
                enabled: true,
                detail: 8.,
                simplify: false,
                ..Default::default()
            };
            let d = Dab {
                radius: 0.7,
                ..dab()
            };
            assert!(mesh.prepare_drag_stamp(&brush, &[d], &settings));
            let added = mesh.vertices.len() - original.vertices.len();
            assert!(
                added > 256,
                "a large stamp needs more than one freehand frame: {added}"
            );
            assert!(added <= 8192);
            assert_eq!(
                &mesh.vertices[..original.vertices.len()],
                &original.vertices
            );
            mesh.validate_closed().unwrap();
            assert!(
                !mesh.prepare_drag_stamp(&brush, &[d], &settings),
                "this footprint should converge in one stamp budget"
            );
            let target = settings.edge_length(&brush, d);
            let mut boundary_edges = 0;
            for (fi, &t) in mesh.triangles.iter().enumerate() {
                if !mesh.in_patch(fi, t, &brush, &[d], &[]) {
                    continue;
                }
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    let weights = [
                        mesh.vertices[a],
                        mesh.vertices[b],
                        (mesh.vertices[a] + mesh.vertices[b]) * 0.5,
                    ]
                    .map(|p| {
                        let v = p - d.center;
                        brush.weight(v.dot(d.right) / d.radius, v.dot(d.up) / d.radius)
                    });
                    if weights.into_iter().fold(0_f32, f32::max)
                        - weights.into_iter().fold(f32::INFINITY, f32::min)
                        > 0.15
                    {
                        boundary_edges += 1;
                        assert!(
                            (mesh.vertices[a] - mesh.vertices[b]).len() <= target * 0.501,
                            "alpha boundary must receive finer edges before deformation"
                        );
                    }
                }
            }
            if !matches!(brush.alpha, Alpha::Image { .. }) {
                assert!(boundary_edges > 0);
            }
            let mut protected = original.clone();
            protected.mask.fill(1.);
            let before = protected.clone();
            assert!(!protected.prepare_drag_stamp(&brush, &[d], &settings));
            assert_eq!(protected, before);
            brush.alpha = Alpha::Image {
                width: 1,
                height: 1,
                pixels: vec![0.].into(),
            };
            let mut black = original.clone();
            assert!(!black.prepare_drag_stamp(&brush, &[d], &settings));
            assert_eq!(black, original);
        }
    }

    #[test]
    fn local_refinement_is_conforming_preserves_masks_and_obeys_work_budget() {
        let mut mesh = Mesh::sphere();
        mesh.mask.iter_mut().for_each(|m| *m = 0.4);
        let original = mesh.clone();
        let settings = AdaptiveSettings {
            enabled: true,
            detail: 2.,
            simplify: false,
            ..Default::default()
        };
        let mut budget = Budget {
            splits: 23,
            collapses: 0,
            // Isolate the split budget; quality flips have their own budget.
            flips: 0,
            ..Default::default()
        };
        assert!(mesh.adapt(
            &Brush::new(Kind::Standard),
            &[dab()],
            &settings,
            &mut budget,
            false
        ));
        assert_eq!(mesh.vertices.len(), original.vertices.len() + 23);
        assert_eq!(mesh.triangles.len(), original.triangles.len() + 46);
        assert_eq!(
            &mesh.vertices[..original.vertices.len()],
            &original.vertices
        );
        assert!(
            mesh.vertices[original.vertices.len()..]
                .iter()
                .all(|v| v.2 > 0.7)
        );
        assert!(mesh.mask.iter().all(|m| (*m - 0.4).abs() < 1e-6));
        mesh.validate_closed().unwrap();
        assert!(!mesh.adapt(
            &Brush::new(Kind::Standard),
            &[dab()],
            &settings,
            &mut budget,
            false
        ));
    }
    #[test]
    fn dense_decimation_has_a_useful_bounded_frame_budget() {
        let mut mesh = Mesh::sphere().subdivide().unwrap();
        let before = mesh.vertices.len();
        let brush = Brush::new(Kind::Decimate);
        let settings = AdaptiveSettings {
            enabled: true,
            detail: 16.,
            refine: false,
            ..Default::default()
        };
        let dab = Dab {
            radius: 0.8,
            ..dab()
        };
        let mut budget = Budget::default();
        let start = std::time::Instant::now();
        mesh.adapt(&brush, &[dab], &settings, &mut budget, true);
        let removed = before - mesh.vertices.len();
        eprintln!(
            "Dense cleanup: {before} vertices, {removed} removed in {:?}",
            start.elapsed()
        );
        assert!((33..=256).contains(&removed));
        mesh.validate_closed().unwrap();
        let once = mesh.clone();
        mesh.adapt(&brush, &[dab], &settings, &mut budget, true);
        assert_eq!(mesh, once, "interpolated dabs must share the frame budget");
    }

    #[test]
    fn dense_patch_simplification_preserves_closed_manifold_and_distant_surface() {
        let mut mesh = Mesh::sphere();
        let before = mesh.clone();
        let settings = AdaptiveSettings {
            enabled: true,
            detail: 32.,
            refine: false,
            collapse_ratio: 0.8,
            ..Default::default()
        };
        let mut budget = Budget::default();
        assert!(mesh.adapt(
            &Brush::new(Kind::Decimate),
            &[dab()],
            &settings,
            &mut budget,
            true
        ));
        assert!(mesh.vertices.len() < before.vertices.len());
        mesh.validate_closed().unwrap();
        for v in before.vertices.iter().filter(|v| v.2 < 0.5) {
            assert!(mesh.vertices.contains(v));
        }
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.len() > 0.98 && v.len() < 1.001)
        );
    }
    #[test]
    fn adaptive_smooth_collapses_only_after_smoothing_and_never_refines() {
        let original = Mesh::sphere().subdivide().unwrap();
        for simplify in [false, true] {
            let settings = AdaptiveSettings {
                enabled: true,
                detail: 16.,
                simplify,
                ..Default::default()
            };
            let mut mesh = original.clone();
            let brush = Brush::new(Kind::Smooth);
            mesh.adapt(&brush, &[dab()], &settings, &mut Budget::default(), true);
            assert_eq!(mesh, original, "Smooth must not collapse before relaxing");
            mesh.stamp(&brush, dab());
            let relaxed = mesh.clone();
            let changed = mesh.adapt(&brush, &[dab()], &settings, &mut Budget::default(), false);
            if simplify {
                assert!(changed);
                assert!(mesh.vertices.len() < relaxed.vertices.len());
            } else {
                assert!(!changed);
                assert_eq!(mesh, relaxed);
            }
            assert!(mesh.vertices.len() <= relaxed.vertices.len());
            mesh.validate_closed().unwrap();

            mesh.adapt(
                &Brush::new(Kind::Decimate),
                &[dab()],
                &settings,
                &mut Budget::default(),
                true,
            );
            assert!(mesh.vertices.len() <= original.vertices.len());
            mesh.validate_closed().unwrap();
        }
    }
    #[test]
    fn adaptive_smooth_splits_a_pinched_vertex_into_closed_fans() {
        let mut first = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Icosphere);
        first.subdivisions = 1;
        let mut mesh = first.mesh().unwrap();
        let shared = 0;
        let offset = mesh.vertices.len();
        let mut second = mesh.clone();
        for face in &mut second.triangles {
            *face = face.map(|v| if v == shared { shared } else { v + offset });
        }
        mesh.vertices.extend(second.vertices.iter().copied());
        mesh.mask.extend(second.mask.iter().copied());
        mesh.triangles.extend(second.triangles);
        mesh.polygroups.clear();
        mesh.hidden_faces.clear();
        let before_vertices = mesh.vertices.len();
        mesh.validate_closed().unwrap();

        let normal = mesh.normals()[shared];
        let dab = Dab {
            center: mesh.vertices[shared],
            normal,
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 1., 0.),
            radius: 0.35,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        };
        let brush = Brush::new(Kind::Smooth);
        assert_eq!(
            mesh.split_disconnected_fans(&brush, &[dab], &[], 4, false),
            1
        );
        assert_eq!(mesh.vertices.len(), before_vertices + 1);
        assert_eq!(mesh.vertices[before_vertices], mesh.vertices[shared]);
        mesh.validate_closed().unwrap();

        let duplicate = before_vertices;
        let first_star: Vec<_> = mesh
            .triangles
            .iter()
            .filter(|face| face.contains(&shared))
            .collect();
        let second_star: Vec<_> = mesh
            .triangles
            .iter()
            .filter(|face| face.contains(&duplicate))
            .collect();
        assert!(!first_star.is_empty());
        assert!(!second_star.is_empty());
        assert!(first_star.iter().all(|face| !face.contains(&duplicate)));
        assert!(second_star.iter().all(|face| !face.contains(&shared)));
    }
    #[test]
    fn adaptive_smooth_splits_a_closed_neck_at_the_detail_scale() {
        let around = 8;
        let levels = [(-2., 1.), (-1., 1.), (0., 0.03), (1., 1.), (2., 1.)];
        let mut mesh = Mesh::default();
        for (z, radius) in levels {
            for i in 0..around {
                let angle = std::f32::consts::TAU * i as f32 / around as f32;
                mesh.vertices
                    .push(Vec3(radius * angle.cos(), radius * angle.sin(), z));
            }
        }
        for level in 0..levels.len() - 1 {
            for i in 0..around {
                let next = (i + 1) % around;
                let a = level * around + i;
                let b = level * around + next;
                let c = (level + 1) * around + next;
                let d = (level + 1) * around + i;
                mesh.triangles.extend([[a, b, c], [a, c, d]]);
            }
        }
        let bottom = mesh.vertices.len();
        mesh.vertices.push(Vec3(0., 0., -3.));
        let top = mesh.vertices.len();
        mesh.vertices.push(Vec3(0., 0., 3.));
        for i in 0..around {
            let next = (i + 1) % around;
            mesh.triangles.push([bottom, next, i]);
            let upper = (levels.len() - 1) * around;
            mesh.triangles.push([upper + i, upper + next, top]);
        }
        mesh.mask = vec![0.; mesh.vertices.len()];
        mesh.validate_closed().unwrap();

        let dab = Dab {
            center: Vec3::default(),
            normal: Vec3(0., 0., 1.),
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 1., 0.),
            radius: 1.1,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        };
        let brush = Brush::new(Kind::Smooth);
        assert_eq!(mesh.edge_connected_face_components(), 1);
        let mut adapted = mesh.clone();
        let settings = AdaptiveSettings {
            enabled: true,
            detail: 32.,
            refine: false,
            ..Default::default()
        };
        let mut brush = brush;
        brush.radius = 50.;
        assert!(adapted.adapt(&brush, &[dab], &settings, &mut Budget::default(), false));
        assert_eq!(adapted.edge_connected_face_components(), 2);
        adapted.validate_closed().unwrap();

        assert_eq!(
            mesh.collapse_short_detail_cycles(&brush, &[dab], &[], 0.1, 1, false),
            1
        );
        assert_eq!(mesh.edge_connected_face_components(), 2);
        mesh.validate_closed().unwrap();
    }
    #[test]
    fn adaptive_smooth_collapses_a_sub_detail_terminal_loop_without_opening_surface() {
        let around = 8;
        let levels = [(-2., 1.), (-1., 1.), (0., 1.), (1., 1.), (2., 0.03)];
        let mut mesh = Mesh::default();
        for (z, radius) in levels {
            for i in 0..around {
                let angle = std::f32::consts::TAU * i as f32 / around as f32;
                mesh.vertices
                    .push(Vec3(radius * angle.cos(), radius * angle.sin(), z));
            }
        }
        for level in 0..levels.len() - 1 {
            for i in 0..around {
                let next = (i + 1) % around;
                let a = level * around + i;
                let b = level * around + next;
                let c = (level + 1) * around + next;
                let d = (level + 1) * around + i;
                mesh.triangles.extend([[a, b, c], [a, c, d]]);
            }
        }
        let bottom = mesh.vertices.len();
        mesh.vertices.push(Vec3(0., 0., -3.));
        let top = mesh.vertices.len();
        mesh.vertices.push(Vec3(0., 0., 3.));
        for i in 0..around {
            let next = (i + 1) % around;
            mesh.triangles.push([bottom, next, i]);
            let upper = (levels.len() - 1) * around;
            mesh.triangles.push([upper + i, upper + next, top]);
        }
        mesh.mask = vec![0.; mesh.vertices.len()];
        mesh.validate_closed().unwrap();
        let before_vertices = mesh.vertices.len();

        let dab = Dab {
            center: Vec3(0., 0., 2.5),
            normal: Vec3(0., 0., 1.),
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 1., 0.),
            radius: 1.1,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        };
        assert_eq!(mesh.edge_connected_face_components(), 1);
        assert_eq!(
            mesh.collapse_short_detail_cycles(
                &Brush::new(Kind::Smooth),
                &[dab],
                &[],
                0.1,
                1,
                false
            ),
            1
        );
        assert!(mesh.vertices.len() < before_vertices);
        assert_eq!(mesh.edge_connected_face_components(), 1);
        mesh.validate_closed().unwrap();
    }
    #[test]
    fn adaptive_smooth_removes_only_a_fully_covered_sub_detail_component() {
        let mut primitive =
            crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Icosphere);
        primitive.subdivisions = 1;
        let mut small = primitive.mesh().unwrap();
        for point in &mut small.vertices {
            *point = *point * 0.01;
        }
        let pinch = small.vertices[0];
        let mut large = primitive.mesh().unwrap();
        let offset = small.vertices.len();
        for point in &mut large.vertices {
            *point = *point + Vec3(10., 0., 0.);
        }
        small.vertices.extend(large.vertices);
        small.mask.extend(large.mask);
        small.triangles.extend(
            large
                .triangles
                .into_iter()
                .map(|face| face.map(|v| v + offset)),
        );
        let mut mesh = small;
        mesh.validate_closed().unwrap();
        assert_eq!(mesh.edge_connected_face_components(), 2);

        let dab = Dab {
            center: Vec3(5., 0., 0.),
            normal: Vec3(0., 0., 1.),
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 1., 0.),
            radius: 20.,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        };
        let brush = Brush::new(Kind::Smooth);
        let before = mesh.clone();
        assert!(!mesh.remove_sub_detail_pinched_component(pinch, 0.001, &brush, &[dab], &[],));
        assert_eq!(
            mesh, before,
            "sub-detail pruning must follow the detail scale"
        );
        assert!(mesh.remove_sub_detail_pinched_component(pinch, 0.1, &brush, &[dab], &[],));
        assert_eq!(mesh.edge_connected_face_components(), 1);
        mesh.validate_closed().unwrap();
    }
    #[test]
    fn protected_zero_alpha_and_mask_brush_leave_topology_unchanged() {
        let settings = AdaptiveSettings {
            enabled: true,
            detail: 1.,
            ..Default::default()
        };
        let mut mesh = Mesh::sphere();
        mesh.mask.fill(1.);
        let before = mesh.clone();
        assert!(!mesh.adapt(
            &Brush::new(Kind::Snake),
            &[dab()],
            &settings,
            &mut Budget::default(),
            true
        ));
        assert_eq!(mesh, before);
        mesh.mask.fill(0.);
        let before = mesh.clone();
        assert!(!mesh.adapt(
            &Brush::new(Kind::Mask),
            &[dab()],
            &settings,
            &mut Budget::default(),
            true
        ));
        let mut brush = Brush::new(Kind::Snake);
        brush.alpha = Alpha::Image {
            width: 1,
            height: 1,
            pixels: vec![0.].into(),
        };
        assert!(!mesh.adapt(&brush, &[dab()], &settings, &mut Budget::default(), true));
        assert_eq!(mesh, before);
    }
    #[test]
    fn decimation_respects_zero_pressure_strength_and_masks() {
        let original = Mesh::sphere();
        for mode in 0..4 {
            let mut mesh = original.clone();
            let mut brush = Brush::new(Kind::Decimate);
            let mut d = dab();
            let mut settings = AdaptiveSettings {
                enabled: true,
                detail: 32.,
                ..Default::default()
            };
            match mode {
                0 => d.pressure = 0.,
                1 => brush.strength = 0.,
                2 => mesh.mask.fill(1.),
                _ => settings.enabled = false,
            }
            let before = mesh.clone();
            assert!(!mesh.adapt(&brush, &[d], &settings, &mut Budget::default(), true));
            assert!(!mesh.stamp_many(&brush, &[d]));
            assert_eq!(mesh, before);
        }
    }
    #[test]
    fn density_scales_with_view_and_relative_brush_size() {
        let mut settings = AdaptiveSettings::default();
        let brush = Brush::new(Kind::Standard);
        let d = dab();
        let mut closer = d;
        closer.radius *= 0.5;
        assert!(
            (settings.edge_length(&brush, closer) * 2. - settings.edge_length(&brush, d)).abs()
                < 1e-6
        );
        settings.relative = true;
        assert!((settings.edge_length(&brush, d) - d.radius * settings.detail / 100.).abs() < 1e-6);
        settings.detail = f32::NAN;
        assert!(settings.validate().is_err());
    }
}

#[cfg(all(test, not(feature = "focused-retopology")))]
mod group_tests {
    use super::*;
    #[test]
    fn adaptive_topology_keeps_face_visibility_and_hidden_positions() {
        let mut m = crate::sculpt::Mesh::starting_sphere();
        m.hidden_faces = m
            .triangles
            .iter()
            .map(|t| t.iter().any(|&v| m.vertices[v].0 > 0.))
            .collect();
        let hidden_points: Vec<_> = m
            .triangles
            .iter()
            .enumerate()
            .filter(|(i, _)| !m.face_visible(*i))
            .flat_map(|(_, t)| t.iter().map(|&v| m.vertices[v]))
            .collect();
        let brush = Brush::new(Kind::Standard);
        let dab = Dab {
            center: Vec3(-1., 0., 0.),
            normal: Vec3(-1., 0., 0.),
            right: Vec3(0., 0., 1.),
            up: Vec3(0., 1., 0.),
            radius: 2.,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        };
        assert!(m.split_local(&brush, &[dab], &[], 0.02, 16, false) > 0);
        m.validate_closed().unwrap();
        assert!(m.hidden_face_count() > 0);
        m.collapse_local(
            &brush,
            &[dab],
            &[],
            CollapseOptions {
                threshold: 0.3,
                limit: 16,
                unmasked_only: false,
                preserve_surface: false,
            },
        );
        m.validate_closed().unwrap();
        assert!(m.hidden_face_count() > 0);
        for p in hidden_points {
            assert!(m.vertices.contains(&p));
        }
    }
    #[test]
    fn adaptive_splits_and_collapses_preserve_groups() {
        let mut m = crate::sculpt::Primitive::for_kind(crate::sculpt::PrimitiveKind::Cube)
            .mesh()
            .unwrap();
        m.create_vertex_group("Whole".into(), 0.5, true).unwrap();
        let brush = Brush::new(Kind::Standard);
        let dab = Dab {
            center: Vec3(0., 0., 1.),
            normal: Vec3(0., 0., 1.),
            right: Vec3(1., 0., 0.),
            up: Vec3(0., 1., 0.),
            radius: 10.,
            pressure: 1.,
            delta: Vec3::default(),
            invert: false,
        };
        assert!(m.split_local(&brush, &[dab], &[], 0.1, 16, false) > 0);
        m.validate_closed().unwrap();
        assert_eq!(m.polygroups, vec![1; m.triangles.len()]);
        assert!(m.vertex_groups[0].weights.iter().all(|&w| w == 1.));
        let count = m.collapse_local(
            &brush,
            &[dab],
            &[],
            CollapseOptions {
                threshold: 3.,
                limit: 16,
                unmasked_only: false,
                preserve_surface: false,
            },
        );
        assert!(count > 0);
        m.validate_closed().unwrap();
        assert_eq!(m.polygroups, vec![1; m.triangles.len()]);
        assert!(m.vertex_groups[0].weights.iter().all(|&w| w == 1.));
    }
}
