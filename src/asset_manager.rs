use bevy::prelude::*;
use bevy_asset_loader::prelude::*;
use bevy_asset_loader::asset_collection::AssetCollection;

use crate::GameStates;

pub struct AssetManagerPluging;

impl Plugin for AssetManagerPluging{
    fn build(&self, app: &mut App) {
        app
            .add_loading_state(
                LoadingState::new(GameStates::AssetLoading).continue_to_state(GameStates::Running),
            )
            .add_dynamic_collection_to_loading_state::<_, StandardDynamicAssetCollection>(
                GameStates::AssetLoading,
                "player.assets.ron",
            )
            .add_collection_to_loading_state::<_, PlayerAssets>(GameStates::AssetLoading);
    }
}


#[derive(AssetCollection, Resource, Reflect)]
pub struct PlayerAssets {

    #[asset(key = "tinyman.texture")]
    tinyman_texture: Handle<TextureAtlas>
}