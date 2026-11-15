use super::*;
pub(super) fn part_picker(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    document: &Document,
    array: &mut SurfaceArray,
) {
    ui.strong("Captured parts · Object brush library");
    egui::ScrollArea::vertical()
        .id_salt("surface-parts")
        .max_height(145.)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for part in &document.instances().assets {
                    if design::brush_tile_with_icon(
                        ui,
                        &part.name,
                        array.piece.source == Source::Asset(part.id),
                        74.,
                        |ctx| controls.object_icon(ctx, part),
                    )
                    .clicked()
                    {
                        array.piece = Piece {
                            source: Source::Asset(part.id),
                            pivot: part.pivot,
                            forward: part.forward,
                            up: part.up,
                        };
                    }
                }
            });
        });
}
pub(super) fn editor(ui: &mut egui::Ui, a: &mut SurfaceArray) {
    ui.label(format!(
        "{} guide polygons · one copy per face",
        a.faces.len()
    ));
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut a.replace, false, "Keep faces");
        ui.selectable_value(&mut a.replace, true, "Replace faces");
    });
    ui.weak(
        "Replace hides only assigned guide faces. Disabling or removing the array restores them.",
    );
    axes(ui, &mut a.piece.forward, &mut a.piece.up);
    vector(ui, "Contact point", &mut a.piece.pivot);
    ui.add(
        egui::DragValue::new(&mut a.scale)
            .range(0.001..=1000.)
            .speed(0.01)
            .prefix("Scale "),
    );
    ui.add(
        egui::DragValue::new(&mut a.rotation)
            .range(-3600. ..=3600.)
            .speed(1.)
            .prefix("Face rotation ° "),
    );
    ui.add(
        egui::DragValue::new(&mut a.offset)
            .range(-1e5..=1e5)
            .speed(0.01)
            .prefix("Normal offset "),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label("Fit");
        for (fit, label) in [
            (SurfaceFit::None, "Original size"),
            (SurfaceFit::Uniform, "Uniform"),
            (SurfaceFit::Stretch, "Stretch in plane"),
        ] {
            ui.selectable_value(&mut a.fit, fit, label);
        }
    });
    ui.weak("Fits the part’s footprint around its contact point to the face’s projected bounds. Uniform scales depth too; Stretch preserves depth. This does not clip to polygon borders.");
    ui.weak("Moving guide vertices updates placements. Connectivity or subdivision changes suspend the region until you explicitly select new faces. Apply, then Edit guide to change its region.");
}
