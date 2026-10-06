use bevy::prelude::*;
use bevy::scene::{ScenePatch, ScenePatchInstance};
use bevy_rapier2d::prelude::*;
use bsn_blueprints::{Recipe, ReflectRecipe};

use crate::GameSet;
use crate::combat::{Damage, Hurtbox, Team};

pub struct WeaponsPlugin;

impl Plugin for WeaponsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<WeaponRecipe>()
            .register_type::<Bullet>()
            .add_systems(PreUpdate, tick_weapon_cooldowns)
            .add_systems(
                Update,
                (move_bullets, bullet_hits)
                    .chain()
                    .in_set(GameSet::Act)
                    .after(crate::characters::CharacterSet::ApplyMovement),
            );
    }
}

/// A weapon, as written in a blueprint. `bullet` refers to a bullet blueprint.
#[derive(Reflect, Default)]
#[reflect(Recipe, Default)]
pub struct WeaponRecipe {
    pub bullet: Handle<ScenePatch>,
    /// Muzzle position relative to the shooter, for a shooter facing right.
    pub offset: Vec2,
    /// Seconds between shots.
    pub cooldown: f32,
    /// Bullets of this weapon allowed at once (0: unlimited).
    pub max_on_screen: u32,
    /// Radius in which enemies hear the shot.
    pub noise: f32,
}

impl Recipe for WeaponRecipe {
    fn bundle(&self) -> impl Bundle + use<> {
        let mut cooldown = Timer::from_seconds(self.cooldown, TimerMode::Once);
        cooldown.finish();
        Weapon {
            bullet: self.bullet.clone(),
            offset: self.offset,
            cooldown,
            max_on_screen: self.max_on_screen,
            on_screen: 0,
            noise: self.noise,
        }
    }
}

#[derive(Component)]
pub struct Weapon {
    pub bullet: Handle<ScenePatch>,
    pub offset: Vec2,
    pub cooldown: Timer,
    pub max_on_screen: u32,
    on_screen: u32,
    pub noise: f32,
}

impl Weapon {
    /// Fires one bullet from `origin` (the shooter's position) towards `direction`.
    /// Returns whether it fired.
    pub fn try_shoot(&mut self, commands: &mut Commands, owner: Entity, origin: Vec3, direction: Vec2) -> bool {
        let cooling_down = !self.cooldown.duration().is_zero() && !self.cooldown.is_finished();
        if cooling_down || (self.max_on_screen > 0 && self.on_screen >= self.max_on_screen) {
            return false;
        }
        self.cooldown.reset();
        self.on_screen += 1;
        let side = if direction.x < 0.0 { -1.0 } else { 1.0 };
        let muzzle = origin + Vec3::new(self.offset.x * side, self.offset.y, 1.5);
        commands.spawn((
            ScenePatchInstance(self.bullet.clone()),
            Transform::from_translation(muzzle),
            BulletDir(direction.normalize_or(Vec2::X)),
            FiredBy(owner),
        ));
        true
    }
}

fn tick_weapon_cooldowns(time: Res<Time>, mut weapons: Query<&mut Weapon>) {
    for mut weapon in &mut weapons {
        weapon.cooldown.tick(time.delta());
    }
}

#[derive(Debug, Component, Reflect, Clone, Default)]
#[reflect(Component, Default)]
#[require(BulletAge)]
pub struct Bullet {
    pub speed: f32,
    pub damage: i32,
    pub lifetime: f32,
    pub radius: f32,
}

#[derive(Component, Default)]
pub struct BulletAge(f32);

#[derive(Component)]
pub struct BulletDir(pub Vec2);

#[derive(Component)]
pub struct FiredBy(pub Entity);

/// Deflected bullets fly away harmlessly.
#[derive(Component)]
pub struct Deflected;

fn move_bullets(
    mut commands: Commands,
    time: Res<Time>,
    rapier: ReadRapierContext,
    mut bullets: Query<(Entity, &Bullet, &BulletDir, &mut BulletAge, &mut Transform, &FiredBy, Has<Deflected>)>,
    mut weapons: Query<&mut Weapon>,
) -> Result {
    let rapier = rapier.single()?;
    for (entity, bullet, dir, mut age, mut transform, fired_by, deflected) in &mut bullets {
        age.0 += time.delta_secs();
        let step = dir.0 * bullet.speed * time.delta_secs();
        let start = transform.translation.truncate();
        let hit_wall = rapier
            .cast_ray(start, dir.0, step.length(), true, QueryFilter::only_fixed())
            .is_some();
        transform.translation += step.extend(0.0);
        if hit_wall || age.0 > bullet.lifetime {
            commands.entity(entity).despawn();
            // A deflected bullet already gave its slot back when it hit.
            if !deflected && let Ok(mut weapon) = weapons.get_mut(fired_by.0) {
                weapon.on_screen = weapon.on_screen.saturating_sub(1);
            }
        }
    }
    Ok(())
}

fn bullet_hits(
    mut commands: Commands,
    bullets: Query<(Entity, &Bullet, &BulletDir, &Transform, &FiredBy), Without<Deflected>>,
    shooters: Query<&Team>,
    targets: Query<(Entity, &GlobalTransform, &Hurtbox, &Team)>,
    mut weapons: Query<&mut Weapon>,
    mut damage: MessageWriter<Damage>,
) {
    for (bullet_entity, bullet, dir, transform, fired_by) in &bullets {
        let Ok(shooter_team) = shooters.get(fired_by.0) else { continue };
        let position = transform.translation.truncate();
        let hit = targets.iter().find(|(target, target_transform, hurtbox, team)| {
            *target != fired_by.0
                && *team != shooter_team
                && hurtbox.overlaps(target_transform.translation().truncate(), position, bullet.radius)
        });
        if let Some((target, ..)) = hit {
            damage.write(Damage {
                target,
                amount: bullet.damage,
                from: position - dir.0,
                bullet: Some(bullet_entity),
            });
            // The bullet is spent unless the target deflects it (see `combat::apply_damage`).
            commands.entity(bullet_entity).insert(Deflected);
            if let Ok(mut weapon) = weapons.get_mut(fired_by.0) {
                weapon.on_screen = weapon.on_screen.saturating_sub(1);
            }
        }
    }
}
