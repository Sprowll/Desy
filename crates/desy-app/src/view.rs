//! The editor window: layout, keyboard actions, file operations.

use crate::actions::*;
use crate::editor::{Align, Drag, Editor, Tool};
use crate::export::Format;
use crate::icons::{Icon, icon};
use crate::motion::{self, Enter, From};
use crate::panels::Props;
use crate::tool_frames::FRAME_W;
use crate::{projects, theme};
use desy_core::{BoolOp, Document, FontLibrary};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::*;
use gpui_kit::*;
use kurbo::Vec2;
use std::path::PathBuf;

pub enum EditorEvent {
    /// Go back to the project list.
    Close,
}

impl EventEmitter<EditorEvent> for EditorView {}

pub struct EditorView {
    pub ed: Editor,
    pub canvas_focus: FocusHandle,
    pub props: Props,
    pub synced_revision: u64,
    pub layers_expanded: Vec<u64>,
    pub renaming_layer: Option<u64>,
    /// Where a drag in the layers panel would drop.
    pub drop_hint: Option<crate::editor::DropSpot>,
    pub notice: Option<(String, bool)>,
    /// Tool rail group whose flyout menu is open.
    pub tool_menu: Option<usize>,
    /// Tool shown by each rail group (the last one used).
    pub group_tool: [Tool; Tool::GROUPS.len()],
    /// Tool option frames the user folded.
    pub collapsed_frames: Vec<crate::tool_frames::Frame>,
    /// Right-click menu position (window coordinates), when open.
    pub context_menu: Option<Point<Pixels>>,
    /// Export format dropdown open.
    pub export_menu: bool,
    /// Quick edit bar above the selection (Canva-like).
    pub quick: crate::quick_bar::QuickBar,
    /// Number field being scrubbed (pressed and dragged sideways).
    pub scrub: Option<crate::scrub::Scrub>,
}

impl EditorView {
    pub fn new(
        doc: Document,
        file: Option<PathBuf>,
        fonts: FontLibrary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let ed = Editor::new(doc, file, fonts);
        let canvas_focus = cx.focus_handle();
        let props = Props::new(window, cx);
        let quick = crate::quick_bar::QuickBar::new(window, cx);
        let layers_expanded = ed.doc.layers.iter().map(|l| l.id).collect();
        window.focus(&canvas_focus, cx);
        let v = Self {
            ed,
            canvas_focus,
            props,
            synced_revision: u64::MAX,
            layers_expanded,
            renaming_layer: None,
            drop_hint: None,
            notice: None,
            tool_menu: None,
            group_tool: Tool::GROUPS.map(|g| g[0]),
            collapsed_frames: Vec::new(),
            context_menu: None,
            export_menu: false,
            quick,
            scrub: None,
        };
        v.sync_title(window);
        v
    }

    /// Call after any editor change triggered from the UI.
    pub fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.ed.prune_cache();
        if let Some(msg) = self.ed.status.take() {
            self.notice = Some((msg, false));
        }
        cx.notify();
    }

    pub fn sync_title(&self, window: &mut Window) {
        window.set_window_title(&self.ed.title());
    }

    pub fn notify_user(&mut self, msg: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        self.notice = Some((msg.into(), error));
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_secs(4)).await;
            let _ = this.update(cx, |v, cx| {
                v.notice = None;
                cx.notify();
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ files

    pub fn save(&mut self, cx: &mut Context<Self>) {
        match self.ed.file.clone() {
            Some(path) => self.write_to(path, cx),
            None => self.save_as(cx),
        }
    }

    pub fn save_as(&mut self, cx: &mut Context<Self>) {
        let dir = projects::projects_dir();
        let _ = std::fs::create_dir_all(&dir);
        let name = format!("{}.{}", self.ed.doc.name, projects::EXTENSION);
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut path))) = rx.await {
                if path.extension().is_none() {
                    path.set_extension(projects::EXTENSION);
                }
                let _ = this.update(cx, |v, cx| v.write_to(path, cx));
            }
        })
        .detach();
    }

    fn write_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        match projects::save(&path, &self.ed.doc) {
            Ok(()) => {
                self.ed.file = Some(path);
                self.ed.saved_doc = self.ed.doc.clone();
                self.notify_user("Projet enregistré", false, cx);
            }
            Err(e) => self.notify_user(format!("Échec de l'enregistrement : {e}"), true, cx),
        }
    }

    pub fn export(&mut self, format: Format, cx: &mut Context<Self>) {
        let svg = desy_core::svg::export(&self.ed.doc, &self.ed.fonts);
        let dir = self
            .ed
            .file
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(projects::projects_dir);
        let name = format!("{}.{}", self.ed.doc.name, format.extension());
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut path))) = rx.await {
                if path.extension().is_none() {
                    path.set_extension(format.extension());
                }
                let target = path.clone();
                let res = cx
                    .background_spawn(async move {
                        let bytes = crate::export::encode(&svg, format)?;
                        std::fs::write(&target, bytes)?;
                        anyhow::Ok(())
                    })
                    .await;
                let _ = this.update(cx, |v, cx| match res {
                    Ok(()) => v.notify_user(format!("Exporté : {}", path.display()), false, cx),
                    Err(e) => v.notify_user(format!("Échec de l'export : {e:#}"), true, cx),
                });
            }
        })
        .detach();
    }

    // ------------------------------------------------------------ images

    pub fn import_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Importer des images".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |v, cx| v.import_images(paths, None, cx));
            }
        })
        .detach();
    }

    /// Import image files; `at` centers them on a window position (drop).
    pub fn import_images(&mut self, paths: Vec<PathBuf>, at: Option<kurbo::Point>, cx: &mut Context<Self>) {
        let mut errors = Vec::new();
        let mut placed = Vec::new();
        for (i, path) in paths.into_iter().enumerate() {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Image".into());
            let loaded = std::fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|d| crate::images::load_asset(name.clone(), d));
            match loaded {
                Ok(asset) => {
                    let id = self.ed.add_image(asset);
                    if let (Some(p), Some(b)) = (at, self.ed.bounds_of(id)) {
                        let target = self.ed.to_doc(p) + Vec2::new(i as f64 * 20.0, i as f64 * 20.0);
                        self.ed.doc.translate_objects(&[id], target - b.center());
                    }
                    placed.push(id);
                }
                Err(e) => errors.push(format!("{name} : {e:#}")),
            }
        }
        if placed.len() > 1 {
            self.ed.selected = placed;
        }
        if !errors.is_empty() {
            self.notify_user(errors.join(" · "), true, cx);
        }
        self.after_edit(cx);
    }

    // ------------------------------------------------------------ fonts

    pub fn install_google_font(&mut self, family: String, cx: &mut Context<Self>) {
        self.notify_user(format!("Téléchargement de « {family} »…"), false, cx);
        let fam = family.clone();
        let task = cx.background_spawn(async move { crate::fonts::download_google_family(&fam) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            let _ = this.update(cx, |v, cx| match res {
                Ok(files) => {
                    for f in &files {
                        v.ed.fonts.load_data(f.clone());
                    }
                    let _ = cx.text_system().add_fonts(files.into_iter().map(std::borrow::Cow::Owned).collect());
                    v.ed.invalidate_text();
                    v.apply_font_family(family.clone(), cx);
                    v.notify_user(format!("« {family} » installée"), false, cx);
                }
                Err(e) => v.notify_user(format!("{e:#}"), true, cx),
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ actions

    fn register_actions(&self, el: Div, cx: &mut Context<Self>) -> Div {
        macro_rules! on {
            ($el:expr, $action:ty, |$v:ident, $cx:ident| $body:expr) => {
                $el.on_action(cx.listener(|$v: &mut Self, _: &$action, _window, $cx| {
                    $body;
                    $v.after_edit($cx);
                }))
            };
        }
        let el = on!(el, Undo, |v, cx| v.ed.undo());
        let el = on!(el, Redo, |v, cx| v.ed.redo());
        let el = on!(el, Save, |v, cx| v.save(cx));
        let el = on!(el, SaveAs, |v, cx| v.save_as(cx));
        let el = on!(el, ExportSvg, |v, cx| v.export(Format::Svg, cx));
        let el = el.on_action(cx.listener(|v: &mut Self, _: &ImportImage, window, cx| v.import_dialog(window, cx)));
        let el = on!(el, CloseProject, |v, cx| cx.emit(EditorEvent::Close));
        let el = on!(el, ToggleSnap, |v, cx| v.ed.snapping = !v.ed.snapping);
        let el = on!(el, ZoomIn, |v, cx| v.ed.zoom_center(1.25));
        let el = on!(el, ZoomOut, |v, cx| v.ed.zoom_center(0.8));
        let el = on!(el, ZoomFit, |v, cx| v.ed.fit());
        let el = on!(el, Zoom100, |v, cx| {
            let f = 1.0 / v.ed.zoom;
            v.ed.zoom_center(f)
        });
        let el = on!(el, Union, |v, cx| v.ed.boolean(BoolOp::Union));
        let el = on!(el, Subtract, |v, cx| v.ed.boolean(BoolOp::Subtract));
        let el = on!(el, Intersect, |v, cx| v.ed.boolean(BoolOp::Intersect));

        on!(el, Exclude, |v, cx| v.ed.boolean(BoolOp::Exclude))
    }

    fn register_canvas_actions(&self, el: Div, cx: &mut Context<Self>) -> Div {
        macro_rules! on {
            ($el:expr, $action:ty, |$v:ident, $cx:ident| $body:expr) => {
                $el.on_action(cx.listener(|$v: &mut Self, _: &$action, _window, $cx| {
                    $body;
                    $v.after_edit($cx);
                }))
            };
        }
        let el = on!(el, Delete, |v, cx| v.ed.delete());
        let el = on!(el, Duplicate, |v, cx| v.ed.duplicate());
        let el = on!(el, Copy, |v, cx| v.ed.copy());
        let el = on!(el, Cut, |v, cx| v.ed.cut());
        let el = on!(el, Paste, |v, cx| v.ed.paste());
        let el = on!(el, SelectAll, |v, cx| v.ed.select_all());
        let el = on!(el, Cancel, |v, cx| {
            // Escape closes the menu or the eyedropper first.
            if v.context_menu.take().is_some() || std::mem::take(&mut v.export_menu) || v.cancel_picker_eyedrop(cx) {
            } else if v.ed.pen.is_some() {
                v.ed.finish_pen()
            } else {
                v.ed.clear_selection()
            }
        });
        let el = on!(el, Confirm, |v, cx| v.ed.finish_pen());
        let el = on!(el, BringForward, |v, cx| v.ed.reorder(1));
        let el = on!(el, SendBackward, |v, cx| v.ed.reorder(-1));
        let el = on!(el, BringToFront, |v, cx| v.ed.reorder(i64::MAX));
        let el = on!(el, SendToBack, |v, cx| v.ed.reorder(i64::MIN));
        let el = on!(el, NudgeLeft, |v, cx| v.ed.nudge(Vec2::new(-1.0, 0.0)));
        let el = on!(el, NudgeRight, |v, cx| v.ed.nudge(Vec2::new(1.0, 0.0)));
        let el = on!(el, NudgeUp, |v, cx| v.ed.nudge(Vec2::new(0.0, -1.0)));
        let el = on!(el, NudgeDown, |v, cx| v.ed.nudge(Vec2::new(0.0, 1.0)));
        let el = on!(el, NudgeLeftBig, |v, cx| v.ed.nudge(Vec2::new(-10.0, 0.0)));
        let el = on!(el, NudgeRightBig, |v, cx| v.ed.nudge(Vec2::new(10.0, 0.0)));
        let el = on!(el, NudgeUpBig, |v, cx| v.ed.nudge(Vec2::new(0.0, -10.0)));
        let el = on!(el, NudgeDownBig, |v, cx| v.ed.nudge(Vec2::new(0.0, 10.0)));
        let el = on!(el, ToolSelect, |v, cx| v.ed.set_tool(Tool::Select));
        let el = on!(el, ToolDirect, |v, cx| v.ed.set_tool(Tool::Direct));
        let el = on!(el, ToolPen, |v, cx| v.ed.set_tool(Tool::Pen));
        let el = on!(el, ToolPencil, |v, cx| v.ed.set_tool(Tool::Pencil));
        let el = on!(el, ToolLine, |v, cx| v.ed.set_tool(Tool::Line));
        let el = on!(el, ToolRect, |v, cx| v.ed.set_tool(Tool::Rect));
        let el = on!(el, ToolEllipse, |v, cx| v.ed.set_tool(Tool::Ellipse));
        let el = on!(el, ToolPolygon, |v, cx| v.ed.set_tool(Tool::Polygon));
        let el = on!(el, ToolStar, |v, cx| v.ed.set_tool(Tool::Star));
        let el = on!(el, ToolWave, |v, cx| v.ed.set_tool(Tool::Wave));
        let el = on!(el, ToolBlob, |v, cx| v.ed.set_tool(Tool::Blob));
        let el = on!(el, ToolText, |v, cx| v.ed.set_tool(Tool::Text));
        let el = on!(el, ToolWidth, |v, cx| v.ed.set_tool(Tool::Width));

        let el = on!(el, FlipHorizontal, |v, cx| v.ed.flip(true));
        let el = on!(el, FlipVertical, |v, cx| v.ed.flip(false));
        let el = on!(el, ToolHand, |v, cx| v.ed.set_tool(Tool::Hand));
        let el = on!(el, ToolEyedropper, |v, cx| v.ed.set_tool(Tool::Eyedropper));
        on!(el, ToolMirror, |v, cx| v.ed.set_tool(Tool::Mirror))
    }

    // ------------------------------------------------------------ layout
    //
    // The canvas fills the window; the tools, the title, the actions and the
    // panels float over it as Material 3 surfaces.

    fn top_bar(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .absolute()
            .top(px(M))
            .left(px(M))
            .right(px(M))
            .h(px(BAR_H))
            .gap_2()
            .items_center()
            .child(self.title_pill(cx))
            .child(div().flex_1())
            .child(self.actions_pill(cx))
    }

    fn title_pill(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let dirty = self.ed.is_dirty();
        let can_undo = self.ed.history.can_undo();
        let can_redo = self.ed.history.can_redo();
        floating()
            .h(px(BAR_H))
            .flex_none()
            .rounded_full()
            .pl(px(6.0))
            .pr(px(6.0))
            .gap_2()
            .child(round_icon_button(
                "home",
                Icon::Back,
                "Retour aux projets (⌘W)",
                false,
                cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Close)),
            ))
            .child(
                v_flex()
                    .max_w(px(220.0))
                    .child(
                        h_flex()
                            .gap(px(6.0))
                            .items_center()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .truncate()
                                    .child(self.ed.doc.name.clone()),
                            )
                            .when(dirty, |d| {
                                d.child(div().size(px(6.0)).flex_none().rounded_full().bg(theme::accent()))
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::text_faint())
                            .child(format!("{} × {} px", self.ed.doc.width, self.ed.doc.height)),
                    ),
            )
            .child(vdivider())
            .child(round_icon_button(
                "undo",
                Icon::Undo,
                "Annuler (⌘Z)",
                !can_undo,
                cx.listener(|v, _, _, cx| {
                    v.ed.undo();
                    v.after_edit(cx)
                }),
            ))
            .child(round_icon_button(
                "redo",
                Icon::Redo,
                "Rétablir (⇧⌘Z)",
                !can_redo,
                cx.listener(|v, _, _, cx| {
                    v.ed.redo();
                    v.after_edit(cx)
                }),
            ))
    }

    /// View controls at the bottom right, next to the side panel: snapping,
    /// zoom and fit.
    fn view_pill(&self, cx: &mut Context<Self>) -> Div {
        floating()
            .absolute()
            .right(px(M + PANEL_W + 8.0))
            .bottom(px(M))
            .h(px(BAR_H))
            .rounded_full()
            .px(px(6.0))
            .gap_1()
            .child(round_toggle_button(
                "snap",
                Icon::Magnet,
                if self.ed.snapping { "Magnétisme activé (⌘;)" } else { "Magnétisme désactivé (⌘;)" },
                self.ed.snapping,
                cx.listener(|v, _, _, cx| {
                    v.ed.snapping = !v.ed.snapping;
                    cx.notify()
                }),
            ))
            .child(vdivider())
            .child(round_icon_button(
                "zoom-out",
                Icon::Minus,
                "Zoom arrière (⌘−)",
                false,
                cx.listener(|v, _, _, cx| {
                    v.ed.zoom_center(0.8);
                    cx.notify()
                }),
            ))
            .child(
                div()
                    .id("zoom-100")
                    .w(px(56.0))
                    .h(px(32.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .hover(|d| d.bg(theme::hover()))
                    .tooltip(|window, cx| Tooltip::new("Taille réelle (⌘1)").build(window, cx))
                    .on_click(cx.listener(|v, _, _, cx| {
                        let f = 1.0 / v.ed.zoom;
                        v.ed.zoom_center(f);
                        cx.notify()
                    }))
                    .child(format!("{:.0} %", self.ed.zoom * 100.0)),
            )
            .child(round_icon_button(
                "zoom-in",
                Icon::Plus,
                "Zoom avant (⌘+)",
                false,
                cx.listener(|v, _, _, cx| {
                    v.ed.zoom_center(1.25);
                    cx.notify()
                }),
            ))
            .child(round_icon_button(
                "fit",
                Icon::Fit,
                "Ajuster à la fenêtre (⌘0)",
                false,
                cx.listener(|v, _, _, cx| {
                    v.ed.fit();
                    cx.notify()
                }),
            ))
    }

    fn actions_pill(&self, cx: &mut Context<Self>) -> impl IntoElement {
        floating()
            .h(px(BAR_H))
            .flex_none()
            .rounded_full()
            .px(px(6.0))
            .gap_1()
            .child(
                h_flex()
                    .id("export")
                    .h(px(40.0))
                    .pl(px(12.0))
                    .pr(px(8.0))
                    .gap_2()
                    .items_center()
                    .rounded_full()
                    .bg(if self.export_menu { theme::active() } else { theme::panel_alt() })
                    .hover(|d| d.bg(theme::panel_high()))
                    .on_click(cx.listener(|v, _, _, cx| {
                        v.export_menu = !v.export_menu;
                        v.tool_menu = None;
                        v.context_menu = None;
                        cx.notify();
                    }))
                    .child(icon(Icon::Export, 18.0, theme::text()))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Exporter"))
                    .child(icon(
                        if self.export_menu { Icon::ChevronUp } else { Icon::ChevronDown },
                        16.0,
                        theme::text_dim(),
                    )),
            )
            .child(primary_button("save", "Enregistrer", cx.listener(|v, _, _, cx| v.save(cx))))
    }

    /// Dropdown under the export button: one entry per format.
    fn export_dropdown(&self, cx: &mut Context<Self>) -> AnyElement {
        let size = format!("{} × {} px", self.ed.doc.width.round(), self.ed.doc.height.round());
        let mut menu = v_flex()
            .id("export-menu")
            .occlude()
            .absolute()
            .top(px(M + BAR_H + 6.0))
            .right(px(M))
            .w(px(232.0))
            .p(px(6.0))
            .rounded(px(theme::RADIUS_MENU))
            .bg(theme::panel_high())
            .border_1()
            .border_color(theme::border())
            .shadow_lg();
        for (i, format) in Format::ALL.into_iter().enumerate() {
            let detail: SharedString = match format {
                Format::Svg => "⌘E".into(),
                _ => size.clone().into(),
            };
            menu = menu.child(
                Enter::row(i).wrap(
                    ("export-row", i),
                    h_flex()
                        .id(("export-item", i))
                        .h(px(40.0))
                        .px_3()
                        .gap_3()
                        .items_center()
                        .rounded(px(10.0))
                        .hover(|d| d.bg(theme::hover()))
                        .active(|d| d.bg(theme::pressed()))
                        .on_click(cx.listener(move |v, _, _, cx| {
                            v.export_menu = false;
                            v.export(format, cx);
                        }))
                        .child(icon(
                            if format == Format::Svg { Icon::Pen } else { Icon::Image },
                            18.0,
                            theme::text_dim(),
                        ))
                        .child(div().flex_1().text_sm().font_weight(FontWeight::MEDIUM).child(format.label()))
                        .child(div().text_xs().text_color(theme::text_faint()).child(detail)),
                ),
            );
        }
        let close = |v: &mut EditorView, _: &MouseDownEvent, _: &mut Window, cx: &mut Context<EditorView>| {
            v.export_menu = false;
            cx.notify();
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                div()
                    .id("export-menu-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(MouseButton::Left, cx.listener(close))
                    .on_mouse_down(MouseButton::Right, cx.listener(close)),
            )
            .child(Enter::menu(From::Above).run("export-menu-enter", menu))
            .into_any_element()
    }

    /// Vertical tool rail: one slot per group, the last used tool of the
    /// group is shown; click the active slot (or right-click) for the others.
    fn tool_rail(&self, cx: &mut Context<Self>) -> Div {
        let current = self.ed.tool;
        let hint = self.tool_hint();
        let mut rail = floating()
            .absolute()
            .left(px(M))
            .top(px(PANEL_TOP))
            .w(px(RAIL_W))
            .flex_col()
            .rounded(px(RAIL_W / 2.0))
            .py(px(RAIL_PAD))
            .gap(px(RAIL_GAP));
        for (gi, group) in Tool::GROUPS.iter().enumerate() {
            if RAIL_DIVIDERS.contains(&gi) {
                rail = rail.child(div().w(px(24.0)).h(px(1.0)).my(px(4.0)).flex_none().bg(theme::border()));
            }
            let active = group.contains(&current);
            let tool = if active { current } else { self.group_tool[gi] };
            let open = self.tool_menu == Some(gi);
            let multi = group.len() > 1;
            let fg = if active { theme::on_active() } else { theme::text() };
            let slot = div()
                .id(("tool-slot", gi))
                .relative()
                .size(px(SLOT))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                // Material 3 Expressive: the active item morphs from a
                // circle to a rounded square.
                .when(active, |d| d.rounded(px(theme::RADIUS_ITEM)).bg(theme::active()))
                .when(!active, |d| d.rounded_full().hover(|d| d.bg(theme::hover())))
                .when(open && !active, |d| d.bg(theme::pressed()))
                .when(!open, |d| {
                    d.tooltip(move |window, cx| {
                        let mut tip = tool.label().to_string();
                        if active {
                            // The active tool explains how to use it.
                            tip = format!("{tip}\n{hint}");
                            if multi {
                                tip.push_str("\nClic : les autres outils du groupe");
                            }
                        }
                        Tooltip::new(tip).build(window, cx)
                    })
                })
                .on_click(cx.listener(move |v, _, window, cx| {
                    if v.tool_menu == Some(gi) {
                        v.tool_menu = None;
                    } else if multi && v.ed.tool == tool {
                        v.tool_menu = Some(gi);
                    } else {
                        v.ed.set_tool(tool);
                        v.tool_menu = None;
                    }
                    window.focus(&v.canvas_focus, cx);
                    cx.notify();
                }))
                .on_aux_click(cx.listener(move |v, ev: &ClickEvent, _, cx| {
                    if multi && ev.is_right_click() {
                        v.tool_menu = Some(gi);
                        cx.notify();
                    }
                }))
                .child(icon(tool.icon(), 20.0, fg))
                .when(multi, |d| {
                    let mut c = fg;
                    c.a *= 0.7;
                    d.child(div().absolute().right(px(3.0)).bottom(px(3.0)).child(icon(Icon::Corner, 8.0, c)))
                });
            // The newly active slot morphs from its circle into the square.
            rail = rail.child(if active {
                motion::morph(("tool-slot-on", gi), slot, SLOT / 2.0, theme::RADIUS_ITEM, theme::active())
                    .into_any_element()
            } else {
                slot.into_any_element()
            });
        }
        rail
    }

    /// Flyout listing the tools of a rail group, aligned with its slot.
    fn tool_flyout(&self, gi: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let dividers = RAIL_DIVIDERS.iter().filter(|d| **d <= gi).count() as f32;
        let y = PANEL_TOP + RAIL_PAD + gi as f32 * (SLOT + RAIL_GAP) + dividers * (9.0 + RAIL_GAP);
        let current = self.ed.tool;
        let mut menu = v_flex()
            .id("tool-flyout")
            .occlude()
            .absolute()
            .left(px(M + RAIL_W + 8.0))
            .top(px(y - 6.0))
            .min_w(px(248.0))
            .p(px(6.0))
            .gap(px(2.0))
            .rounded(px(theme::RADIUS_MENU))
            .bg(theme::panel_high())
            .border_1()
            .border_color(theme::border())
            .shadow_lg();
        for (i, &t) in Tool::GROUPS[gi].iter().enumerate() {
            let on = t == current;
            let fg = if on { theme::on_active() } else { theme::text() };
            menu = menu.child(
                Enter::row(i).wrap(
                    ("tool-row", i),
                    h_flex()
                        .id(("tool-item", i))
                        .h(px(44.0))
                        .px_3()
                        .gap_3()
                        .items_center()
                        .rounded(px(theme::RADIUS_ITEM))
                        .when(on, |d| d.bg(theme::active()))
                        .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
                        .child(icon(t.icon(), 20.0, fg))
                        .child(div().flex_1().text_sm().text_color(fg).child(t.name()))
                        .child(
                            div()
                                .min_w(px(22.0))
                                .px_1()
                                .rounded(px(6.0))
                                .flex()
                                .justify_center()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(if on { theme::on_active() } else { theme::text_faint() })
                                .border_1()
                                .border_color(if on { theme::on_active().opacity(0.4) } else { theme::border() })
                                .child(t.key()),
                        )
                        .on_click(cx.listener(move |v, _, window, cx| {
                            v.ed.set_tool(t);
                            v.tool_menu = None;
                            window.focus(&v.canvas_focus, cx);
                            cx.notify();
                        })),
                ),
            );
        }
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                // Click anywhere else to close.
                div()
                    .id("tool-flyout-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|v, _, _, cx| {
                            v.tool_menu = None;
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|v, _, _, cx| {
                            v.tool_menu = None;
                            cx.notify();
                        }),
                    ),
            )
            // Keyed by group: switching flyouts plays again.
            .child(Enter::menu(From::Left).run(("tool-flyout-enter", gi), menu))
    }

    /// Layers card (left, top, width, height in window coordinates): at
    /// the bottom left under the tool rail, beside the rail when the window
    /// is too short.
    pub fn layers_frame(&self, window: &Window) -> (f32, f32, f32, f32) {
        let groups = Tool::GROUPS.len() as f32;
        let rail_h =
            2.0 * RAIL_PAD + groups * SLOT + (groups - 1.0) * RAIL_GAP + RAIL_DIVIDERS.len() as f32 * (9.0 + RAIL_GAP);
        let rail_bottom = PANEL_TOP + rail_h;
        let window_h = f32::from(window.viewport_size().height);
        let below = window_h - rail_bottom - 8.0 - M;
        let (left, h) = if below >= LAYERS_MIN_H {
            (M, below.min(LAYERS_MAX_H))
        } else {
            (M + RAIL_W + 8.0, (window_h - PANEL_TOP - M).min(LAYERS_MAX_H))
        };
        (left, window_h - M - h, LAYERS_W, h)
    }

    fn layers_card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (left, _, _, h) = self.layers_frame(window);
        v_flex()
            .occlude()
            .absolute()
            .left(px(left))
            .bottom(px(M))
            .w(px(LAYERS_W))
            .h(px(h))
            .overflow_hidden()
            .rounded(px(theme::RADIUS_PANEL))
            .bg(theme::panel())
            .border_1()
            .border_color(theme::border())
            .shadow_lg()
            .child(self.layers_panel(cx))
    }

    /// Bottom strip between the layers and the panel: snackbar.
    fn bottom_strip(&self, window: &Window, effects_shown: bool) -> impl IntoElement {
        let left = if effects_shown {
            let (l, w, _) = self.effects_frame(window);
            l + w + M
        } else {
            M + LAYERS_W + M
        };
        // Keep clear of the tool option frames.
        let right = M + PANEL_W + 8.0 + VIEW_PILL_W.max(if self.frames().is_empty() { 0.0 } else { FRAME_W }) + M;
        v_flex().absolute().bottom(px(M)).left(px(left)).right(px(right)).gap_2().items_center().children(
            self.notice.as_ref().map(|(msg, err)| {
                // Each new message rises again.
                let key = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    msg.hash(&mut h);
                    h.finish() as usize
                };
                // Material snackbar: inverse surface (or error container).
                let bar = div()
                    .occlude()
                    .max_w(px(560.0))
                    .px_4()
                    .py(px(10.0))
                    .rounded(px(theme::RADIUS_ITEM))
                    .shadow_lg()
                    .text_sm()
                    .bg(if *err { theme::danger_container() } else { theme::text() })
                    .text_color(if *err { rgb(0xffdad6).into() } else { theme::on_surface_inverse() })
                    .child(msg.clone());
                Enter::card(From::Below).run(("snackbar", key), bar)
            }),
        )
    }

    pub fn apply_align(&mut self, a: Align, cx: &mut Context<Self>) {
        self.ed.align(a);
        self.after_edit(cx);
    }
}

// ---------------------------------------------------------------- geometry

/// Outer margin between floating surfaces and the window edge.
pub const M: f32 = 12.0;
pub const BAR_H: f32 = 52.0;
/// Top of the tool rail and the side panel.
pub const PANEL_TOP: f32 = M + BAR_H + 8.0;
pub const PANEL_W: f32 = 300.0;
/// Approximate width of the bottom view pill (snapping, zoom, fit).
const VIEW_PILL_W: f32 = 360.0;
/// Bottom of the tool option frames, stacked above the view pill.
pub const FRAMES_BOTTOM: f32 = M + BAR_H + 8.0;
const RAIL_W: f32 = 56.0;
/// Layers card, bottom left.
const LAYERS_W: f32 = 280.0;
const LAYERS_MIN_H: f32 = 200.0;
const LAYERS_MAX_H: f32 = 340.0;
const RAIL_PAD: f32 = 8.0;
const RAIL_GAP: f32 = 4.0;
const SLOT: f32 = 40.0;
/// Rail groups preceded by a divider.
const RAIL_DIVIDERS: [usize; 2] = [3, 4];

// ---------------------------------------------------------------- widgets

/// A floating Material surface (container, hairline, elevation shadow) that
/// stops pointer events from reaching the canvas below.
pub fn floating() -> Div {
    h_flex().items_center().occlude().bg(theme::panel()).border_1().border_color(theme::border()).shadow_lg()
}

fn vdivider() -> impl IntoElement {
    div().w(px(1.0)).h(px(24.0)).mx_1().flex_none().bg(theme::border())
}

/// Standard 40 px circular icon button.
pub fn round_icon_button(
    id: &'static str,
    ic: Icon,
    tip: &'static str,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(40.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .when(disabled, |d| d.opacity(0.38))
        .when(!disabled, |d| d.hover(|d| d.bg(theme::hover())).active(|d| d.bg(theme::pressed())).on_click(on_click))
        .child(icon(ic, 20.0, theme::text()))
}

/// 40 px circular icon toggle: filled with the active color when on.
fn round_toggle_button(
    id: &'static str,
    ic: Icon,
    tip: &'static str,
    on: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(40.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(on, |d| d.bg(theme::active()))
        .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
        .active(|d| d.bg(theme::pressed()))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .on_click(on_click)
        .child(icon(ic, 20.0, if on { theme::on_active() } else { theme::text_dim() }))
}

/// Text button (low emphasis), pill shaped.
pub fn bar_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(40.0))
        .px_4()
        .flex()
        .items_center()
        .rounded_full()
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::accent())
        .hover(|d| d.bg(theme::accent_soft()))
        .on_click(on_click)
        .child(label)
}

/// Filled button (primary action), pill shaped.
pub fn primary_button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .h(px(40.0))
        .px(px(20.0))
        .flex()
        .items_center()
        .rounded_full()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::on_accent())
        .bg(theme::accent())
        .hover(|d| d.opacity(0.92))
        .on_click(on_click)
        .child(label)
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.synced_revision != self.ed.revision {
            self.synced_revision = self.ed.revision;
            self.sync_props(window, cx);
        }
        if std::mem::take(&mut self.ed.focus_text) {
            self.sync_props(window, cx);
            self.props.focus_text(window, cx);
        }
        self.sync_title(window);
        if let Some(g) = self.ed.tool.group() {
            self.group_tool[g] = self.ed.tool;
        }
        self.ed.view_insets =
            ((M + RAIL_W + M) as f64, PANEL_TOP as f64, (M + PANEL_W + M) as f64, FRAMES_BOTTOM as f64);

        let root = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme::canvas_bg())
            .text_color(theme::text())
            .key_context("Editor");
        let root = self.register_actions(root, cx);

        let canvas = self.render_canvas(window, cx).into_any_element();
        let middle = div().size_full().flex().child(canvas);
        let middle = self.register_canvas_actions(middle, cx);

        let effects = self.effects_window(window, cx);
        let effects_shown = effects.is_some();
        let flyout = self.tool_menu.map(|gi| self.tool_flyout(gi, cx).into_any_element());
        let sel_bar = self.quick_bar(window, cx);
        if self.ed.selected.is_empty() || self.ed.tool != Tool::Select || !matches!(self.ed.drag, Drag::None) {
            // Leaving the bar forgets its open menu.
            self.quick.menu = None;
        }
        // Opening a project: the surfaces glide in from their edges, one
        // after the other.
        root.child(middle)
            .child(self.bottom_strip(window, effects_shown))
            .child(Enter::card(From::Below).after(200).run("view-pill-enter", self.view_pill(cx)))
            .children(self.tool_frames(cx))
            .child(Enter::card(From::Above).flat().run("top-bar-enter", self.top_bar(cx)))
            .child(Enter::card(From::Left).after(70).run("tool-rail-enter", self.tool_rail(cx)))
            .child(Enter::card(From::Right).flat().after(110).run("right-panel-enter", self.right_panel(window, cx)))
            .child(Enter::card(From::Left).after(150).run("layers-enter", self.layers_card(window, cx)))
            .children(effects)
            .children(sel_bar)
            .children(flyout)
            .children(self.context_menu(window, cx))
            .children(self.export_menu.then(|| self.export_dropdown(cx)))
            .children(self.scrub_tracker(cx))
    }
}
