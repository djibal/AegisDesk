// App commands registered here are subject to capabilities (deny-by-default).
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "core_status",
            "validate_site",
            "list_apps",
            "add_app",
            "update_network",
            "remove_app",
            "launch_app",
            "close_app",
            "wipe_session",
            "score_host",
            "advise_permission",
            "record_permission",
            "fabric_stats",
            "workspace_predictions",
            "governor_snapshot",
            "set_app_limits",
            "set_governor",
        ]),
    ))
    .expect("failed to run tauri-build");
}
