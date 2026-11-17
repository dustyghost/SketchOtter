//! The temporary mask remembers its own shape and brush preset, not the sculpt tool.
use crate::{document::Document, sculpt::Kind, viewport::SelectionTool};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum MaskShape {
    #[default]
    Brush,
    Rectangle,
    Lasso,
    Line,
}
impl MaskShape {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Brush => "Surface brush",
            Self::Lasso => "Lasso",
            Self::Line => "Line",
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct MaskPreset {
    pub shape: MaskShape,
    pub visible_only: bool,
    pub erase: bool,
    pub flip: bool,
    pub brush_id: Option<String>,
}
impl MaskPreset {
    pub(crate) fn brush_index(&self, document: &Document) -> Option<usize> {
        self.brush_id
            .as_ref()
            .and_then(|id| {
                document
                    .brushes()
                    .iter()
                    .position(|b| b.kind == Kind::Mask && b.identity() == *id)
            })
            .or_else(|| document.brushes().iter().position(|b| b.kind == Kind::Mask))
    }
    pub(crate) fn selection_tool(&self) -> Option<SelectionTool> {
        if self.shape == MaskShape::Rectangle
            || (self.visible_only && self.shape != MaskShape::Brush)
        {
            use crate::viewport::{RegionOperation, RegionShape, RegionTool};
            return Some(SelectionTool::Region(RegionTool {
                shape: match self.shape {
                    MaskShape::Rectangle => RegionShape::Rectangle,
                    MaskShape::Lasso => RegionShape::Lasso,
                    _ => RegionShape::Line,
                },
                through: !self.visible_only,
                flip: self.flip && self.shape == MaskShape::Line,
                operation: RegionOperation::Mask { erase: self.erase },
            }));
        }
        match self.shape {
            MaskShape::Brush => None,
            MaskShape::Rectangle => unreachable!(),
            MaskShape::Lasso => Some(SelectionTool::MaskLassoOperation { erase: self.erase }),
            MaskShape::Line => Some(SelectionTool::MaskLine {
                erase: self.erase,
                flip: self.flip,
            }),
        }
    }
}
