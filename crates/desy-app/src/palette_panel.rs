//! Palette block of the right panel: project swatches in a grid (as in
//! Aseprite), colors used in the document (as in Canva), presets and palette
//! files (`.gpl`, `.hex`, `.txt`).

use crate::color_field::FieldLook;
use crate::editor::{Editor, Tool};
use crate::panels::{chip, small_button};
use crate::view::EditorView;
use crate::{projects, theme};
use desy_core::palette::{self, PRESETS};
use desy_core::{Color, Document};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::color_picker::{ColorPickerEvent, ColorPickerState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::path::{Path, PathBuf};

const SWATCH: f32 = 22.0;

/// Extensions accepted when a palette file is dropped on the canvas.
pub fn is_palette_path(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "gpl" | "hex"))
}

/// A color being chosen with the palette's picker. Slider drags emit a
/// change per step: they all update the same swatch and, once the picker
/// closes, count as a single undo step.
#[derive(Default)]
pub struct PaletteEdit {
    /// Swatch being edited; `None` = the next pick adds a new swatch.
    pub target: Option<usize>,
    /// The target swatch was created by this session.
    created: bool,
    /// Document before the first change of the session.
    origin: Option<Document>,
}

impl PaletteEdit {
    pub fn is_active(&self) -> bool {
        self.target.is_some() || self.origin.is_some()
    }

    /// Start editing an existing swatch.
    pub fn edit_swatch(&mut self, index: usize) {
        self.target = Some(index);
        self.created = false;
    }

    pub fn pick(&mut self, ed: &mut Editor, color: Color) {
        if self.origin.is_none() {
            self.origin = Some(ed.doc.clone());
        }
        match self.target {
            Some(i) if i < ed.doc.palette.len() => ed.doc.palette[i] = color,
            _ => {
                ed.doc.palette.push(color);
                self.target = Some(ed.doc.palette.len() - 1);
                self.created = true;
            }
        }
        ed.touch();
    }

    /// Picker closed: record one undo step and go back to "new color".
    pub fn finish(&mut self, ed: &mut Editor) {
        if let (Some(i), true) = (self.target, self.created)
            && let Some(&c) = ed.doc.palette.get(i)
            && ed.doc.palette.iter().enumerate().any(|(j, o)| j != i && palette::same(*o, c))
        {
            ed.doc.palette.remove(i);
            ed.status = Some(format!("{} est déjà dans la palette", c.to_hex()));
        }
        if let Some(origin) = self.origin.take() {
            ed.commit(origin);
        }
        self.target = None;
        self.created = false;
    }
}

/// The picker used to create and edit swatches, opened on its HSLA sliders.
pub fn new_palette_picker(
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> (Entity<ColorPickerState>, Vec<Subscription>) {
    let state = cx.new(|cx| ColorPickerState::new(window, cx));
    let subs = vec![
        cx.subscribe_in(&state, window, |v: &mut EditorView, _s, ev: &ColorPickerEvent, _window, cx| {
            if let ColorPickerEvent::Change(Some(c)) = ev {
                v.props.palette_edit.pick(&mut v.ed, theme::from_hsla(*c));
                cx.notify();
            }
        }),
        cx.observe_in(&state, window, |v: &mut EditorView, s, _window, cx| {
            // Hidden while the eyedropper samples for it: the session goes on.
            if !s.read(cx).is_open() && v.props.palette_edit.is_active() && !v.picker_waits_for_eyedrop(&s) {
                v.props.palette_edit.finish(&mut v.ed);
                v.after_edit(cx);
            }
        }),
    ];
    (state, subs)
}

impl EditorView {
    /// End a palette edit whose picker is closed (eyedropper abandoned).
    pub fn finish_idle_palette_edit(&mut self, cx: &mut Context<Self>) {
        if self.props.palette_edit.is_active() && !self.props.palette_picker.read(cx).is_open() {
            self.props.palette_edit.finish(&mut self.ed);
            self.after_edit(cx);
        }
    }

    /// While idle, the picker starts from the current color, so "+" then a
    /// small adjustment gives a variation of it.
    pub fn sync_palette_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.props.palette_picker.clone();
        if state.read(cx).is_open() || self.props.palette_edit.is_active() {
            return;
        }
        let (fill, stroke) = self.current_colors();
        if let Some(c) = fill.or(stroke)
            && !state.read(cx).value().is_some_and(|v| palette::same(theme::from_hsla(v), c))
        {
            state.update(cx, |s, cx| s.set_value(theme::to_hsla(c), window, cx));
        }
    }

    /// Open the picker on an existing swatch.
    fn edit_swatch(&mut self, index: usize, color: Color, window: &mut Window, cx: &mut Context<Self>) {
        self.props.palette_picker.update(cx, |s, cx| {
            s.set_open(true, cx);
            s.set_value(theme::to_hsla(color), window, cx);
        });
        self.props.palette_edit.edit_swatch(index);
        cx.notify();
    }

    /// Fill (or stroke) color shown for the selection, or the default style
    /// used by the next drawings when nothing is selected.
    fn current_colors(&self) -> (Option<Color>, Option<Color>) {
        match self.ed.selected.first().and_then(|id| self.ed.doc.get(*id)) {
            Some(o) => (o.fill, o.stroke.map(|s| s.color)),
            None => (self.ed.shape_style.fill, self.ed.line_style.stroke.map(|s| s.color)),
        }
    }

    /// Apply a swatch to the selection (or to the next drawings).
    pub fn apply_swatch(&mut self, color: Color, to_stroke: bool, cx: &mut Context<Self>) {
        self.ed.apply_color(color, to_stroke);
        self.after_edit(cx);
    }

    fn palette_add_current(&mut self, cx: &mut Context<Self>) {
        let (fill, stroke) = self.current_colors();
        let Some(color) = fill.or(stroke) else {
            self.notify_user("Aucune couleur à ajouter : active un remplissage ou un contour", true, cx);
            return;
        };
        let mut added = false;
        self.ed.edit(|ed| added = ed.doc.palette_add(color));
        if !added {
            self.notify_user(format!("{} est déjà dans la palette", color.to_hex()), false, cx);
        }
        self.after_edit(cx);
    }

    fn palette_add_all(&mut self, colors: Vec<Color>, cx: &mut Context<Self>) -> usize {
        let mut n = 0;
        self.ed.edit(|ed| {
            for c in colors {
                n += ed.doc.palette_add(c) as usize;
            }
        });
        self.after_edit(cx);
        n
    }

    pub fn import_palette_dialog(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Importer une palette (.gpl, .hex, .txt)".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |v, cx| v.import_palettes(paths, cx));
            }
        })
        .detach();
    }

    /// Append the colors of palette files to the project palette.
    pub fn import_palettes(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mut colors = Vec::new();
        let mut errors = Vec::new();
        for p in &paths {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            match std::fs::read_to_string(p) {
                Ok(text) => {
                    let found = palette::parse(&text);
                    if found.is_empty() {
                        errors.push(format!("{name} : aucune couleur reconnue"));
                    }
                    colors.extend(found);
                }
                Err(e) => errors.push(format!("{name} : {e}")),
            }
        }
        if !errors.is_empty() {
            self.notify_user(errors.join(" · "), true, cx);
        }
        if !colors.is_empty() {
            let total = colors.len();
            let added = self.palette_add_all(colors, cx);
            let msg = match (added, total - added) {
                (0, _) => "Toutes ces couleurs sont déjà dans la palette".to_string(),
                (a, 0) => format!("{a} couleur(s) ajoutée(s) à la palette"),
                (a, d) => format!("{a} couleur(s) ajoutée(s), {d} déjà présente(s)"),
            };
            if errors.is_empty() {
                self.notify_user(msg, false, cx);
            }
        }
    }

    fn export_palette(&mut self, cx: &mut Context<Self>) {
        if self.ed.doc.palette.is_empty() {
            self.notify_user("La palette est vide", true, cx);
            return;
        }
        let gpl = palette::to_gpl(&self.ed.doc.name, &self.ed.doc.palette);
        let dir = self
            .ed
            .file
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(projects::projects_dir);
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{}.gpl", self.ed.doc.name)));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut path))) = rx.await {
                if path.extension().is_none() {
                    path.set_extension("gpl");
                }
                let res = std::fs::write(&path, gpl);
                let _ = this.update(cx, |v, cx| match res {
                    Ok(()) => v.notify_user(format!("Palette exportée : {}", path.display()), false, cx),
                    Err(e) => v.notify_user(format!("Échec de l'export : {e}"), true, cx),
                });
            }
        })
        .detach();
    }

    // ------------------------------------------------------------ rendering

    pub fn palette_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let swatches = self.ed.doc.palette.clone();
        let (fill, stroke) = self.current_colors();
        let is = |c: Color, cur: Option<Color>| cur.is_some_and(|x| palette::same(x, c));
        let open = self.props.palette_menu_open;
        let editing = self.props.palette_edit.target.filter(|_| self.props.palette_picker.read(cx).is_open());

        let mut grid = h_flex().flex_wrap().gap_1();
        for (i, c) in swatches.iter().copied().enumerate() {
            let tip: SharedString = format!(
                "{} — clic : remplissage · ⇧ clic ou clic droit : contour · double-clic : modifier · ⌥ clic : retirer",
                c.to_hex()
            )
            .into();
            grid = grid.child(
                swatch(("swatch", i), c, is(c, fill), is(c, stroke))
                    .when(editing == Some(i), |d| d.border_2().border_color(theme::accent()))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |v, ev: &ClickEvent, window, cx| {
                        let m = ev.modifiers();
                        if ev.click_count() >= 2 && !m.alt && !m.shift {
                            v.edit_swatch(i, c, window, cx);
                        } else if m.alt {
                            v.ed.edit(|ed| {
                                if i < ed.doc.palette.len() {
                                    ed.doc.palette.remove(i);
                                }
                            });
                            v.after_edit(cx);
                        } else {
                            v.apply_swatch(c, m.shift, cx);
                        }
                    }))
                    .on_aux_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
                        if ev.is_right_click() {
                            v.apply_swatch(c, true, cx);
                        }
                    })),
            );
        }
        // Saved gradients follow the colors.
        let current_gradient = self.ed.selection_gradient().cloned();
        for (i, g) in self.ed.doc.palette_gradients.iter().enumerate() {
            let g2 = g.clone();
            grid = grid.child(
                crate::fill_popover::gradient_swatch(
                    ("palette-gradient", i),
                    g,
                    SWATCH,
                    current_gradient.as_ref() == Some(g),
                )
                .tooltip(|window, cx| Tooltip::new("Dégradé — clic : remplissage · ⌥ clic : retirer").build(window, cx))
                .on_click(cx.listener(move |v, ev: &ClickEvent, window, cx| {
                    if ev.modifiers().alt {
                        v.remove_palette_gradient(i, cx);
                    } else {
                        v.apply_palette_gradient(g2.clone(), window, cx);
                    }
                })),
            );
        }
        grid = grid.child(self.color_field(
            &self.props.palette_picker,
            FieldLook::Plus,
            "Nouvelle couleur… (sélecteur)",
            false,
            window,
            cx,
        ));

        // Colors used in the document that are not in the palette yet.
        let used: Vec<Color> =
            self.ed.doc.used_colors().into_iter().filter(|c| !swatches.iter().any(|s| palette::same(*s, *c))).collect();

        let mut col = v_flex()
            .flex_none()
            .pb_1()
            .child(
                h_flex()
                    .h(px(44.0))
                    .pl_4()
                    .pr_2()
                    .items_center()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::accent())
                                    .child("Palette"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::text_faint())
                                    .child(format!("{}", swatches.len() + self.ed.doc.palette_gradients.len())),
                            ),
                    )
                    .child(
                        h_flex().gap_1().child(eyedropper_button(self.ed.tool == Tool::Eyedropper, cx)).child(
                            div()
                                .id("palette-menu")
                                .size(px(30.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .text_sm()
                                .when(open, |d| d.bg(theme::accent_soft()))
                                .hover(|d| d.bg(theme::hover()))
                                .tooltip(|window, cx| Tooltip::new("Préréglages, importer, exporter").build(window, cx))
                                .on_click(cx.listener(|v, _, _, cx| {
                                    v.props.palette_menu_open = !v.props.palette_menu_open;
                                    cx.notify();
                                }))
                                .child("⋯"),
                        ),
                    ),
            )
            .child(div().id("palette-grid").px_4().max_h(px(4.0 * (SWATCH + 6.0))).overflow_y_scroll().child(grid));

        if !used.is_empty() {
            let mut row = h_flex().flex_wrap().gap_1();
            for (i, c) in used.iter().copied().enumerate() {
                let tip: SharedString =
                    format!("{} — clic : remplissage · ⇧ clic : contour · ⌥ clic : ajouter à la palette", c.to_hex())
                        .into();
                row = row.child(
                    swatch(("doc-color", i), c, is(c, fill), is(c, stroke))
                        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                        .on_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
                            let m = ev.modifiers();
                            if m.alt {
                                v.palette_add_all(vec![c], cx);
                            } else {
                                v.apply_swatch(c, m.shift, cx);
                            }
                        }))
                        .on_aux_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
                            if ev.is_right_click() {
                                v.apply_swatch(c, true, cx);
                            }
                        })),
                );
            }
            col = col
                .child(
                    h_flex()
                        .px_4()
                        .pt_2()
                        .pb_1()
                        .justify_between()
                        .items_center()
                        .child(div().text_xs().text_color(theme::text_dim()).child("Couleurs du document"))
                        .child(
                            div()
                                .id("doc-colors-add")
                                .text_xs()
                                .text_color(theme::accent())
                                .hover(|d| d.underline())
                                .on_click(cx.listener(move |v, _, _, cx| {
                                    v.palette_add_all(used.clone(), cx);
                                }))
                                .child("Tout ajouter"),
                        ),
                )
                .child(div().id("doc-colors").px_4().max_h(px(2.0 * (SWATCH + 4.0))).overflow_y_scroll().child(row));
        }

        if open {
            let mut presets = h_flex().flex_wrap().gap_1();
            for (i, (name, _)) in PRESETS.iter().enumerate() {
                let current = palette::preset(name).is_some_and(|p| {
                    p.len() == swatches.len() && p.iter().zip(&swatches).all(|(a, b)| palette::same(*a, *b))
                });
                presets = presets.child(chip(
                    ("preset", i),
                    name,
                    current,
                    cx.listener(move |v, _, _, cx| {
                        if let Some(colors) = palette::preset(name) {
                            v.ed.edit(|ed| ed.doc.palette = colors);
                            v.after_edit(cx);
                        }
                    }),
                ));
            }
            col = col
                .child(
                    h_flex()
                        .px_4()
                        .pt_2()
                        .pb_1()
                        .gap(px(6.0))
                        .items_center()
                        .child(div().text_xs().text_color(theme::text_dim()).child("Préréglages"))
                        .child(crate::panels::info("info-presets", "Remplacent la palette (⌘Z pour revenir).")),
                )
                .child(div().px_4().child(presets))
                .child(
                    h_flex()
                        .px_4()
                        .pt_2()
                        .gap_2()
                        .child(small_button(
                            "palette-add",
                            "Ajouter la sélection",
                            cx.listener(|v, _, _, cx| v.palette_add_current(cx)),
                        ))
                        .child(small_button(
                            "palette-import",
                            "Importer…",
                            cx.listener(|v, _, _, cx| v.import_palette_dialog(cx)),
                        ))
                        .child(small_button(
                            "palette-export",
                            "Exporter .gpl…",
                            cx.listener(|v, _, _, cx| v.export_palette(cx)),
                        ))
                        .child(small_button(
                            "palette-clear",
                            "Vider",
                            cx.listener(|v, _, _, cx| {
                                v.ed.edit(|ed| ed.doc.palette.clear());
                                v.after_edit(cx);
                            }),
                        )),
                );
        }
        col
    }
}

/// Toggles the eyedropper tool (I).
fn eyedropper_button(active: bool, cx: &mut Context<EditorView>) -> impl IntoElement {
    div()
        .id("palette-eyedropper")
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(active, |d| d.bg(theme::accent_soft()))
        .hover(|d| d.bg(theme::hover()))
        .tooltip(|window, cx| {
            Tooltip::new("Pipette (I) — clic : remplissage · ⇧ : contour · ⌥ : ajouter à la palette").build(window, cx)
        })
        .on_click(cx.listener(move |v, _, window, cx| {
            v.ed.set_tool(if active { Tool::Select } else { Tool::Eyedropper });
            window.focus(&v.canvas_focus, cx);
            cx.notify();
        }))
        .child(crate::icons::icon(
            crate::icons::Icon::Eyedropper,
            16.0,
            if active { theme::accent() } else { theme::text() },
        ))
}

/// A color square; `fill` / `stroke` mark it as the current colors.
fn swatch(id: impl Into<ElementId>, c: Color, fill: bool, stroke: bool) -> Stateful<Div> {
    let ring = if fill {
        Some(theme::text())
    } else if stroke {
        Some(theme::accent())
    } else {
        None
    };
    // Round swatches, as in Android's color pickers. The current color gets
    // a ring separated from the color by a gap.
    div().id(id.into()).size(px(SWATCH)).flex().flex_none().items_center().justify_center().rounded_full().map(|d| {
        match ring {
            Some(r) => {
                d.border_2().border_color(r).child(div().size(px(SWATCH - 8.0)).rounded_full().bg(theme::to_hsla(c)))
            }
            None => d
                .bg(theme::to_hsla(c))
                .border_1()
                .border_color(theme::border())
                .hover(|d| d.border_color(theme::text_dim())),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::PaletteEdit;
    use crate::editor::Editor;
    use desy_core::{Color, Document, FontLibrary};

    fn editor() -> Editor {
        let mut doc = Document::new("t", 100.0, 100.0);
        doc.palette = vec![Color::BLACK, Color::WHITE];
        Editor::new(doc, None, FontLibrary::empty())
    }

    fn hex(h: &str) -> Color {
        Color::from_hex(h).unwrap()
    }

    #[test]
    fn new_color_drag_is_one_swatch_and_one_undo_step() {
        let mut ed = editor();
        let mut pe = PaletteEdit::default();
        for h in ["#100000", "#200000", "#300000"] {
            pe.pick(&mut ed, hex(h));
        }
        pe.finish(&mut ed);
        assert_eq!(ed.doc.palette, vec![Color::BLACK, Color::WHITE, hex("#300000")]);
        assert!(!pe.is_active());
        ed.undo();
        assert_eq!(ed.doc.palette, vec![Color::BLACK, Color::WHITE]);
    }

    #[test]
    fn editing_a_swatch_replaces_it() {
        let mut ed = editor();
        let mut pe = PaletteEdit::default();
        pe.edit_swatch(1);
        pe.pick(&mut ed, hex("#ff0000"));
        pe.pick(&mut ed, hex("#00ff00"));
        pe.finish(&mut ed);
        assert_eq!(ed.doc.palette, vec![Color::BLACK, hex("#00ff00")]);
        ed.undo();
        assert_eq!(ed.doc.palette, vec![Color::BLACK, Color::WHITE]);
    }

    #[test]
    fn new_duplicate_is_dropped_and_closing_without_change_does_nothing() {
        let mut ed = editor();
        let mut pe = PaletteEdit::default();
        pe.pick(&mut ed, Color::WHITE);
        pe.finish(&mut ed);
        assert_eq!(ed.doc.palette.len(), 2);
        assert!(ed.status.is_some());
        pe.edit_swatch(0);
        pe.finish(&mut ed);
        assert_eq!(ed.doc.palette, vec![Color::BLACK, Color::WHITE]);
        assert!(!ed.history.can_undo());
    }
}
