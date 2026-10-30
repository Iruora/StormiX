mod commands;

use stormix_engine::{RunOptions, Runner};
use tauri::Manager;
use tauri_specta::{collect_commands, collect_events};

fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::get_app_info,
            commands::example_test_file,
            commands::validate_test_file,
            commands::run_test_file,
            commands::import_collection,
            commands::run_load_test,
        ])
        .events(collect_events![commands::LoadProgress])
}

#[cfg(any(debug_assertions, test))]
fn export_bindings(builder: &tauri_specta::Builder<tauri::Wry>) {
    builder
        .export(
            specta_typescript::Typescript::default()
                .header("// @ts-nocheck\n// Regenerate with `cargo test -p stormix-desktop`."),
            concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts"),
        )
        .expect("failed to export TypeScript bindings");
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,stormix=debug".into()),
        )
        .init();

    let builder = specta_builder();
    #[cfg(debug_assertions)]
    export_bindings(&builder);

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(Runner::new(RunOptions::default())?);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run the Stormix app");
}

#[cfg(test)]
mod tests {
    /// Regenerates `src/bindings.ts`. CI fails if the committed file is stale.
    #[test]
    fn export_bindings() {
        super::export_bindings(&super::specta_builder());
    }
}
