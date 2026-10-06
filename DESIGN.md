# Tinyman: design notes

Classic Mega Man controls and look, scaled down so that **a whole stage fits on one 256×240
screen**. The player sees every enemy, platform and route from the first frame. That changes the
game: a Mega Man stage is about reacting to what scrolls in, while a single-screen stage is about
reading the room and planning a route through it.

## The problem: everyone can see everyone

If enemies react to the player as soon as the stage starts, a one-screen stage turns into five
enemies converging on the spawn point. So enemies start **unaware** and have to *notice* the player
first (`src/perception.rs`):

| Sense   | Works when                                                        | Effect                            |
|---------|-------------------------------------------------------------------|-----------------------------------|
| Sight   | in range, inside the cone in front of the enemy, not behind walls | fills a suspicion meter           |
| Touch   | within a few pixels, any facing                                   | alert at once                     |
| Hearing | inside `noise radius × hearing`                                   | suspicious, turns towards the sound |
| Alarm   | an ally within its `alarm_radius` became alert                    | alert at once                     |
| Damage  | being shot                                                        | alert, looks at the shooter       |

States: `Unaware → Suspicious → Alert → Searching → Unaware`. The meter fills faster the closer
the player is, so a far glimpse gives you time to back off, while walking into a cone up close
doesn't.

Every value lives in `assets/blueprints/enemies.bp.ron` and is rolled per spawn (`Range(..)`,
`OneOf(..)`), so two walkers never see exactly alike. Press **F1** in game to see cones, ranges,
suspicion meters and last known positions.

### Noises the player makes

| Action                | Radius | Notes                                         |
|-----------------------|--------|-----------------------------------------------|
| Buster shot           | 64 px  | `WeaponRecipe.noise`: weapons can be quieter or louder |
| Landing from a big fall | 32 px | dropping from a ledge behind an enemy gives you away |
| Slide                 | 20 px  |                                               |
| An enemy going down   | 40 px  | `Enemy.death_noise`: kills are heard          |

Walking is silent, so moving is free and attacking is the risky part.

### Current roster

- **Walker**: patrols, chases when alert and **shouts** (alerts allies within 96 px).
  A **guard** variant stands still and turns around every few seconds.
- **Sentry** (Met): invulnerable under its helmet while unaware (bullets bounce off), peeks
  when suspicious, opens to fire a spread when alert. It's only vulnerable while open.
- **Bat**: asleep on its perch and blind until something wakes it, but it hears well. It flies
  at the player, then gives up and goes back to sleep.

Shooting an enemy that hasn't noticed you does **double damage** (`AMBUSH_MULTIPLIER`).

## Game loop: Room Rush (with a bit of infiltration)

Each LDtk level is a room. Reach the **exit** as fast as you can (`src/room.rs`).

- **The exit is locked while any enemy is alert.** Kill them, or lose them, to open it. Being
  seen costs time, so the fastest route is usually the unseen one, and speed and stealth push the
  same way. Going loud stays a real option, at the price of time and the Ghost badge.
- **Badges** at the exit, independent so each can be chased on its own:
  - **Fast**: under the room's `Par_time` (a level field in LDtk)
  - **Ghost**: no enemy became alert
  - **Clean**: no damage taken
- **R** restarts instantly; **Z/X** skips the results; **1-9** jump to a room (for testing).
- Noise rings show how far the player's noises carry, so hearing is learned by playing.
- Ambush kills that finish an enemy in one hit don't count as alerts: a silent takedown. Killing
  is still heard by others, though (`death_noise`).

### Draft rooms

| Room | Teaches | Notes |
|------|---------|-------|
| Patrol | cones, the locked exit | cross the bridge while the walker below looks away |
| Sleepers | hearing | shooting the guard wakes the bats (its alarm reaches them) |
| Chain | alarms | three walkers in shouting range; one sighting alerts all |
| Gauntlet | mix | the original test room |

Par times are first guesses; tune them in LDtk once the rooms feel right.

### Next steps

1. The **decoy**: a weapon whose bullet makes a loud noise where it lands, to pull enemies off a
   route. The first infiltration tool, standing in for a boss reward.
2. Per-placement **facing** for enemies (an LDtk field), so rooms can aim cones deliberately.
3. Keeping best times and badges between sessions.
4. A fortress structure: rooms → boss → tool, with tools opening faster routes in earlier rooms.

Other loops considered: a living fortress where alarms spread between rooms and enemies remember
you, and a roguelite tower built from random room blueprints (`OneOf`/`Maybe` make this cheap).

## Open questions

- Should alert enemies ever calm down fully, or stay "on edge" (faster to notice) for the rest of
  the stage?
- Is ambush damage enough reward for stealth, or should unaware enemies drop more?
- Should the player get an indicator of their own noise (a ring on shots)? It would teach the
  hearing rules without text.
- Does the 3-bullet limit still feel right when most fights are against one alerted enemy at a
  time?

## Where things are

| | |
|---|---|
| `assets/blueprints/*.bp.ron` | Player, enemies and bullets ([bsn_blueprints](libs/bsn_blueprints)) |
| `assets/maps/fire_world.ldtk` | Stage. An LDtk entity spawns the blueprint with its name in lowercase (`Walker` → `walker`); `Start_position` spawns the player |
| `assets/sprites/enemies.png` | Placeholder enemy sprites, 16×16, same grid as `tinyman.png` |
| `src/perception.rs` | Senses and awareness states |
| `src/enemies.rs` | Brains: what each enemy does in each state |
| `src/combat.rs` | Health, damage, contact damage, knockback |
| `src/characters.rs` | Player movement (Mega Man state machine) |
| `src/weapons.rs` | Weapons and bullets |
| `src/level.rs` | LDtk loading, walls, camera |
| `src/room.rs` | Room Rush loop: exits, lockdown, timer, badges, restart |
| `assets/blueprints/props.bp.ron` | Exit door |
