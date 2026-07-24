use super::coordinates::{DesktopPoint, TileCoordinate, TileLocalPixel};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_TILE_SIZE: u32 = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub pixels: Vec<u8>,
    pub preview_dirty: bool,
    pub recovery_dirty: bool,
    pub revision: u64,
    pub contains_artwork: bool,
}
impl Tile {
    pub fn blank(size: u32) -> Self {
        Self {
            pixels: vec![0; (size * size * 4) as usize],
            preview_dirty: false,
            recovery_dirty: false,
            revision: 0,
            contains_artwork: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SparseTileStore {
    pub tile_size: u32,
    pub tiles: HashMap<TileCoordinate, Tile>,
}
impl Default for SparseTileStore {
    fn default() -> Self {
        Self {
            tile_size: DEFAULT_TILE_SIZE,
            tiles: HashMap::new(),
        }
    }
}
impl SparseTileStore {
    pub fn get_or_allocate(&mut self, coord: TileCoordinate) -> &mut Tile {
        self.tiles
            .entry(coord)
            .or_insert_with(|| Tile::blank(self.tile_size))
    }
    pub fn touched_tile_count(&self) -> usize {
        self.tiles.len()
    }
    pub fn artwork_tile_count(&self) -> usize {
        self.tiles.values().filter(|t| t.contains_artwork).count()
    }
    pub fn put_pixel(&mut self, x: i32, y: i32, rgba: [u8; 4], blend: impl Fn(&mut [u8], [u8; 4])) {
        let (tc, lp) = desktop_pixel_to_tile(x, y, self.tile_size);
        let size = self.tile_size;
        let tile = self.get_or_allocate(tc);
        let idx = ((lp.y as u32 * size + lp.x as u32) * 4) as usize;
        blend(&mut tile.pixels[idx..idx + 4], rgba);
        tile.preview_dirty = true;
        tile.recovery_dirty = true;
        tile.revision += 1;
        if tile.pixels[idx + 3] != 0 {
            tile.contains_artwork = true;
        }
    }
}

pub fn desktop_point_to_tile(
    point: DesktopPoint,
    tile_size: u32,
) -> (TileCoordinate, TileLocalPixel) {
    desktop_pixel_to_tile(point.x.floor() as i32, point.y.floor() as i32, tile_size)
}
pub fn desktop_pixel_to_tile(x: i32, y: i32, tile_size: u32) -> (TileCoordinate, TileLocalPixel) {
    let s = tile_size as i32;
    let tx = x.div_euclid(s);
    let ty = y.div_euclid(s);
    let lx = x.rem_euclid(s) as u16;
    let ly = y.rem_euclid(s) as u16;
    (
        TileCoordinate { x: tx, y: ty },
        TileLocalPixel { x: lx, y: ly },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_allocation_creating_only_touched_tiles() {
        let mut s = SparseTileStore::default();
        s.put_pixel(1, 1, [1, 2, 3, 255], |d, c| d.copy_from_slice(&c));
        assert_eq!(s.touched_tile_count(), 1);
    }
    #[test]
    fn supports_negative_tile_indices() {
        let (t, l) = desktop_pixel_to_tile(-1, -257, 256);
        assert_eq!(t, TileCoordinate { x: -1, y: -2 });
        assert_eq!(l, TileLocalPixel { x: 255, y: 255 });
    }
}
