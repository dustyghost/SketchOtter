use super::*;
pub(super) fn part_picker(
    ui: &mut egui::Ui,
    controls: &mut Controls,
    document: &Document,
    array: &mut SurfaceArray,
) {
    ui.strong("1. Choose a captured part");
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
                        && array.piece.source != Source::Asset(part.id)
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
pub(super) fn editor(ui: &mut egui::Ui, a: &mut SurfaceArray, document: &Document) {
    let owner = source_name(document, Source::Model(a.owner)).unwrap_or("Missing guide");
    ui.strong("2. Guide faces");
    ui.label(format!("Guide: {owner}"));
    ui.label(format!(
        "{} guide polygons · one copy per face",
        a.faces.len()
    ));
    ui.weak("Outlines show the assigned faces. Apply, then Edit guide to change the region.");
    ui.strong("3. Fit the part");
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut a.replace, false, "Keep faces");
        ui.selectable_value(&mut a.replace, true, "Replace faces");
    });
    if a.replace {
        ui.colored_label(egui::Color32::YELLOW, "Replace hides each whole guide face. Smaller parts leave holes; fitting does not seal or clip the borders.");
    }
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
    ui.weak("Uniform scales depth too. Stretch in plane keeps the part’s depth.");
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
    egui::CollapsingHeader::new("Orientation and contact point").show(ui, |ui| {
        ui.weak(
            "Up follows the face normal. Forward follows its first edge. Choose different axes.",
        );
        axes(ui, &mut a.piece.forward, &mut a.piece.up);
        vector(ui, "Contact point", &mut a.piece.pivot);
        if let Source::Asset(id) = a.piece.source
            && let Some(part) = document.instances().assets.iter().find(|p| p.id == id)
            && ui.button("Reset orientation and contact").clicked()
        {
            a.piece.pivot = part.pivot;
            a.piece.forward = part.forward;
            a.piece.up = part.up;
        }
    });
    ui.weak(
        "Moving guide vertices updates copies. Connectivity changes require a new face selection.",
    );
}
