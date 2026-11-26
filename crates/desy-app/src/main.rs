// Release builds on Windows are GUI apps: no console window next to Desy.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod canvas;
mod color_field;
mod context_menu;
#[cfg(feature = "dev-screenshot")]
mod devshot;
mod editor;
mod effects_panel;
mod export;
mod eyedropper;
mod fill_popover;
mod fonts;
mod icons;
mod images;
mod motion;
mod paint;
mod palette_panel;
mod panels;
mod projects;
mod quick_bar;
mod raster;
mod scrub;
mod theme;
mod tool_frames;
mod update;
mod view;

use gpui_kit::*;

fn main() {
    projects::migrate_legacy();
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
        theme::install(cx);
        set_dock_icon();
        actions::bind_keys(cx);
        cx.on_action(|_: &actions::Quit, cx| cx.quit());

        // Make downloaded Google Fonts usable by the UI (font previews).
        let files = fonts::downloaded_font_files();
        if !files.is_empty() {
            let _ = cx.text_system().add_fonts(files.into_iter().map(std::borrow::Cow::Owned).collect());
        }

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(None, size(px(1440.0), px(900.0)), cx)),
            titlebar: Some(TitlebarOptions { title: Some("Desy".into()), ..Default::default() }),
            ..Default::default()
        };
        let (_handle, _root) =
            gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| app::DesyApp::new(window, cx)))
                .expect("failed to open window");
        #[cfg(feature = "dev-screenshot")]
        if let Ok(path) = std::env::var("DESY_SHOT") {
            let _ = _handle.update(cx, |_, window, cx| devshot::schedule(path, window, cx));
        }
        cx.activate(true);
    });
}

/// App icon (DY. logo on the macOS icon grid), shown in the Dock even when
/// running the bare binary. A bundled Desy.app gets it from icon.icns too.
#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::AllocAnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let Some(mtm) = MainThreadMarker::new() else { return };
    let data = NSData::with_bytes(include_bytes!("../../../assets/icon.png"));
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else { return };
    // SAFETY: called on the main thread (checked above) with a valid image.
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}

#[cfg(not(target_os = "macos"))]
fn set_dock_icon() {}
