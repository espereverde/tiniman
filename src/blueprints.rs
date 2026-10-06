//! Game elements are defined in `assets/blueprints/*.bp.ron` (see `libs/bsn_blueprints`).
//!
//! The files are loaded while in [`GameStates::AssetLoading`]; the game starts once all of them
//! (and every file they refer to) have loaded.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bsn_blueprints::{BlueprintLoadingPlugin, BlueprintPlugin};

use crate::GameStates;

/// Names the set of blueprint files of the game: `characters`, `enemies`, `weapons` and `props`.
pub struct GameBlueprints;

pub struct BlueprintsPlugin;

impl Plugin for BlueprintsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            BlueprintPlugin,
            BlueprintLoadingPlugin::new(GameStates::AssetLoading, GameStates::Running).load_set::<GameBlueprints>([
                ("characters", "blueprints/characters.bp.ron"),
                ("enemies", "blueprints/enemies.bp.ron"),
                ("weapons", "blueprints/weapons.bp.ron"),
                ("props", "blueprints/props.bp.ron"),
            ]),
        ))
        .register_type::<Name>()
        .register_type::<Visibility>()
        .register_type::<SpriteSheet>()
        .init_resource::<AtlasLayouts>()
        .add_observer(build_sheet_sprite);
    }
}

/// A sprite cut from a grid sprite sheet. Blueprints can't build a `TextureAtlasLayout` asset,
/// so they describe the grid and [`build_sheet_sprite`] turns it into a [`Sprite`].
#[derive(Component, Reflect, Default, Clone)]
#[reflect(Component, Default)]
pub struct SpriteSheet {
    pub image: Handle<Image>,
    pub tile_size: UVec2,
    pub columns: u32,
    pub rows: u32,
    /// Space between tiles.
    pub padding: UVec2,
    /// Initial frame.
    pub index: usize,
}

/// One layout per grid shape, shared by every sprite cut from that grid.
#[derive(Resource, Default)]
struct AtlasLayouts(HashMap<(UVec2, u32, u32, UVec2), Handle<TextureAtlasLayout>>);

fn build_sheet_sprite(
    insert: On<Insert, SpriteSheet>,
    mut commands: Commands,
    sheets: Query<&SpriteSheet>,
    mut cache: ResMut<AtlasLayouts>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let Ok(sheet) = sheets.get(insert.entity) else { return };
    let key = (sheet.tile_size, sheet.columns, sheet.rows, sheet.padding);
    let layout = cache
        .0
        .entry(key)
        .or_insert_with(|| {
            layouts.add(TextureAtlasLayout::from_grid(
                sheet.tile_size,
                sheet.columns,
                sheet.rows,
                Some(sheet.padding),
                None,
            ))
        })
        .clone();
    commands.entity(insert.entity).insert(Sprite::from_atlas_image(
        sheet.image.clone(),
        TextureAtlas { layout, index: sheet.index },
    ));
}
