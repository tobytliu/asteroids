//! The player ship as palette-indexed pixel art, drawn nose-up with light
//! from the upper left. `.` is transparent.

pub const SHIP: [&str; 18] = [
    ".......K.......",
    "......KWK......",
    "......KLK......",
    ".....KWLMK.....",
    ".....KLCMK.....",
    "....KWCCcMK....",
    "....KLWCcMK....",
    "....KLcccDK....",
    "...KWLMMMDDK...",
    "..KWLMKMKMDDK..",
    ".KWLMMRMRMMDDK.",
    "KWLMMMRMRMMMDDK",
    "KLMMDKMMMKDMMDK",
    "KLMDK.KMK.KDMDK",
    "KMDK..KMK..KDDK",
    "KDK...KDK...KDK",
    ".K....KOK....K.",
    "......KKK......",
];

/// How many times larger the uploaded texture is than the sprite. Must match
/// `UPSCALE` in `ship.wgsl`.
const UPSCALE_PASSES: u32 = 3; // 2^3 = 8×

/// A sprite expanded to RGBA8 texels, row-major from the top-left.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decode a sprite and upscale it with Scale2x. Rotating the upscaled copy
/// keeps outlines and details intact where rotating the original would break
/// them up (the "RotSprite" technique).
pub fn decode(rows: &[&str]) -> Image {
    let width = rows[0].len();
    assert!(
        rows.iter().all(|row| row.len() == width),
        "ragged sprite rows"
    );
    let mut grid: Vec<Vec<char>> = rows.iter().map(|row| row.chars().collect()).collect();
    for _ in 0..UPSCALE_PASSES {
        grid = scale2x(&grid);
    }
    Image {
        width: grid[0].len() as u32,
        height: grid.len() as u32,
        rgba: grid.iter().flatten().flat_map(|&c| color(c)).collect(),
    }
}

/// Double a pixel grid, rounding diagonal edges instead of making 2×2 blocks.
fn scale2x(grid: &[Vec<char>]) -> Vec<Vec<char>> {
    let (h, w) = (grid.len() as isize, grid[0].len() as isize);
    let at = |y: isize, x: isize| {
        if (0..h).contains(&y) && (0..w).contains(&x) {
            grid[y as usize][x as usize]
        } else {
            '.'
        }
    };
    let mut out = vec![vec!['.'; 2 * w as usize]; 2 * h as usize];
    for y in 0..h {
        for x in 0..w {
            let (p, up, right, left, down) = (
                at(y, x),
                at(y - 1, x),
                at(y, x + 1),
                at(y, x - 1),
                at(y + 1, x),
            );
            let (oy, ox) = (2 * y as usize, 2 * x as usize);
            out[oy][ox] = if left == up && left != down && up != right {
                up
            } else {
                p
            };
            out[oy][ox + 1] = if up == right && up != left && right != down {
                right
            } else {
                p
            };
            out[oy + 1][ox] = if down == left && down != right && left != up {
                left
            } else {
                p
            };
            out[oy + 1][ox + 1] = if right == down && right != up && down != left {
                down
            } else {
                p
            };
        }
    }
    out
}

fn color(c: char) -> [u8; 4] {
    match c {
        '.' => [0, 0, 0, 0],
        'K' => [0x1a, 0x1c, 0x2c, 0xff], // outline
        'D' => [0x33, 0x3c, 0x57, 0xff], // hull shadow
        'M' => [0x56, 0x6c, 0x86, 0xff], // hull
        'L' => [0x94, 0xb0, 0xc2, 0xff], // hull light
        'W' => [0xf4, 0xf4, 0xf4, 0xff], // highlight
        'C' => [0x41, 0xa6, 0xf6, 0xff], // canopy
        'c' => [0x29, 0x36, 0x6f, 0xff], // canopy shadow
        'R' => [0xb1, 0x3e, 0x53, 0xff], // stripe
        'O' => [0xef, 0x7d, 0x57, 0xff], // engine
        _ => panic!("unknown sprite color {c:?}"),
    }
}
