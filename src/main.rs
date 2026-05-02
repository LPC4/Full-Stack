#![allow(clippy::all)]
#![warn(rust_2018_idioms)]
#![windows_subsystem = "windows"]

#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

/// Compilation pipeline: HLL -> Lexer -> Parser -> Compiler -> IR
#[cfg(not(target_arch = "wasm32"))]
fn compile_hll_file(input_file: &str, output_file: &str) -> Result<(), Box<dyn std::error::Error>> {
    use full_stack::high_level_language::compilation_pipeline::CompilationPipeline;

    log::info!("Reading HLL file: {input_file}");
    let content = fs::read_to_string(input_file)?;

    log::info!("Starting compilation pipeline");
    let pipeline = CompilationPipeline::new();
    let result = pipeline.compile(&content)?;

    log::info!("Compilation successful!");
    log::info!("  Declarations: {}", result.ast.declarations.len());
    log::info!("  Diagnostics: {}", result.diagnostics.len());

    let ir_text = format!("{}", result.ir_program);

    // Create output directory if needed
    if let Some(parent) = Path::new(output_file).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    log::info!("Writing IR to file: {output_file}");
    fs::write(output_file, ir_text.clone())?;
    log::info!("Successfully wrote IR output to {output_file}");

    log::info!("=== GENERATED IR ===\n{ir_text}");

    Ok(())
}

// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    eprintln!("\n=== Starting Compilation Pipeline ===\n");

    // Compilation pipeline
    let input_file = "programs/debug/debug.hll";
    let output_file = "out/IR.txt";

    match compile_hll_file(input_file, output_file) {
        Ok(()) => {
            log::info!("Pipeline completed successfully!");
            eprintln!("\n=== Pipeline completed successfully! ===\n");
        }
        Err(e) => {
            log::error!("Pipeline failed: {e}");
            eprintln!("\n!!! Pipeline failed: {e} !!!\n");
        }
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 860.0])
            .with_min_inner_size([900.0, 680.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(&include_bytes!("../assets/icon/icon.png")[..])
                    .expect("Failed to load icon"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Full-Stack",
        native_options,
        Box::new(|cc| Ok(Box::new(full_stack::FullStackApp::new(cc)))),
    )
}

// When compiling to web using trunk:
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
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
                Box::new(|cc| Ok(Box::new(full_stack::FullStackApp::new(cc)))),
            )
            .await;

        // Remove the loading text and spinner:
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
