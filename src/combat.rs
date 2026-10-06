//! Health, damage, contact damage and the player's knockback and invulnerability.

use bevy::prelude::*;

use crate::GameSet;
use crate::characters::{CharVelocity, CharacterSet, OnGround, Player};
use crate::perception::Awareness;
use crate::weapons::{BulletDir, FiredBy};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Team>()
            .register_type::<Health>()
            .register_type::<ContactDamage>()
            .add_message::<Damage>()
            .add_message::<Defeated>()
            .add_systems(PreUpdate, (tick_invulnerability, tick_stun))
            .add_systems(Update, knockback.in_set(CharacterSet::ApplyMovement).before(crate::characters::apply_movement))
            .add_systems(Update, (contact_damage, apply_damage).chain().in_set(GameSet::Resolve))
            .add_systems(PostUpdate, blink_invulnerable);
    }
}

/// Whose side an entity is on. Bullets hurt the other team only.
#[derive(Component, Reflect, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[reflect(Component, Default)]
pub enum Team {
    #[default]
    Player,
    Enemy,
}

#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default)]
pub struct Health {
    pub hp: i32,
    pub max: i32,
}

/// Damage dealt to the other team on touch.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default)]
pub struct ContactDamage(pub i32);

/// Area that bullets and touches hit, centered on the entity.
#[derive(Component, Clone, Debug, Default)]
pub struct Hurtbox {
    pub half_size: Vec2,
}

impl Hurtbox {
    pub fn overlaps(&self, center: Vec2, point: Vec2, radius: f32) -> bool {
        let d = (point - center).abs();
        d.x <= self.half_size.x + radius && d.y <= self.half_size.y + radius
    }

    pub fn overlaps_box(&self, center: Vec2, other: &Hurtbox, other_center: Vec2) -> bool {
        let d = (other_center - center).abs();
        d.x < self.half_size.x + other.half_size.x && d.y < self.half_size.y + other.half_size.y
    }
}

/// Takes no damage; bullets bounce off (a closed sentry).
#[derive(Component)]
pub struct Shielded;

/// Takes no damage for a while after being hit.
#[derive(Component)]
pub struct Invulnerable(Timer);

/// Pushed back and out of control for a moment after being hit.
#[derive(Component)]
pub struct Stunned {
    timer: Timer,
    direction: f32,
}

/// Damage being dealt this frame.
#[derive(Message)]
pub struct Damage {
    pub target: Entity,
    pub amount: i32,
    /// Where the hit came from.
    pub from: Vec2,
    /// The bullet that hit, if any.
    pub bullet: Option<Entity>,
}

/// Something ran out of health this frame (already despawned).
#[derive(Message)]
pub struct Defeated {
    pub entity: Entity,
    pub team: Team,
    pub position: Vec2,
}

/// Damage multiplier against enemies that haven't noticed the player.
const AMBUSH_MULTIPLIER: i32 = 2;

fn contact_damage(
    attackers: Query<(&GlobalTransform, &Hurtbox, &ContactDamage, &Team)>,
    targets: Query<(Entity, &GlobalTransform, &Hurtbox, &Team), Without<Invulnerable>>,
    mut damage: MessageWriter<Damage>,
) {
    for (target, target_transform, target_box, target_team) in &targets {
        let target_center = target_transform.translation().truncate();
        for (transform, hurtbox, contact, team) in &attackers {
            let center = transform.translation().truncate();
            if team != target_team && hurtbox.overlaps_box(center, target_box, target_center) {
                damage.write(Damage { target, amount: contact.0, from: center, bullet: None });
                break;
            }
        }
    }
}

#[allow(clippy::type_complexity)]
fn apply_damage(
    mut commands: Commands,
    mut damage: MessageReader<Damage>,
    mut targets: Query<(
        &mut Health,
        &GlobalTransform,
        &Team,
        Has<Shielded>,
        Has<Invulnerable>,
        Option<&mut Awareness>,
    )>,
    mut bullets: Query<(&mut BulletDir, &FiredBy)>,
    positions: Query<&GlobalTransform>,
    mut defeated: MessageWriter<Defeated>,
) {
    for hit in damage.read() {
        let Ok((mut health, transform, team, shielded, invulnerable, awareness)) = targets.get_mut(hit.target) else {
            continue;
        };
        if health.hp <= 0 {
            continue;
        }
        let position = transform.translation().truncate();
        if shielded {
            // Mega Man's "tink": the bullet bounces back diagonally and stays harmless.
            if let Some(bullet) = hit.bullet
                && let Ok((mut dir, _)) = bullets.get_mut(bullet)
            {
                dir.0 = Vec2::new(-dir.0.x.signum(), 1.0).normalize();
            }
            continue;
        }
        if let Some(bullet) = hit.bullet {
            commands.entity(bullet).try_despawn();
        }
        if invulnerable {
            continue;
        }

        let mut amount = hit.amount;
        if let Some(mut awareness) = awareness {
            if awareness.is_unaware() {
                amount *= AMBUSH_MULTIPLIER;
            }
            // Getting shot is the surest way to notice someone: look where it came from.
            let shooter = hit
                .bullet
                .and_then(|bullet| bullets.get(bullet).ok())
                .and_then(|(_, fired_by)| positions.get(fired_by.0).ok())
                .map_or(hit.from, |shooter| shooter.translation().truncate());
            awareness.alert_at(shooter);
        }
        health.hp -= amount;

        if health.hp <= 0 {
            defeated.write(Defeated { entity: hit.target, team: *team, position });
            commands.entity(hit.target).despawn();
        } else if *team == Team::Player {
            let direction = if position.x < hit.from.x { -1.0 } else { 1.0 };
            commands.entity(hit.target).insert((
                Invulnerable(Timer::from_seconds(1.2, TimerMode::Once)),
                Stunned { timer: Timer::from_seconds(0.3, TimerMode::Once), direction },
            ));
        }
    }
}

fn knockback(time: Res<Time>, mut stunned: Query<(&Stunned, &mut CharVelocity, Has<OnGround>), With<Player>>) {
    for (stun, mut velocity, on_ground) in &mut stunned {
        velocity.x = stun.direction * 24.0;
        velocity.y = if on_ground { -1.0 } else { (velocity.y - 450.0 * time.delta_secs()).max(-210.0) };
    }
}

fn tick_stun(mut commands: Commands, time: Res<Time>, mut stunned: Query<(Entity, &mut Stunned)>) {
    for (entity, mut stun) in &mut stunned {
        if stun.timer.tick(time.delta()).is_finished() {
            commands.entity(entity).remove::<Stunned>();
        }
    }
}

fn tick_invulnerability(mut commands: Commands, time: Res<Time>, mut query: Query<(Entity, &mut Invulnerable, &mut Sprite)>) {
    for (entity, mut invulnerable, mut sprite) in &mut query {
        if invulnerable.0.tick(time.delta()).is_finished() {
            sprite.color.set_alpha(1.0);
            commands.entity(entity).remove::<Invulnerable>();
        }
    }
}

fn blink_invulnerable(mut query: Query<(&Invulnerable, &mut Sprite)>) {
    for (invulnerable, mut sprite) in &mut query {
        let visible = (invulnerable.0.elapsed_secs() * 20.0) as u32 % 2 == 0;
        sprite.color.set_alpha(if visible { 1.0 } else { 0.2 });
    }
}
