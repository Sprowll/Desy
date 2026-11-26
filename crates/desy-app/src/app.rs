//! Root view: the project list (home) or an open editor.

use crate::canvas::{Item, paint_items};
use crate::projects::{self, ProjectEntry};
use crate::theme;
use crate::view::{EditorEvent, EditorView, primary_button};
use desy_core::{Document, FontLibrary};
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::*;
use kurbo::{Affine, Rect};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

const PRESETS: [(&str, f64, f64); 5] = [
    ("Logo", 1000.0, 1000.0),
    ("Icône", 512.0, 512.0),
    ("Mascotte", 2000.0, 2000.0),
    ("Bannière", 1920.0, 1080.0),
    ("Carte de visite", 1050.0, 600.0),
];

struct Thumb {
    entry: ProjectEntry,
    items: Vec<Item>,
}

pub struct DesyApp {
    fonts: FontLibrary,
    editor: Option<Entity<EditorView>>,
    projects: Vec<Rc<Thumb>>,
    name: Entity<InputState>,
    preset: usize,
    error: Option<String>,
    /// Newer release found on GitHub, announced on the home screen.
    update: Option<crate::update::Release>,
    _subs: Vec<Subscription>,
}

impl DesyApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Nom du projet"));
        let sub = cx.subscribe_in(&name, window, |this: &mut Self, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.create_project(window, cx);
            }
        });
        let mut app = Self {
            fonts: FontLibrary::new(),
            editor: None,
            projects: Vec::new(),
            name,
            preset: 0,
            error: None,
            update: None,
            _subs: vec![sub],
        };
        app.refresh();
        let check = cx.background_spawn(async { crate::update::check() });
        cx.spawn(async move |this, cx| {
            if let Some(release) = check.await {
                let _ = this.update(cx, |app, cx| {
                    app.update = Some(release);
                    cx.notify();
                });
            }
        })
        .detach();
        window.set_window_title("Desy");
        #[cfg(feature = "dev-screenshot")]
        if std::env::var("DESY_SCENE").as_deref() == Ok("editor") {
            let doc = if std::env::var("DESY_SELECT").as_deref() == Ok("mirror") {
                crate::devshot::mirror_document()
            } else {
                crate::devshot::demo_document()
            };
            app.open(doc, None, window, cx);
            if let Some(e) = &app.editor {
                e.update(cx, |v, cx| {
                    crate::devshot::prepare_editor(v);
                    crate::devshot::open_fill_picker(v, cx);
                });
            }
        }
        app
    }

    fn refresh(&mut self) {
        let fonts = &self.fonts;
        self.projects = projects::list_projects()
            .into_iter()
            .map(|entry| {
                let items = entry
                    .doc
                    .visible_effective()
                    .map(|o| Item::new(&o, Rc::new(o.render(fonts)), &entry.doc))
                    .collect();
                Rc::new(Thumb { entry, items })
            })
            .collect();
    }

    fn create_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let raw = self.name.read(cx).value().trim().to_string();
        let (preset, w, h) = PRESETS[self.preset];
        let name = if raw.is_empty() { format!("{preset} sans titre") } else { raw };
        let doc = Document::new(name.clone(), w, h);
        let path = projects::new_project_path(&name);
        if let Err(e) = projects::save(&path, &doc) {
            self.error = Some(format!("Impossible de créer le projet : {e}"));
            cx.notify();
            return;
        }
        self.name.update(cx, |s, cx| s.set_value("", window, cx));
        self.open(doc, Some(path), window, cx);
    }

    fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match projects::load(&path) {
            Ok(doc) => {
                projects::remember(&path);
                self.open(doc, Some(path), window, cx);
            }
            Err(e) => {
                self.error = Some(format!("Impossible d'ouvrir {} : {e}", path.display()));
                cx.notify();
            }
        }
    }

    fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Ouvrir un projet Desy".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.into_iter().next()
            {
                let _ = this.update_in(cx, |app, window, cx| app.open_file(p, window, cx));
            }
        })
        .detach();
    }

    fn open(&mut self, doc: Document, path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let fonts = self.fonts.clone();
        let editor = cx.new(|cx| EditorView::new(doc, path, fonts, window, cx));
        let sub = cx.subscribe_in(&editor, window, |this: &mut Self, editor, ev: &EditorEvent, window, cx| match ev {
            EditorEvent::Close => {
                let e = editor.read(cx);
                if let Some(path) = e.ed.file.clone()
                    && e.ed.is_dirty()
                {
                    // Never lose work: save on the way out.
                    let _ = projects::save(&path, &e.ed.doc);
                }
                this.fonts = editor.read(cx).ed.fonts.clone();
                this.editor = None;
                this.refresh();
                window.set_window_title("Desy");
                cx.notify();
            }
        });
        self._subs.push(sub);
        self.editor = Some(editor);
        self.error = None;
        cx.notify();
    }

    fn home(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut presets = h_flex().gap_2().flex_wrap();
        for (i, (name, w, h)) in PRESETS.iter().enumerate() {
            let active = i == self.preset;
            presets = presets.child(
                v_flex()
                    .id(("preset", i))
                    .min_w(px(120.0))
                    .px_4()
                    .py_3()
                    .gap(px(2.0))
                    .rounded(px(theme::RADIUS_MENU))
                    .border_1()
                    .when(active, |d| d.bg(theme::active()).border_color(theme::active()))
                    .when(!active, |d| d.border_color(theme::border()).hover(|d| d.bg(theme::hover())))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if active { theme::on_active() } else { theme::text() })
                            .child(*name),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(if active { theme::on_active().opacity(0.75) } else { theme::text_faint() })
                            .child(format!("{w} × {h}")),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preset = i;
                        cx.notify();
                    })),
            );
        }

        let mut grid = div().flex().flex_wrap().gap_4();
        for (i, thumb) in self.projects.iter().enumerate() {
            let t = thumb.clone();
            let path = thumb.entry.path.clone();
            let forget = thumb.entry.path.clone();
            grid = grid.child(
                v_flex()
                    .id(("project", i))
                    .w(px(208.0))
                    .gap_1()
                    .p(px(6.0))
                    .pb_3()
                    .rounded(px(theme::RADIUS_PANEL))
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::border())
                    .hover(|d| d.bg(theme::panel_alt()))
                    .child(
                        div()
                            .w_full()
                            .h(px(150.0))
                            .mb_1()
                            .rounded(px(18.0))
                            .overflow_hidden()
                            .bg(theme::canvas_bg())
                            .child(
                                canvas(|_, _, _| (), move |bounds, _, window, _| paint_thumb(&t, bounds, window))
                                    .size_full(),
                            ),
                    )
                    .child(
                        div()
                            .px_2()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(thumb.entry.doc.name.clone()),
                    )
                    .child(
                        h_flex()
                            .px_2()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::text_faint())
                                    .child(format!("{} × {}", thumb.entry.doc.width, thumb.entry.doc.height)),
                            )
                            .child(
                                div()
                                    .id(("forget", i))
                                    .px_2()
                                    .rounded_full()
                                    .text_xs()
                                    .text_color(theme::text_faint())
                                    .hover(|d| d.text_color(theme::danger()).bg(theme::hover()))
                                    .child("Retirer")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        projects::forget(&forget);
                                        this.refresh();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| this.open_file(path.clone(), window, cx))),
            );
        }

        div().id("home").size_full().overflow_y_scroll().bg(theme::canvas_bg()).text_color(theme::text()).child(
            v_flex()
                .max_w(px(1100.0))
                .mx_auto()
                .px_8()
                .py_10()
                .gap_6()
                .child(
                    h_flex()
                        .items_center()
                        .gap_3()
                        .child(
                            // The logo's black matches the background: an outline keeps the square readable.
                            div()
                                .size(px(56.0))
                                .flex_none()
                                .rounded(px(16.0))
                                .border_1()
                                .border_color(theme::border())
                                .overflow_hidden()
                                .child(img(logo()).size_full().rounded(px(15.0))),
                        )
                        .child(
                            v_flex().child(div().text_3xl().font_weight(FontWeight::BOLD).child("Desy")).child(
                                div()
                                    .text_sm()
                                    .text_color(theme::text_dim())
                                    .child("Logos, mascottes et formes vectorielles"),
                            ),
                        ),
                )
                .children(self.update.clone().map(|release| {
                    h_flex()
                        .gap_3()
                        .items_center()
                        .justify_between()
                        .px_6()
                        .py_3()
                        .rounded(px(theme::RADIUS_PANEL))
                        .bg(theme::panel())
                        .border_1()
                        .border_color(theme::accent())
                        .child(div().text_sm().child(format!(
                            "Desy {} est disponible (version actuelle : {}).",
                            release.version,
                            env!("CARGO_PKG_VERSION")
                        )))
                        .child(primary_button("update", "Télécharger", move |_, _, cx| cx.open_url(&release.url)))
                }))
                .child(
                    v_flex()
                        .gap_4()
                        .p_6()
                        .rounded(px(28.0))
                        .bg(theme::panel())
                        .border_1()
                        .border_color(theme::border())
                        .child(
                            div()
                                .text_lg()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::accent())
                                .child("Nouveau projet"),
                        )
                        .child(presets)
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(div().w(px(320.0)).child(Input::new(&self.name)))
                                .child(primary_button(
                                    "create",
                                    "Créer",
                                    cx.listener(|this, _, window, cx| this.create_project(window, cx)),
                                ))
                                .child(crate::view::bar_button(
                                    "open",
                                    "Ouvrir un fichier…",
                                    cx.listener(|this, _, window, cx| this.open_dialog(window, cx)),
                                )),
                        )
                        .children(self.error.clone().map(|e| div().text_sm().text_color(theme::danger()).child(e))),
                )
                .child(div().pt_2().text_lg().font_weight(FontWeight::SEMIBOLD).text_color(theme::accent()).child(
                    if self.projects.is_empty() {
                        "Aucun projet pour l'instant".to_string()
                    } else {
                        format!("Projets — {}", projects::projects_dir().display())
                    },
                ))
                .child(grid),
        )
    }
}

/// The DY. logo (assets/logo.jpg), decoded once.
fn logo() -> Arc<Image> {
    thread_local! {
        static LOGO: Arc<Image> =
            Arc::new(Image::from_bytes(ImageFormat::Jpeg, include_bytes!("../../../assets/logo.jpg").to_vec()));
    }
    LOGO.with(Arc::clone)
}

fn paint_thumb(t: &Thumb, bounds: Bounds<Pixels>, window: &mut Window) {
    let doc = &t.entry.doc;
    let bw = f32::from(bounds.size.width) as f64;
    let bh = f32::from(bounds.size.height) as f64;
    let s = ((bw - 16.0) / doc.width).min((bh - 16.0) / doc.height);
    let ox = f32::from(bounds.origin.x) as f64 + (bw - doc.width * s) * 0.5;
    let oy = f32::from(bounds.origin.y) as f64 + (bh - doc.height * s) * 0.5;
    let view = Affine::translate((ox, oy)) * Affine::scale(s);
    let ab = view.transform_rect_bbox(Rect::new(0.0, 0.0, doc.width, doc.height));
    let bg = doc.background.map(theme::to_hsla).unwrap_or(hsla(0.0, 0.0, 0.9, 1.0));
    window.paint_quad(fill(
        Bounds::from_corners(point(px(ab.x0 as f32), px(ab.y0 as f32)), point(px(ab.x1 as f32), px(ab.y1 as f32))),
        bg,
    ));
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        paint_items(&t.items, view, window);
    });
}

impl Render for DesyApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.editor {
            Some(e) => div().size_full().child(e.clone()).into_any_element(),
            None => self.home(cx).into_any_element(),
        }
    }
}
