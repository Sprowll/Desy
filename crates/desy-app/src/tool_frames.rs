//! Floating, collapsible frames holding the options of the current tool,
//! stacked at the bottom right, next to the side panel, above the view pill.
//!
//! To give a tool its own frame: add a `Frame` variant, say when it shows in
//! `frames()`, and render its body in `frame_body()`.

use crate::editor::{Editor, Tool};
use crate::icons::{Icon, icon};
use crate::motion::{Enter, From};
use crate::panels::info;
use crate::theme;
use crate::view::{EditorView, FRAMES_BOTTOM, M, PANEL_W};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub const FRAME_W: f32 = 272.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    /// Layer (and shape) symmetry.
    Mirror,
    /// Variable stroke width.
    Width,
}

impl Frame {
    fn id(self) -> &'static str {
        match self {
            Frame::Mirror => "frame-mirror",
            Frame::Width => "frame-width",
        }
    }

    fn info_id(self) -> &'static str {
        match self {
            Frame::Mirror => "frame-mirror-info",
            Frame::Width => "frame-width-info",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Frame::Mirror => "Symétrie",
            Frame::Width => "Épaisseur variable",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Frame::Mirror => Icon::Mirror,
            Frame::Width => Icon::Width,
        }
    }
}

/// Frames to show for the current tool and selection, top to bottom.
pub fn frames_for(ed: &Editor) -> Vec<Frame> {
    let mut out = Vec::new();
    let layer_axis = ed.doc.layers.get(ed.mirror_layer()).is_some_and(|l| l.mirror.is_some());
    // With a selection, an existing axis is a card of the Effects window;
    // without one, its frame keeps it reachable from any tool.
    if ed.tool == Tool::Mirror || (layer_axis && ed.selected.is_empty()) {
        out.push(Frame::Mirror);
    }
    if ed.tool == Tool::Width || (ed.tool == Tool::Direct && !ed.nodes.is_empty()) {
        out.push(Frame::Width);
    }
    out
}

impl EditorView {
    /// Frames to show for the current tool and selection, top to bottom.
    pub fn frames(&self) -> Vec<Frame> {
        frames_for(&self.ed)
    }

    fn frame_body(&self, frame: Frame, cx: &mut Context<Self>) -> Div {
        match frame {
            Frame::Mirror => self.mirror_options(cx),
            Frame::Width => self.width_options(cx),
        }
    }

    fn frame_tip(&self, frame: Frame) -> &'static str {
        match frame {
            Frame::Mirror => self.mirror_tip(),
            Frame::Width => "Outil Largeur (W) : glisse depuis un tracé pour l'épaissir à cet endroit.",
        }
    }

    pub fn tool_frames(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let frames = self.frames();
        if frames.is_empty() {
            return None;
        }
        let mut col = v_flex()
            .absolute()
            .right(px(M + PANEL_W + 8.0))
            .bottom(px(FRAMES_BOTTOM))
            .w(px(FRAME_W))
            .gap_2()
            .items_end();
        for frame in frames {
            let collapsed = self.collapsed_frames.contains(&frame);
            let header = h_flex()
                .id(frame.id())
                .h(px(44.0))
                .pl(px(14.0))
                .pr(px(6.0))
                .gap_2()
                .items_center()
                .when(!collapsed, |d| d.w_full())
                .child(icon(frame.icon(), 18.0, theme::accent()))
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(frame.title()))
                .child(info(frame.info_id(), self.frame_tip(frame)))
                .when(!collapsed, |d| d.child(div().flex_1()))
                .child(
                    div()
                        .size(px(32.0))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .hover(|d| d.bg(theme::hover()))
                        .child(icon(
                            if collapsed { Icon::ChevronUp } else { Icon::ChevronDown },
                            18.0,
                            theme::text_dim(),
                        )),
                )
                .on_click(cx.listener(move |v, _, _, cx| {
                    if let Some(i) = v.collapsed_frames.iter().position(|f| *f == frame) {
                        v.collapsed_frames.remove(i);
                    } else {
                        v.collapsed_frames.push(frame);
                    }
                    cx.notify();
                }));
            let card = v_flex()
                .occlude()
                .overflow_hidden()
                .bg(theme::panel())
                .border_1()
                .border_color(theme::border())
                .shadow_lg()
                .child(header);
            let card = if collapsed {
                card.rounded_full()
            } else {
                card.w_full().rounded(px(20.0)).pb_3().child(self.frame_body(frame, cx))
            };
            col =
                col.child(Enter::card(From::Right).run(ElementId::Name(format!("{}-enter", frame.id()).into()), card));
        }
        Some(col.into_any_element())
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: GPUI's glob import brings its own `test` attribute.
    use super::{Frame, frames_for};
    use crate::editor::{Editor, Tool};
    use desy_core::{Document, FontLibrary};

    #[test]
    fn frames_follow_the_tool() {
        let mut ed = Editor::new(Document::new("t", 100.0, 100.0), None, FontLibrary::empty());
        assert!(frames_for(&ed).is_empty());
        ed.set_tool(Tool::Mirror);
        assert_eq!(frames_for(&ed), vec![Frame::Mirror]);
        ed.set_tool(Tool::Width);
        assert_eq!(frames_for(&ed), vec![Frame::Width]);
        ed.set_tool(Tool::Rect);
        assert!(frames_for(&ed).is_empty());
    }

    #[test]
    fn a_layer_axis_keeps_its_frame_with_any_tool() {
        let mut ed = Editor::new(Document::new("t", 100.0, 100.0), None, FontLibrary::empty());
        ed.mirror_layer_center(true);
        ed.set_tool(Tool::Select);
        assert_eq!(frames_for(&ed), vec![Frame::Mirror]);
    }
}
