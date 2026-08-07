//! Framebuffer support for the kernel.

use core::iter;

/// IBM PC VGA 8x16 font data for framebuffer text rendering.
pub static VGA8X16: FramebufferFont =
    FramebufferFont::new(include_bytes!("../../assets/fonts/VGA8.F16"), 16);

/// Represents a point in 2D space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    /// The x-coordinate of the point.
    pub x: i32,
    /// The y-coordinate of the point.
    pub y: i32,
}

impl Point {
    /// A point at the origin (0, 0).
    pub const ZERO: Self = Self { x: 0, y: 0 };

    /// Creates a new `Point` with the specified coordinates.
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Represents a pixel in the framebuffer, consisting of a point and a color.
///
/// The color is represented as a 32-bit unsigned integer in XRGB8888 format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pixel(pub Point, pub u32);

impl Pixel {
    /// Creates a new `Pixel` with the specified coordinates and color.
    pub const fn new(x: i32, y: i32, color: u32) -> Self {
        Self(Point::new(x, y), color)
    }

    /// Returns the x-coordinate of the pixel.
    pub const fn x(&self) -> i32 {
        self.0.x
    }

    /// Returns the y-coordinate of the pixel.
    pub const fn y(&self) -> i32 {
        self.0.y
    }

    /// Returns the color of the pixel.
    pub const fn color(&self) -> u32 {
        self.1
    }
}

/// A framebuffer device with a specific geometry and pixel format.
pub trait DrawTarget {
    /// Returns the width of the framebuffer in pixels.
    fn width(&self) -> usize;

    /// Returns the height of the framebuffer in pixels.
    fn height(&self) -> usize;

    /// Draws an iterator of pixels onto the framebuffer.
    fn draw_iter<I>(&mut self, pixels: I)
    where
        I: IntoIterator<Item = Pixel>;

    /// Clears the framebuffer with the specified color.
    ///
    /// Implementors will typically want to provide a more efficient implementation than the default one,
    /// which draws each pixel individually.
    fn clear(&mut self, color: u32) {
        let (width, height) = (self.width(), self.height());

        self.draw_iter(
            (0..width)
                .flat_map(|x| (0..height).map(move |y| Pixel::new(x as i32, y as i32, color))),
        );
    }
}

/// A [`DrawTarget`]-backed framebuffer that provides higher-level drawing operations, such as
/// geometry and text rendering.
pub struct Framebuffer<T> {
    target: T,
}

impl<T: DrawTarget> Framebuffer<T> {
    /// Creates a new `Framebuffer` that wraps the specified [`DrawTarget`].
    pub fn new(target: T) -> Self {
        Self { target }
    }

    /// Clears the framebuffer with the specified color.
    pub fn clear(&mut self, color: u32) {
        self.target.clear(color);
    }

    /// Draws a pixel at the specified coordinates with the given color.
    pub fn draw_pixel(&mut self, px: Pixel) {
        self.target.draw_iter(iter::once(px));
    }

    /// Draws a string of text at the specified point with the given color using the provided font.
    ///
    /// If the text contains newline characters, the cursor will move down to the next line.
    /// If the text contains carriage return characters, the cursor will return to the start of the line.
    pub fn draw_text(&mut self, font: &FramebufferFont, p: Point, text: &str, color: u32) {
        let mut cursor_x = p.x;
        let mut cursor_y = p.y;

        for ch in text.bytes() {
            match ch {
                b'\n' => {
                    cursor_x = p.x;
                    cursor_y += font.char_height as i32;
                }
                b'\r' => {
                    cursor_x = p.x;
                }
                ch => {
                    font.draw_glyph(&mut self.target, Point::new(cursor_x, cursor_y), ch, color);
                    cursor_x += FramebufferFont::CHAR_WIDTH as i32; // Move to the next character position
                }
            }
        }
    }
}

/// A font for rendering text on a framebuffer.
pub struct FramebufferFont<'a> {
    font: &'a [u8],
    char_height: usize,
}

impl<'a> FramebufferFont<'a> {
    const CHAR_WIDTH: usize = 8;

    /// Creates a new `FramebufferFont` with the specified font data and character height.
    ///
    /// The font data should be a byte slice where each character is represented by a fixed number of bytes
    /// corresponding to the character height. For example, an 8x16 font would have 16 bytes per character.
    ///
    /// Only 8 pixel width characters are supported, and the font data should be arranged in a way
    /// that each character's bitmap is stored sequentially in the slice, indexed by the ASCII value
    /// of the character.
    pub const fn new(font: &'a [u8], char_height: usize) -> Self {
        Self { font, char_height }
    }

    /// Returns the glyph bitmap for the specified character, or `None` if the character is not supported.
    pub fn glyph(&self, ch: u8) -> Option<&[u8]> {
        let index = ch as usize;
        let glyph_size = self.char_height;
        let start = index * glyph_size;
        let end = start + glyph_size;

        if end <= self.font.len() {
            Some(&self.font[start..end])
        } else {
            None
        }
    }

    /// Draws a single character glyph at the specified point with the given color on the provided surface.
    pub fn draw_glyph(&self, target: &mut impl DrawTarget, p: Point, ch: u8, color: u32) {
        debug_assert_eq!(
            Self::CHAR_WIDTH,
            8,
            "Only 8 pixel width characters are supported"
        );

        if let Some(glyph) = self.glyph(ch) {
            for (row, &row_data) in glyph.iter().enumerate() {
                let pixels = (0..8).filter_map(|col| {
                    if (row_data >> (7 - col)) & 1 != 0 {
                        Some(Pixel::new(p.x + col, p.y + row as i32, color))
                    } else {
                        None
                    }
                });
                target.draw_iter(pixels);
            }
        }
    }
}
