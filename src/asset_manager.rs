use bevy::prelude::*;
use bevy_asset_loader::prelude::*;
use bevy_asset_loader::asset_collection::AssetCollection;

use crate::GameStates;

pub struct AssetManagerPluging;

impl Plugin for AssetManagerPluging{
    fn build(&self, app: &mut App) {
        app
        .add_loading_state(
            LoadingState::new(GameStates::AssetLoading).continue_to_state(GameStates::Running)
            .with_dynamic_assets_file::<StandardDynamicAssetCollection>("player.assets.ron")
            .load_collection::<TextureAssets>(),
        );
    }
}


#[derive(AssetCollection, Resource, Reflect)]
pub struct TextureAssets {

    #[asset(key = "tinyman.texture")]
    tinyman: Handle<TextureAtlas>,

    #[asset(key = "bullet.texture")]
    bullet: Handle<TextureAtlas>
}