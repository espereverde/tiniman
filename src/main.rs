#![cfg_attr(debug_assertions, allow(dead_code, unused_imports))]

use bevy::prelude::*;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::diagnostic::LogDiagnosticsPlugin;
use bevy_rapier2d::{render::RapierDebugRenderPlugin, prelude::*};
use bevy_inspector_egui::quick::WorldInspectorPlugin;

mod asset_manager;
mod level;
mod animation;
mod characters;

use asset_manager::AssetManagerPluging;
use animation::AnimationPlugin;
use characters::CharacterPlugin;


const BACKGROUNDCOLOR: [u8;3] = [0, 0, 0];

#[derive(Clone, Eq, PartialEq, Debug, Hash, Default, States)]
enum GameStates {
    #[default]
    AssetLoading,
    Running,
}

fn main() {
    App::new()
        .add_state::<GameStates>()
        .insert_resource(ClearColor(Color::rgb_u8(BACKGROUNDCOLOR[0], BACKGROUNDCOLOR[1], BACKGROUNDCOLOR[2])))

        .add_plugins(
            DefaultPlugins.set(ImagePlugin::default_nearest()), // prevents blurry sprites
        )
        
        //.init_collection::<CharacterAssets>()
        //.add_plugins(WorldInspectorPlugin::new())
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(6.0))
        //.add_plugins(RapierDebugRenderPlugin::default())
        //.add_plugins(FrameTimeDiagnosticsPlugin::default())
        //.add_plugins(LogDiagnosticsPlugin::default())
        .add_plugins(AssetManagerPluging)
        .add_plugins(AnimationPlugin)
        .add_plugins(CharacterPlugin)
        .add_plugins(level::LevelPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());

}
