//! Right-click menu on the canvas: clipboard, order, conversions, booleans,
//! image actions. Its entries depend on the selection.

use crate::icons::{Icon, icon};
use crate::motion::{Enter, From};
use crate::theme;
use crate::view::EditorView;
use desy_core::{BoolOp, Shape};
use gpui_kit::base::v_flex;
use gpui_kit::prelude::*;
use gpui_kit::*;

const MENU_W: f32 = 264.0;
const ITEM_H: f32 = 36.0;
const SEPARATOR_H: f32 = 9.0;
const PAD: f32 = 6.0;

type Run = fn(&mut EditorView, &mut Window, &mut Context<EditorView>);

enum Entry {
    Item { icon: Icon, label: &'static str, keys: &'static str, enabled: bool, run: Run },
    Separator,
}

fn item(icon: Icon, label: &'static str, keys: &'static str, enabled: bool, run: Run) -> Entry {
    Entry::Item { icon, label, keys, enabled, run }
}

impl EditorView {
    /// Entries for the current selection.
    fn context_entries(&self) -> Vec<Entry> {
        let ed = &self.ed;
        let objs: Vec<_> = ed.selected.iter().filter_map(|id| ed.doc.get(*id)).collect();
        let n = objs.len();
        let any = n > 0;
        let all_images = any && objs.iter().all(|o| matches!(o.shape, Shape::Image { .. }));
        let shapes = objs.iter().any(|o| !matches!(o.shape, Shape::Image { .. } | Shape::Path(_)));
        let strokes = objs.iter().any(|o| o.stroke.is_some_and(|s| s.width > 0.0));

        let mut out = vec![
            item(Icon::Cut, "Couper", "⌘X", any, |v, _, _| v.ed.cut()),
            item(Icon::Copy, "Copier", "⌘C", any, |v, _, _| v.ed.copy()),
            item(Icon::Paste, "Coller", "⌘V", true, |v, _, _| v.ed.paste()),
            item(Icon::Duplicate, "Dupliquer", "⌘D", any, |v, _, _| v.ed.duplicate()),
            item(Icon::Trash, "Supprimer", "⌫", any, |v, _, _| v.ed.delete()),
        ];
        if !any {
            out.push(Entry::Separator);
            out.push(item(Icon::Select, "Tout sélectionner", "⌘A", true, |v, _, _| v.ed.select_all()));
            out.push(item(Icon::Fit, "Ajuster à la fenêtre", "⌘0", true, |v, _, _| v.ed.fit()));
            out.push(item(Icon::Image, "Importer une image…", "⌘I", true, |v, window, cx| {
                v.import_dialog(window, cx)
            }));
            return out;
        }

        out.push(Entry::Separator);
        out.push(item(Icon::ToFront, "Premier plan", "⇧⌘]", true, |v, _, _| v.ed.reorder(i64::MAX)));
        out.push(item(Icon::Up, "Avancer", "⌘]", true, |v, _, _| v.ed.reorder(1)));
        out.push(item(Icon::Down, "Reculer", "⌘[", true, |v, _, _| v.ed.reorder(-1)));
        out.push(item(Icon::ToBack, "Arrière-plan", "⇧⌘[", true, |v, _, _| v.ed.reorder(i64::MIN)));

        if n >= 2 {
            out.push(Entry::Separator);
            out.push(item(Icon::Union, "Union", "⌥⌘U", true, |v, _, _| v.ed.boolean(BoolOp::Union)));
            out.push(item(Icon::Subtract, "Soustraire l'avant", "⌥⌘S", true, |v, _, _| {
                v.ed.boolean(BoolOp::Subtract)
            }));
            out.push(item(Icon::Intersect, "Intersection", "⌥⌘I", true, |v, _, _| v.ed.boolean(BoolOp::Intersect)));
            out.push(item(Icon::Exclude, "Exclusion", "⌥⌘X", true, |v, _, _| v.ed.boolean(BoolOp::Exclude)));
        }

        out.push(Entry::Separator);
        out.push(item(Icon::FlipH, "Miroir horizontal", "⇧H", true, |v, _, _| v.ed.flip(true)));
        out.push(item(Icon::FlipV, "Miroir vertical", "⇧V", true, |v, _, _| v.ed.flip(false)));
        if !all_images {
            out.push(item(Icon::Pen, "Convertir en tracé", "", shapes, |v, _, _| v.ed.convert_to_path()));
            out.push(item(Icon::Width, "Vectoriser le contour", "", strokes, |v, _, _| v.ed.outline_stroke()));
        }
        if all_images {
            out.push(Entry::Separator);
            out.push(item(Icon::Image, "En faire une référence", "", true, |v, _, _| v.ed.make_reference()));
            out.push(item(Icon::Fit, "Taille réelle", "", true, |v, _, _| v.ed.image_original_size()));
        }

        let active = ed.active_layer;
        let elsewhere = objs.iter().any(|o| ed.doc.find(o.id).is_some_and(|l| l.layer != active));
        out.push(Entry::Separator);
        out.push(item(Icon::Layers, "Envoyer vers le calque actif", "", elsewhere, |v, _, _| {
            let li = v.ed.active_layer;
            v.ed.move_selection_to_layer(li)
        }));
        out
    }

    pub fn context_menu(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let at = self.context_menu?;
        let entries = self.context_entries();
        let height = PAD * 2.0
            + entries
                .iter()
                .map(|e| match e {
                    Entry::Item { .. } => ITEM_H,
                    Entry::Separator => SEPARATOR_H,
                })
                .sum::<f32>();
        // Keep the menu inside the window.
        let vp = window.viewport_size();
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
        let x = f32::from(at.x).min(vw - MENU_W - 8.0).max(8.0);
        let y = f32::from(at.y).min(vh - height - 8.0).max(8.0);

        let mut menu = v_flex()
            .id("context-menu")
            .occlude()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(MENU_W))
            .p(px(PAD))
            .rounded(px(theme::RADIUS_MENU))
            .bg(theme::panel_high())
            .border_1()
            .border_color(theme::border())
            .shadow_lg();
        for (i, e) in entries.into_iter().enumerate() {
            menu = match e {
                Entry::Separator => menu.child(div().h(px(1.0)).my(px(4.0)).mx_2().flex_none().bg(theme::border())),
                Entry::Item { icon: ic, label, keys, enabled, run } => menu.child(
                    Enter::row(i).wrap(
                        ("ctx-row", i),
                        div()
                            .id(("ctx-item", i))
                            .h(px(ITEM_H))
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap_3()
                            .px_3()
                            .rounded(px(10.0))
                            .when(!enabled, |d| d.opacity(0.38))
                            .when(enabled, |d| {
                                d.hover(|d| d.bg(theme::hover())).active(|d| d.bg(theme::pressed())).on_click(
                                    cx.listener(move |v, _, window, cx| {
                                        v.context_menu = None;
                                        run(v, window, cx);
                                        window.focus(&v.canvas_focus, cx);
                                        v.after_edit(cx);
                                    }),
                                )
                            })
                            .child(icon(ic, 18.0, theme::text_dim()))
                            .child(div().flex_1().text_sm().child(label))
                            .child(div().text_xs().text_color(theme::text_faint()).child(keys)),
                    ),
                ),
            };
        }

        let close = |v: &mut EditorView, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<EditorView>| {
            v.context_menu = None;
            cx.notify();
        };
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(
                    // Click anywhere else to close.
                    div()
                        .id("context-menu-backdrop")
                        .occlude()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(MouseButton::Left, cx.listener(close))
                        .on_mouse_down(MouseButton::Right, cx.listener(close)),
                )
                .child(Enter::menu(From::Above).run("context-menu-enter", menu))
                .into_any_element(),
        )
    }
}
