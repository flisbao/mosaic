use crate::domain::theme::Rgb;

const INDEXED_CUBE_START: u8 = 16;
const INDEXED_CUBE_SIDE: usize = 6;
const INDEXED_CUBE_LEVELS: [u8; INDEXED_CUBE_SIDE] = [0, 95, 135, 175, 215, 255];
const INDEXED_GRAY_START: u8 = 232;
const INDEXED_GRAY_STEPS: u8 = 24;
const INDEXED_GRAY_BASE: i32 = 8;
const INDEXED_GRAY_STEP: i32 = 10;

pub fn nearest_indexed(rgb: Rgb) -> u8 {
    let cube = nearest_cube_index(rgb);
    let gray = nearest_gray_index(rgb);
    if distance(rgb, indexed_rgb(gray)) < distance(rgb, indexed_rgb(cube)) {
        gray
    } else {
        cube
    }
}

fn nearest_cube_index(rgb: Rgb) -> u8 {
    let level = |channel: u8| {
        INDEXED_CUBE_LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, candidate)| (i32::from(**candidate) - i32::from(channel)).abs())
            .map_or(0, |(position, _)| position)
    };
    let (red, green, blue) = (level(rgb.red), level(rgb.green), level(rgb.blue));
    let offset = red * INDEXED_CUBE_SIDE * INDEXED_CUBE_SIDE + green * INDEXED_CUBE_SIDE + blue;
    INDEXED_CUBE_START + u8::try_from(offset).unwrap_or(0)
}

fn nearest_gray_index(rgb: Rgb) -> u8 {
    let average = (i32::from(rgb.red) + i32::from(rgb.green) + i32::from(rgb.blue)) / 3;
    let step = ((average - INDEXED_GRAY_BASE + INDEXED_GRAY_STEP / 2) / INDEXED_GRAY_STEP)
        .clamp(0, i32::from(INDEXED_GRAY_STEPS) - 1);
    INDEXED_GRAY_START + u8::try_from(step).unwrap_or(0)
}

fn indexed_rgb(index: u8) -> Rgb {
    if index >= INDEXED_GRAY_START {
        let level = INDEXED_GRAY_BASE + INDEXED_GRAY_STEP * i32::from(index - INDEXED_GRAY_START);
        let level = u8::try_from(level).unwrap_or(u8::MAX);
        return Rgb::new(level, level, level);
    }
    let offset = usize::from(index - INDEXED_CUBE_START);
    let level = |position: usize| INDEXED_CUBE_LEVELS[position % INDEXED_CUBE_SIDE];
    Rgb::new(
        level(offset / (INDEXED_CUBE_SIDE * INDEXED_CUBE_SIDE)),
        level(offset / INDEXED_CUBE_SIDE),
        level(offset),
    )
}

fn distance(first: Rgb, second: Rgb) -> i32 {
    let delta = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
    delta(first.red, second.red) + delta(first.green, second.green) + delta(first.blue, second.blue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_indexed_maps_pure_black_white_and_grays() {
        assert_eq!(nearest_indexed(Rgb::new(0, 0, 0)), 16);
        assert_eq!(nearest_indexed(Rgb::new(255, 255, 255)), 231);
        assert_eq!(nearest_indexed(Rgb::new(128, 128, 128)), 244);
        assert_eq!(nearest_indexed(Rgb::new(255, 0, 0)), 196);
    }

    #[test]
    fn nearest_indexed_is_stable_for_exact_cube_colors() {
        assert_eq!(nearest_indexed(Rgb::new(0x5f, 0x87, 0xaf)), 67);
        assert_eq!(nearest_indexed(Rgb::new(0xd7, 0xd7, 0xd7)), 188);
    }

    #[test]
    fn nearest_indexed_never_leaves_the_extended_range() {
        for red in (0..=255u16).step_by(15) {
            for green in (0..=255u16).step_by(15) {
                for blue in (0..=255u16).step_by(15) {
                    let index = nearest_indexed(Rgb::new(red as u8, green as u8, blue as u8));
                    assert!(index >= INDEXED_CUBE_START);
                }
            }
        }
    }
}
