fn main() {
    let attrs = tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "get_library",
            "get_settings",
            "set_settings",
            "add_games",
            "update_game",
            "remove_game",
            "reorder_games",
            "launch_game",
            "recent_games",
            "island_games",
            "island_feed",
            "island_dismiss_notification",
            "island_clear_notifications",
            "island_mark_notifications_read",
            "island_media_toggle",
            "island_media_next",
            "island_media_previous",
            "set_media_path",
            "sgdb_search",
            "sgdb_list_assets",
            "sgdb_apply_asset",
            "sgdb_autofetch",
            "remove_screenshot",
            "optimize_library",
            "process_media_jobs",
            "toggle_island",
            "island_layout",
            "show_main",
            "get_displays",
            "ffmpeg_available",
            "import_nebula",
            "open_folder",
            "open_data_dir",
            "overlay_stats",
            "list_play_sessions",
            "get_play_session",
            "delete_play_session",
        ]),
    );

    // Release builds request admin so launched games inherit elevation
    // without a UAC prompt on every Play click. Skip in `tauri dev`
    // so cargo can spawn the debug exe from a normal terminal.
    #[cfg(not(debug_assertions))]
    let attrs = {
        let windows = tauri_build::WindowsAttributes::new()
            .app_manifest(include_str!("windows-app.manifest"));
        attrs.windows_attributes(windows)
    };

    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
