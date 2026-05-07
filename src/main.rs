use bevy::color::palettes::tailwind::BLUE_400;
use bevy::prelude::*;
use bevy::sprite_render::TilemapChunkMeshCache;
use bevy::{
    color::palettes::tailwind::RED_400,
    image::{ImageArrayLayout, ImageLoaderSettings},
    input::mouse::AccumulatedMouseScroll,
    sprite_render::{TileData, TilemapChunk, TilemapChunkTileData},
};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha8Rng;
use std::ops::Range;
use time::UtcDateTime;

/// Game Cursor movement speed factor.
const GAMECURSOR_SPEED: f32 = 1.0;

/// How quickly should the camera snap to the desired location.
const CAMERA_DECAY_RATE: f32 = 2.0;
const CAMERA_ZOOM_SPEED: f32 = 0.1;
const CAMERA_ZOOM_RANGE: Range<f32> = 0.0001..1.0;
const CELL_SIZE: u8 = 16;
const CELL_SCALE: f32 = 1. / (CELL_SIZE as f32);
const RANDOM_SEED: u64 = 34;

#[derive(Component)]
struct Cell {
    logical_position: LogicalPosition,
    state: CellState,
    bomb_locations: TilesArray,
}

#[derive(Debug, Clone, Copy)]
struct LogicalPosition {
    x: i64,
    y: i64,
}

#[derive(Debug, Component, Clone, Copy)]
struct TilePosition {
    cell: LogicalPosition,
    x: u8,
    y: u8,
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

type TilesArray = [[bool; CELL_SIZE as usize]; CELL_SIZE as usize];

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()))
        .add_systems(
            Startup,
            (setup, spawn_gamecursor, spawn_tile_cursor).chain(),
        )
        .add_systems(
            Update,
            (
                move_gamecursor,
                move_tile_cursor,
                move_cells,
                update_camera,
                zoom_camera,
            )
                .chain(),
        )
        .add_systems(Update, (on_click).chain())
        .run();
}

#[derive(Resource, Deref, DerefMut)]
struct SeededRng(ChaCha8Rng);

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    // We're seeding the PRNG here to make this example deterministic for testing purposes.
    // This isn't strictly required in practical use unless you need your app to be deterministic.
    let mut rng = ChaCha8Rng::seed_from_u64(RANDOM_SEED);

    let minefield_tilemap_chunk: TilemapChunk = TilemapChunk {
        chunk_size: UVec2::splat(CELL_SIZE as u32),
        tile_display_size: UVec2::splat(1),
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

    let tile_data: Vec<Option<TileData>> = (0..UVec2::splat(CELL_SIZE as u32).element_product())
        .map(|_| rng.random_range(0..15))
        .map(|i| {
            if i == 0 {
                None
            } else {
                Some(TileData::from_tileset_index(i - 1))
            }
        })
        .collect();
    let mut initial_bomb_locations: TilesArray = default();
    for i in 1..(CELL_SIZE as usize) {
        initial_bomb_locations[i - 1][i - 1] = true;
    }

    let logical_scale: f32 = 1. / (CELL_SIZE as f32);

    commands.spawn((
        Transform {
            scale: { Vec3::splat(CELL_SCALE) },
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
            scale: { Vec3::new(logical_scale, logical_scale, logical_scale) },
            ..Default::default()
        },
        Cell {
            logical_position: LogicalPosition { x: 2, y: 0 },
            state: CellState::Fresh,
            bomb_locations: initial_bomb_locations,
        },
        minefield_tilemap_chunk.clone(),
        TilemapChunkTileData(tile_data.clone()),
    ));

    commands.spawn((
        Transform {
            scale: { Vec3::new(logical_scale, logical_scale, logical_scale) },
            ..Default::default()
        },
        Cell {
            logical_position: LogicalPosition { x: 2, y: 2 },
            state: CellState::Fresh,
            bomb_locations: initial_bomb_locations,
        },
        minefield_tilemap_chunk.clone(),
        TilemapChunkTileData(tile_data.clone()),
    ));

    commands.spawn((
        Transform {
            scale: { Vec3::new(logical_scale, logical_scale, logical_scale) },
            ..Default::default()
        },
        Cell {
            logical_position: LogicalPosition { x: -1, y: -1 },
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
    frac_position: Vec2,
}

fn spawn_gamecursor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(0.2, 0.2))),
        MeshMaterial2d(materials.add(Color::from(RED_400))),
        Transform {
            translation: Vec3 {
                x: 0.,
                y: 0.,
                z: 1.,
            },
            ..default()
        },
        GameCursor {
            logical_position: LogicalPosition { x: 0, y: 0 },
            frac_position: Vec2 { x: 0., y: 0. },
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
    let move_delta = direction.normalize_or_zero() * GAMECURSOR_SPEED * time.delta_secs();

    game_cursor.frac_position += move_delta;

    game_cursor.logical_position.x += game_cursor.frac_position.x.round() as i64;
    game_cursor.logical_position.y += game_cursor.frac_position.y.round() as i64;

    game_cursor.frac_position = game_cursor.frac_position.fract();

    game_cursor.frac_position = GameCursor::cursor_frac_wrap(game_cursor.frac_position);

    // println!(
    //     "Game_Cursor: {:?}, {:?}",
    //     game_cursor.logical_position, game_cursor.frac_position
    // );
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

// fn spawn_visible_cells(
//     game_cursor: Single<&GameCursor>,
//     q_window: Single<&Window>,
//     q_camera: Single<&Camera>,
//     q_cells: Query<&Cell>,
// ) {
// }

fn move_cells(
    q_cells: Query<(&mut Transform, &Cell), With<Cell>>,
    game_cursor: Single<&GameCursor>,
) {
    for (mut transform, cell) in q_cells {
        let game_space_transform: Vec2 =
            GameCursor::logical_to_world(&game_cursor, cell.logical_position);
        transform.translation.x = -game_space_transform.x;
        transform.translation.y = -game_space_transform.y;
        transform.translation.z = 0.;
    }
}

fn on_click(
    game_cursor: Single<&GameCursor>,
    q_window: Single<&Window>,
    q_camera: Single<&Camera>,
    mouse_click_input: Res<ButtonInput<MouseButton>>,
) {
    if let Some(cursor_position) = q_window.cursor_position() {
        if let Ok(world_position) =
            q_camera.viewport_to_world_2d(&GlobalTransform::default(), cursor_position)
        {
            let tile_location = GameCursor::world_2d_to_logical(&game_cursor, world_position);
            if mouse_click_input.just_pressed(MouseButton::Left) {
                println!("Reveal: {:?}", tile_location);
            } else if mouse_click_input.just_pressed(MouseButton::Right) {
                println!("Flag: {:?}", tile_location);
            }
        }
    }
}

fn spawn_tile_cursor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(CELL_SCALE, CELL_SCALE))),
        MeshMaterial2d(materials.add(Color::from(BLUE_400))),
        Transform {
            translation: Vec3 {
                x: 0.,
                y: 0.,
                z: 1.,
            },
            ..default()
        },
        TilePosition {
            cell: LogicalPosition { x: 0, y: 0 },
            x: 0,
            y: 0,
        },
    ));
}
fn move_tile_cursor(
    game_cursor: Single<&GameCursor>,
    q_window: Single<&Window>,
    q_camera: Single<&Camera>,
    mut tile_pos: Single<&mut TilePosition>,
    mut tile_transform: Single<&mut Transform, With<TilePosition>>,
) {
    if let Some(cursor_position) = q_window.cursor_position() {
        if let Ok(world_position) =
            q_camera.viewport_to_world_2d(&GlobalTransform::default(), cursor_position)
        {
            let temp_pos: TilePosition =
                GameCursor::world_2d_to_logical(&game_cursor, world_position);
            // tile_pos.cell = temp_pos.cell;
            // tile_pos.x = temp_pos.x;
            // tile_pos.y = temp_pos.y;

            let game_space_transform: Vec2 = TilePosition::tile_to_world(&game_cursor, temp_pos);
            println!("cursor tile pos: {:?}", temp_pos);
            println!("transform: {:?}", game_space_transform);
            tile_transform.translation.x = game_space_transform.x;
            tile_transform.translation.y = game_space_transform.y;
            tile_transform.translation.z = 0.5;
        }
    }
}

impl Cell {
    pub fn reveal_tile(target_tile: TilePosition) {
        unimplemented!()
    }
    pub fn place_flag() {
        unimplemented!()
    }
}

impl TilePosition {
    pub fn tile_to_world(game_cursor: &GameCursor, tile_position: TilePosition) -> Vec2 {
        return Vec2 {
            x: ((game_cursor.logical_position.x - tile_position.cell.x) as f32)
                + (tile_position.x as f32 * CELL_SCALE),
            y: ((game_cursor.logical_position.y - tile_position.cell.y) as f32)
                + (tile_position.y as f32 * CELL_SCALE),
        };
    }
}

impl GameCursor {
    fn new(logical_position: LogicalPosition, frac_position: Vec2) -> Self {
        GameCursor {
            logical_position: logical_position,
            frac_position: frac_position,
        }
    }

    pub fn logical_to_world(game_cursor: &GameCursor, logical_position: LogicalPosition) -> Vec2 {
        return Vec2 {
            x: (game_cursor.logical_position.x - logical_position.x) as f32
                + game_cursor.frac_position.x,
            y: (game_cursor.logical_position.y - logical_position.y) as f32
                + game_cursor.frac_position.y,
        };
    }

    pub fn world_2d_to_logical(game_cursor: &GameCursor, world_position_2d: Vec2) -> TilePosition {
        let mut offset_frac_pos = game_cursor.frac_position + world_position_2d;

        let log_pos: LogicalPosition = LogicalPosition {
            x: game_cursor.logical_position.x + offset_frac_pos.x.round() as i64,
            y: game_cursor.logical_position.y + offset_frac_pos.y.round() as i64,
        };

        offset_frac_pos =
            GameCursor::cursor_frac_wrap(offset_frac_pos.fract()) + Vec2::new(0.5, 0.5);
        offset_frac_pos = offset_frac_pos * Vec2::splat(CELL_SIZE as f32);

        return TilePosition {
            cell: log_pos,
            x: offset_frac_pos.x.round_ties_even() as u8,
            y: offset_frac_pos.y.round_ties_even() as u8,
        };
    }

    const CURSOR_FRAC_WRAP_LIMIT: Rect = Rect {
        min: Vec2 { x: -0.5, y: -0.5 },
        max: Vec2 { x: 0.5, y: 0.5 },
    };
    pub fn cursor_frac_wrap(mut cursor_frac: Vec2) -> Vec2 {
        if cursor_frac.x > Self::CURSOR_FRAC_WRAP_LIMIT.max.x {
            cursor_frac.x -= Self::CURSOR_FRAC_WRAP_LIMIT.width();
        } else if cursor_frac.x < Self::CURSOR_FRAC_WRAP_LIMIT.min.x {
            cursor_frac.x += Self::CURSOR_FRAC_WRAP_LIMIT.width();
        }

        if cursor_frac.y > Self::CURSOR_FRAC_WRAP_LIMIT.max.y {
            cursor_frac.y -= Self::CURSOR_FRAC_WRAP_LIMIT.height();
        } else if cursor_frac.y < Self::CURSOR_FRAC_WRAP_LIMIT.min.y {
            cursor_frac.y += Self::CURSOR_FRAC_WRAP_LIMIT.height();
        }

        return cursor_frac;
    }
}
