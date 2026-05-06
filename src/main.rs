use bevy::{
    color::palettes::tailwind::RED_400,
    image::{ImageArrayLayout, ImageLoaderSettings},
    input::mouse::AccumulatedMouseScroll,
    math::bounding::Aabb2d,
    prelude::*,
    sprite_render::{TileData, TilemapChunk, TilemapChunkTileData},
};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::{
    mem::transmute,
    ops::{Index, Range},
};
use time::UtcDateTime;

/// Game Cursor movement speed factor.
const GAMECURSOR_SPEED: f32 = 100.;

/// How quickly should the camera snap to the desired location.
const CAMERA_DECAY_RATE: f32 = 2.;
const CAMERA_ZOOM_SPEED: f32 = 0.5;
const CAMERA_ZOOM_RANGE: Range<f32> = 0.1..10.0;
const CELL_SIZE: usize = 16;

const RANDOM_SEED: u64 = 34;

#[derive(Component)]
struct Cell {
    logical_position: LogicalPosition,
    state: CellState,
    bomb_locations: TilesArray,
}

#[derive(Debug)]
struct LogicalPosition {
    x: i64,
    y: i64,
}

enum CellState {
    Fresh,
    InProgress {
        revealed_tiles: TilesArray,
    },
    LockedOut {
        lock_start_time: UtcDateTime,
        cause_tile: (usize, usize),
        revealed_tiles: TilesArray,
    },
    Solved,
}

type TilesArray = [[bool; CELL_SIZE]; CELL_SIZE];

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
        .add_systems(Startup, (setup, spawn_gamecursor).chain())
        .add_systems(
            Update,
            (
                // update_tilemap,
                move_gamecursor,
                move_cells,
                // log_tile,
                update_camera,
                zoom_camera,
            )
                .chain(),
        )
        .run();
}

#[derive(Resource, Deref, DerefMut)]
struct SeededRng(ChaCha8Rng);

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    // We're seeding the PRNG here to make this example deterministic for testing purposes.
    // This isn't strictly required in practical use unless you need your app to be deterministic.
    let mut rng = ChaCha8Rng::seed_from_u64(RANDOM_SEED);

    let minefield_tilemap_chunk: TilemapChunk = TilemapChunk {
        chunk_size: UVec2::splat(16),
        tile_display_size: UVec2::splat(16),
        tileset: assets.load_with_settings(
            "minefield-tiles.png",
            |settings: &mut ImageLoaderSettings| {
                // The tileset texture is expected to be an array of tile textures, so we tell the
                // `ImageLoader` that our texture is composed of 4 stacked tile images.
                settings.array_layout = Some(ImageArrayLayout::RowCount { rows: 16 });
            },
        ),
        ..default()
    };

    let chunk_size = UVec2::splat(16);
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
    let mut initial_bomb_locations: TilesArray = [[false; CELL_SIZE]; CELL_SIZE];
    for i in 1..CELL_SIZE {
        initial_bomb_locations[i - 1][i - 1] = true;
    }

    commands.spawn((
        Transform {
            ..Default::default()
        },
        Cell {
            logical_position: LogicalPosition { x: 0, y: 0 },
            state: CellState::Fresh,
            bomb_locations: initial_bomb_locations,
        },
        minefield_tilemap_chunk.clone(),
        TilemapChunkTileData(tile_data.clone()),
    ));

    commands.spawn((
        Transform {
            ..Default::default()
        },
        Cell {
            logical_position: LogicalPosition { x: 100, y: 0 },
            state: CellState::Fresh,
            bomb_locations: initial_bomb_locations,
        },
        minefield_tilemap_chunk.clone(),
        TilemapChunkTileData(tile_data.clone()),
    ));

    commands.spawn(Camera2d);

    commands.insert_resource(SeededRng(rng));
}

/// Update the camera position by tracking the player.
fn update_camera(
    mut camera: Single<&mut Transform, (With<Camera2d>, Without<GameCursor>)>,
    game_cursor: Single<&Transform, (With<GameCursor>, Without<Camera2d>)>,
    time: Res<Time>,
) {
    let Vec3 { x, y, .. } = game_cursor.translation;
    let direction = Vec3::new(x, y, camera.translation.z);

    // Applies a smooth effect to camera movement using stable interpolation
    // between the camera position and the player position on the x and y axes.
    camera
        .translation
        .smooth_nudge(&direction, CAMERA_DECAY_RATE, time.delta_secs());
}

#[derive(Component, Debug)]
struct GameCursor {
    logical_position: LogicalPosition,
    float_position: Vec2,
}

fn spawn_gamecursor(
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
        GameCursor {
            logical_position: LogicalPosition { x: 0, y: 0 },
            float_position: Vec2 { x: 0., y: 0. },
        },
    ));
}

fn move_gamecursor(
    mut game_cursor: Single<&mut GameCursor>,
    time: Res<Time>,
    kb_input: Res<ButtonInput<KeyCode>>,
) {
    let mut direction = Vec2::ZERO;

    if kb_input.pressed(KeyCode::KeyW) {
        direction.y -= 1.;
    }

    if kb_input.pressed(KeyCode::KeyS) {
        direction.y += 1.;
    }

    if kb_input.pressed(KeyCode::KeyA) {
        direction.x += 1.;
    }

    if kb_input.pressed(KeyCode::KeyD) {
        direction.x -= 1.;
    }

    // Progressively update the player's position over time. Normalize the
    // direction vector to prevent it from exceeding a magnitude of 1 when
    // moving diagonally.
    let move_delta = direction.normalize_or_zero() * GAMECURSOR_SPEED * time.delta_secs();

    game_cursor.logical_position.x = game_cursor.logical_position.x
        + (move_delta.x.trunc() as i64)
        + (game_cursor.float_position.x.trunc() as i64);
    game_cursor.logical_position.y = game_cursor.logical_position.y
        + (move_delta.y.trunc() as i64)
        + (game_cursor.float_position.y.trunc() as i64);

    game_cursor.float_position.x = game_cursor.float_position.x.fract() + move_delta.x.fract();
    game_cursor.float_position.y = game_cursor.float_position.y.fract() + move_delta.y.fract();

    // game_cursor.translation += move_delta.extend(0.);
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

// fn spawn_cells(
//     mut commands: Commands,
//     game_cursor: Single<&GameCursor>,
//     transform: Query<&Transform, With<Camera>>,
//     windows: Query<&Window>,
//     random: Res<SeededRng>,
// ) {
//     let Ok(window) = windows.single() else {
//         return;
//     };

//     let window_size = window.size();

//     // TODO: Actually Spawn Cells
// }

fn move_cells(
    q_cells: Query<(&mut Transform, &Cell), With<Cell>>,
    game_cursor: Single<&GameCursor>,
) {
    for (mut transform, cell) in q_cells {
        let game_space_transform: Vec2 = Vec2 {
            x: ((game_cursor.logical_position.x - cell.logical_position.x) as f32)
                + game_cursor.float_position.x,
            y: ((game_cursor.logical_position.y - cell.logical_position.y) as f32)
                + game_cursor.float_position.y,
        };

        transform.translation.x = game_space_transform.x;
        transform.translation.y = game_space_transform.y;
        transform.translation.z = 0.;

        println!("{}, {:?}", transform.translation, cell.logical_position);
    }
}
