mod commands;
mod encapsulation;
mod error;
mod fabric;
mod governor;
mod runtime;
mod workspace;
mod security;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::core_status,
            commands::validate_site,
            commands::list_apps,
            commands::add_app,
            commands::update_network,
            commands::remove_app,
            commands::launch_app,
            commands::close_app,
            commands::wipe_session,
            commands::score_host,
            commands::advise_permission,
            commands::record_permission,
            commands::fabric_stats,
            commands::workspace_predictions,
            commands::governor_snapshot,
            commands::set_app_limits,
            commands::set_governor,
        ])
        .setup(|app| {
            security::harden_windows(app)?;
            let registry = encapsulation::Registry::load(app.handle())?;
            app.manage(registry);
            app.manage(fabric::Fabric::start(app.handle()));
            let dir = app.path().app_data_dir()?;
            app.manage(workspace::Workspace::load(dir.join("workspace.json")));
            app.manage(runtime::Runtime::new());
            runtime::start(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running AegisDesk");
}
