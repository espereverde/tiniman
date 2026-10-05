#![cfg_attr(debug_assertions, allow(dead_code, unused_imports))]

use bevy::{prelude::*};
use bevy_rapier2d::prelude::*;

use crate::asset_manager::TextureAssets;
use crate::animation::FrameAnimation;

use crate::characters::{CharacterSet, CharVelocity};

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app


            //.add_systems(PreUpdate, (update_bullet_lifetime))
            //.add_systems(Update, (
             //   execute_orders
            //).in_set(CharacterSet::PrepareMovement))
            //.add_systems( Update, (apply_movement).chain().in_set(CharacterSet::ApplyMovement))

            //.add_systems(PostUpdate, ().in_set(CharacterSet::UpdateMode))
            //.add_systems(PostUpdate, ().in_set(CharacterSet::SyncSprite))
        ;
    }
}

#[derive(Component)]
pub struct Weapon{
    pub bullets: BulletBundle,
    pub offset: Vec2
}

impl Weapon{
    pub fn try_shoot(
        &self,
        mut command: &mut Commands,
        base_transform: &Transform,
        facing: f32
    ) -> bool{
        // If can shoot
        let mut noozle_transform = base_transform.clone();
        noozle_transform.translation += self.offset.extend(1.5);
        error!("z {:?}", base_transform.translation.z);
        command.spawn(self.bullets.clone().facing(facing))
            .insert(TransformBundle::from_transform(noozle_transform))
        ;
        return true;
        // If can not shoot
        // return false
    }
}

#[derive(Debug, Component, Clone)]
pub struct Bullet{
    speed: f32,
    damage: i32, 
    lifetime: f32
}

pub const BUSTER_BULLET: Bullet = Bullet{
    speed: 90.0,
    damage: 1,
    lifetime: 2.0
};

#[derive(Bundle, Clone)]
pub struct BulletBundle{
    bullet: Bullet,
    ssheet: SpriteSheetBundle,
    body: RigidBody,
    collision: Collider,
    controller: KinematicCharacterController,
    velocity: CharVelocity,
}

impl BulletBundle{
    pub fn new(name: &str, assets: &Res<TextureAssets>) -> Self{
        let atlas_field_name  = format!("{name}");
        let atlas_field = assets.field(&atlas_field_name).unwrap();
        let atlas_handle = atlas_field.downcast_ref::<Handle<TextureAtlas>>().unwrap();
        
        let ssheet = SpriteSheetBundle{
            texture_atlas: atlas_handle.clone(),
            //sprite: TextureAtlasSprite{anchor:bevy::sprite::Anchor::BottomCenter, ..default()},
            ..default()
        };

        Self { 
            ssheet,
            body: RigidBody::KinematicPositionBased, 
            //collision: Collider::capsule(Vec2::NEG_Y, Vec2::Y, 3.0),
            collision: Collider::cuboid(1.0, 1.0), 
            controller: KinematicCharacterController{
                offset:CharacterLength::Absolute(0.2),
                
                 ..default()},
            velocity: CharVelocity::default(),
            bullet: BUSTER_BULLET
        }
    }

    pub fn facing(mut self, face_dir: f32) -> Self{
        self.velocity.x = self.bullet.speed * face_dir;
        self
    }
}