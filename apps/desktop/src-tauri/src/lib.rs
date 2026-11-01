mod commands;
mod workspaces;

use std::sync::Mutex;

use stormix_engine::{RunOptions, Runner};
use stormix_store::Store;
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
            commands::read_environment,
            commands::environments_beside,
            commands::environment_path_beside,
            commands::save_environment,
            commands::run_load_test,
            commands::cached_steps,
            commands::clear_cache,
            commands::list_runs,
            commands::get_run,
            commands::delete_run,
            commands::diff_runs,
            commands::export_run,
            commands::read_text_file,
            commands::write_text_file,
            commands::test_file_to_yaml,
            commands::list_workspaces,
            commands::save_workspace,
            commands::select_workspace,
            commands::delete_workspace,
        ])
        .events(collect_events![
            commands::LoadProgress,
            commands::ImportProgress,
            commands::RunProgress,
        ])
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
        // Checks the latest GitHub release's latest.json; see UpdateBanner.tsx.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            app.manage(Runner::new(RunOptions::default())?);
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            app.manage(Mutex::new(Store::open(&data.join("runs.db"))?));
            app.manage(Mutex::new(workspaces::WorkspaceFile::open(
                &data.join("workspaces.json"),
            )));
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
