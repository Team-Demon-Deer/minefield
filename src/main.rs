use bevy::prelude::*;
use bevy::window::WindowResolution;

#[cfg(feature = "debug")]
use bevy_inspector_egui::{bevy_egui::EguiPlugin, quick::WorldInspectorPlugin};
use board_plugin::BoardPlugin;

fn main() {
    let mut app = App::new();

    // Window setup
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "minefield".to_string(),
            resolution: WindowResolution::new(700, 600),
            resizable: false,
            ..default()
        }),
        ..default()
    }))
    .add_plugins(BoardPlugin);

    //debug hierarchy inspector
    #[cfg(feature = "debug")]
    app.add_plugins(EguiPlugin::default())
        .add_plugins(WorldInspectorPlugin::new());

    // Startup system (cameras)
    app.add_systems(Startup, camera_setup)
        // Run the app
        .run();
}

fn camera_setup(mut commands: Commands) {
    // 2D orthographic camera
    commands.spawn(Camera2d);
}
