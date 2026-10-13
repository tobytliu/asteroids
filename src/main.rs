use bevy::prelude::*;

fn main() {
    App::new()
        .app_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
            }),
            ..default()
        }
                ))
}
