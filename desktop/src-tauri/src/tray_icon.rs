use font8x8::{BASIC_FONTS, UnicodeFonts};
use image::{Rgba, RgbaImage};
use tauri::image::Image;

const SIZE: u32 = 64;

#[derive(Clone, Copy)]
pub enum IconState {
    Remaining(u8),
    Error,
}

fn background_color(state: IconState) -> Rgba<u8> {
    match state {
        IconState::Remaining(value) if value > 50 => Rgba([38, 162, 105, 255]),
        IconState::Remaining(value) if value > 20 => Rgba([229, 165, 10, 255]),
        IconState::Remaining(_) => Rgba([224, 27, 36, 255]),
        IconState::Error => Rgba([110, 110, 110, 255]),
    }
}

fn draw_digit(image: &mut RgbaImage, character: char, x: u32, y: u32, scale: u32) {
    let Some(glyph) = BASIC_FONTS.get(character) else {
        return;
    };

    for (row, bits) in glyph.iter().enumerate() {
        for column in 0..8 {
            if bits & (1u8 << column) == 0 {
                continue;
            }

            for dy in 0..scale {
                for dx in 0..scale {
                    let px = x + column * scale + dx;
                    let py = y + row as u32 * scale + dy;

                    if px < SIZE && py < SIZE {
                        image.put_pixel(px, py, Rgba([255, 255, 255, 255]));
                    }
                }
            }
        }
    }
}

pub fn generate(state: IconState) -> Image<'static> {
    let color = background_color(state);
    let mut image = RgbaImage::new(SIZE, SIZE);

    // Закруглённый фон.
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = if x < 12 {
                12 - x
            } else if x > 51 {
                x - 51
            } else {
                0
            };

            let dy = if y < 12 {
                12 - y
            } else if y > 51 {
                y - 51
            } else {
                0
            };

            if dx * dx + dy * dy <= 144 {
                image.put_pixel(x, y, color);
            }
        }
    }

    let text = match state {
        IconState::Remaining(value) => value.to_string(),
        IconState::Error => "?".to_string(),
    };

    let scale = if text.len() >= 3 { 2 } else { 3 };
    let width = text.len() as u32 * 8 * scale;
    let x = (SIZE - width) / 2;
    let y = (SIZE - 8 * scale) / 2;

    for (index, character) in text.chars().enumerate() {
        draw_digit(
            &mut image,
            character,
            x + index as u32 * 8 * scale,
            y,
            scale,
        );
    }

    Image::new_owned(image.into_raw(), SIZE, SIZE)
}
