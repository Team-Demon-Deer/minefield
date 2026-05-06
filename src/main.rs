use std::ops::Range;

use bevy::{
    color::palettes::tailwind::RED_400,
    image::{ImageArrayLayout, ImageLoaderSettings},
    input::mouse::AccumulatedMouseScroll,
    prelude::*,
    sprite_render::{TileData, TilemapChunk, TilemapChunkTileData},
};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;

/// Player movement speed factor.
const PLAYER_SPEED: f32 = 100.;

/// How quickly should the camera snap to the desired location.
const CAMERA_DECAY_RATE: f32 = 2.;
const CAMERA_ZOOM_SPEED: f32 = 0.5;
const CAMERA_ZOOM_RANGE: Range<f32> = 0.1..10.0;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
        .add_systems(Startup, (setup, spawn_fake_player).chain())
        .add_systems(
            Update,
            (
                update_tilemap,
                move_player,
                log_tile,
                update_camera,
                zoom_camera,
            )
                .chain(),
        )
        .run();
}

#[derive(Component, Deref, DerefMut)]
struct UpdateTimer(Timer);

#[derive(Resource, Deref, DerefMut)]
struct SeededRng(ChaCha8Rng);

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    // We're seeding the PRNG here to make this example deterministic for testing purposes.
    // This isn't strictly required in practical use unless you need your app to be deterministic.
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    let chunk_size = UVec2::splat(16);
    let tile_display_size = UVec2::splat(16);
    let tile_data: Vec<Option<TileData>> = (0..chunk_size.element_product())
        .map(|_| rng.random_range(0..15))
        .map(|i| {
            if i == 0 {
                None
            } else {
                Some(TileData::from_tileset_index(i - 1))
            }
        })
        .collect();

    commands.spawn((
        TilemapChunk {
            chunk_size,
            tile_display_size,
            tileset: assets.load_with_settings(
                "minefield-tiles.png",
                |settings: &mut ImageLoaderSettings| {
                    // The tileset texture is expected to be an array of tile textures, so we tell the
                    // `ImageLoader` that our texture is composed of 4 stacked tile images.
                    settings.array_layout = Some(ImageArrayLayout::RowCount { rows: 16 });
                },
            ),
            ..default()
        },
        TilemapChunkTileData(tile_data),
        UpdateTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
    ));

    commands.spawn(Camera2d);

    commands.insert_resource(SeededRng(rng));
}

/// Update the camera position by tracking the player.
fn update_camera(
    mut camera: Single<&mut Transform, (With<Camera2d>, Without<Player>)>,
    player: Single<&Transform, (With<Player>, Without<Camera2d>)>,
    time: Res<Time>,
) {
    let Vec3 { x, y, .. } = player.translation;
    let direction = Vec3::new(x, y, camera.translation.z);

    // Applies a smooth effect to camera movement using stable interpolation
    // between the camera position and the player position on the x and y axes.
    camera
        .translation
        .smooth_nudge(&direction, CAMERA_DECAY_RATE, time.delta_secs());
}

#[derive(Component)]
struct Player;

fn spawn_fake_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    chunk: Single<&TilemapChunk>,
) {
    let mut transform = chunk.calculate_tile_transform(UVec2::new(0, 0));
    transform.translation.z = 1.;

    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(8., 8.))),
        MeshMaterial2d(materials.add(Color::from(RED_400))),
        transform,
        Player,
    ));

    let mut transform = chunk.calculate_tile_transform(UVec2::new(5, 6));
    transform.translation.z = 1.;

    // second "player" to visually test a non-zero position
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(8., 8.))),
        MeshMaterial2d(materials.add(Color::from(RED_400))),
        transform,
    ));
}

/// Update the player position with keyboard inputs.
/// Note that the approach used here is for demonstration purposes only,
/// as the point of this example is to showcase the camera tracking feature.
///
/// A more robust solution for player movement can be found in `examples/movement/physics_in_fixed_timestep.rs`.
fn move_player(
    mut player: Single<&mut Transform, With<Player>>,
    time: Res<Time>,
    kb_input: Res<ButtonInput<KeyCode>>,
) {
    let mut direction = Vec2::ZERO;

    if kb_input.pressed(KeyCode::KeyW) {
        direction.y += 1.;
    }

    if kb_input.pressed(KeyCode::KeyS) {
        direction.y -= 1.;
    }

    if kb_input.pressed(KeyCode::KeyA) {
        direction.x -= 1.;
    }

    if kb_input.pressed(KeyCode::KeyD) {
        direction.x += 1.;
    }

    // Progressively update the player's position over time. Normalize the
    // direction vector to prevent it from exceeding a magnitude of 1 when
    // moving diagonally.
    let move_delta = direction.normalize_or_zero() * PLAYER_SPEED * time.delta_secs();
    player.translation += move_delta.extend(0.);
}

fn zoom_camera(
    camera: Single<&mut Projection, With<Camera>>,
    mouse_wheel_input: Res<AccumulatedMouseScroll>,
) {
    match *camera.into_inner() {
        Projection::Orthographic(ref mut orthographic) => {
            let delta_zoom = -mouse_wheel_input.delta.y * CAMERA_ZOOM_SPEED;
            // When changing scales, logarithmic changes are more intuitive.
            // To get this effect, we add 1 to the delta, so that a delta of 0
            // results in no multiplicative effect, positive values result in a multiplicative increase,
            // and negative values result in multiplicative decreases.
            let multiplicative_zoom = 1. + delta_zoom;

            orthographic.scale = (orthographic.scale * multiplicative_zoom)
                .clamp(CAMERA_ZOOM_RANGE.start, CAMERA_ZOOM_RANGE.end);
        }
        _ => (),
    }
}

fn update_tilemap(
    time: Res<Time>,
    mut query: Query<(&mut TilemapChunkTileData, &mut UpdateTimer)>,
    mut rng: ResMut<SeededRng>,
) {
    for (mut tile_data, mut timer) in query.iter_mut() {
        timer.tick(time.delta());

        if timer.just_finished() {
            for _ in 0..50 {
                let index = rng.random_range(0..tile_data.len());
                tile_data[index] = Some(TileData::from_tileset_index(rng.random_range(0..5)));
            }
        }
    }
}

// find the data for an arbitrary tile in the chunk and log its data
fn log_tile(tilemap: Single<(&TilemapChunk, &TilemapChunkTileData)>, mut local: Local<u16>) {
    let (chunk, data) = tilemap.into_inner();
    let Some(tile_data) = data.tile_data_from_tile_pos(chunk.chunk_size, UVec2::new(3, 4)) else {
        return;
    };
    // log when the tile changes
    if tile_data.tileset_index != *local {
        info!(?tile_data, "tile_data changed");
        *local = tile_data.tileset_index;
    }
}
