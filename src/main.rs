use bevy::prelude::*;
use bevy_rapier2d::{render::RapierDebugRenderPlugin, prelude::*};


mod level;

const BACKGROUNDCOLOR: [u8;3] = [0, 0, 0];

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::rgb_u8(BACKGROUNDCOLOR[0], BACKGROUNDCOLOR[1], BACKGROUNDCOLOR[2])))

        .add_plugins(
            DefaultPlugins.set(ImagePlugin::default_nearest()), // prevents blurry sprites
        )
        .add_plugins(RapierPhysicsPlugin::<NoUserData>::pixels_per_meter(8.0))
        .add_plugins(RapierDebugRenderPlugin::default())
        .add_plugins(level::LevelPlugin)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());

}

