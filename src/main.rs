#![warn(rust_2018_idioms)]
#![expect(
    clippy::too_many_lines,
    reason = "IDE shell panels are long by nature; lint cleanup proceeds incrementally"
)]
#![windows_subsystem = "windows"]

mod app;
mod ui;
mod web_reference;

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 860.0])
            .with_min_inner_size([900.0, 680.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Full-Stack",
        native_options,
        Box::new(|cc| Ok(Box::new(app::FullStackApp::new(cc)))),
    )
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Info: debug-level compiler tracing floods the browser console (and jank)
    // on every in-browser build.
    eframe::WebLogger::init(log::LevelFilter::Info).ok();

    let mut web_options = eframe::WebOptions::default();
    // Chromium on Windows ignores explicit GPU preferences and logs a warning.
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut web_options.wgpu_options.wgpu_setup
    {
        setup.power_preference = eframe::wgpu::PowerPreference::None;
    }

    wasm_bindgen_futures::spawn_local(async {
        let startup_reference = web_reference::load().await;
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(move |cc| {
                    Ok(Box::new(app::FullStackApp::new_with_reference(
                        cc,
                        startup_reference,
                    )))
                }),
            )
            .await;

        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}
