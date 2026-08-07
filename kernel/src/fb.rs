//! Framebuffer support for the kernel.

use alloc::boxed::Box;
use spin::Once;

use crate::sync::IrqSpinLock;

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

/// Represents a rectangle in 2D space, defined by its origin point and dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    tl: Point,
    br: Point,
}

impl Rect {
    /// Creates a rectangle spanning from `tl` (inclusive) to `br` (exclusive).
    pub const fn new(tl: Point, br: Point) -> Self {
        Self { tl, br }
    }

    /// Creates a rectangle with its top-left corner at `origin` and the given dimensions.
    pub const fn from_size(origin: Point, width: i32, height: i32) -> Self {
        Self {
            tl: origin,
            br: Point::new(origin.x + width, origin.y + height),
        }
    }

    /// Returns this rectangle translated by the given offset.
    pub const fn translate(self, offset: Point) -> Self {
        Self {
            tl: Point::new(self.tl.x + offset.x, self.tl.y + offset.y),
            br: Point::new(self.br.x + offset.x, self.br.y + offset.y),
        }
    }

    /// Returns the y-coordinate of the top edge of the rectangle.
    #[inline]
    pub fn top(&self) -> i32 {
        self.tl.y
    }

    /// Returns the x-coordinate of the left edge of the rectangle.
    #[inline]
    pub fn left(&self) -> i32 {
        self.tl.x
    }

    /// Returns the y-coordinate of the bottom edge of the rectangle.
    #[inline]
    pub fn bottom(&self) -> i32 {
        self.br.y
    }

    /// Returns the x-coordinate of the right edge of the rectangle.
    #[inline]
    pub fn right(&self) -> i32 {
        self.br.x
    }

    /// Returns the width of the rectangle.
    #[inline]
    pub fn width(&self) -> i32 {
        self.br.x - self.tl.x
    }

    /// Returns the height of the rectangle.
    #[inline]
    pub fn height(&self) -> i32 {
        self.br.y - self.tl.y
    }

    /// Returns whether the specified point is contained within this rectangle.
    pub fn contains(&self, point: Point) -> bool {
        let right = self.br.x;
        let bottom = self.br.y;
        point.x >= self.tl.x && point.x < right && point.y >= self.tl.y && point.y < bottom
    }

    /// Returns the intersection of this rectangle with another rectangle, if they overlap.
    ///
    /// If the rectangles do not overlap, returns `None`.
    pub fn intersect(self, other: Rect) -> Option<Rect> {
        let x1 = self.tl.x.max(other.tl.x);
        let y1 = self.tl.y.max(other.tl.y);
        let x2 = self.br.x.min(other.br.x);
        let y2 = self.br.y.min(other.br.y);

        if x1 < x2 && y1 < y2 {
            Some(Rect {
                tl: Point::new(x1, y1),
                br: Point::new(x2, y2),
            })
        } else {
            None
        }
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
pub trait DrawTarget: Send + Sync {
    /// Returns information about the framebuffer, including its dimensions and pixel format.
    fn info(&self) -> FbInfo;

    /// Fills the specified rectangle with the given color.
    fn fill_rect(&mut self, rect: Rect, color: u32);

    /// Fills the specified rectangle with pixel data from the provided source slice.
    ///
    /// The source slice should contain pixel data in the same format as the framebuffer.
    fn blit(&mut self, rect: Rect, src: &[u32]);

    /// Copies a rectangular region of pixels from one location to another within the framebuffer.
    fn copy_rect(&mut self, src: Rect, dst: Point);

    /// Flushes pending drawing operations affecting the specified region to the framebuffer.
    fn flush(&mut self, damage: Rect);
}

/// Represents the framebuffer information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FbInfo {
    /// The width of the framebuffer in pixels.
    pub width: usize,
    /// The height of the framebuffer in pixels.
    pub height: usize,
    /// The number of pixels per scanline.
    ///
    /// This is at least [`width`](Self::width), but may be larger when scanlines are padded.
    /// The pixel at `(x, y)` lives at index `y * stride + x`.
    pub stride: usize,
}

impl FbInfo {
    /// Returns the rectangle representing the entire framebuffer.
    pub fn rect(&self) -> Rect {
        Rect::from_size(Point::ZERO, self.width as i32, self.height as i32)
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

    /// Returns the area of the framebuffer as a rectangle.
    pub fn rect(&self) -> Rect {
        self.target.info().rect()
    }

    /// Flushes any pending drawing operations to the framebuffer.
    pub fn flush(&mut self) {
        self.target.flush(self.rect());
    }

    /// Clears the framebuffer with the specified color.
    pub fn clear(&mut self, color: u32) {
        self.target.fill_rect(self.rect(), color);
    }

    /// Draws a pixel at the specified coordinates with the given color.
    pub fn draw_pixel(&mut self, px: Pixel) {
        self.target.blit(Rect::from_size(px.0, 1, 1), &[px.1]);
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

impl<T: DrawTarget + ?Sized> DrawTarget for Box<T> {
    fn info(&self) -> FbInfo {
        self.as_ref().info()
    }

    fn fill_rect(&mut self, rect: Rect, color: u32) {
        self.as_mut().fill_rect(rect, color);
    }

    fn blit(&mut self, rect: Rect, src: &[u32]) {
        self.as_mut().blit(rect, src);
    }

    fn copy_rect(&mut self, src: Rect, dst: Point) {
        self.as_mut().copy_rect(src, dst);
    }

    fn flush(&mut self, damage: Rect) {
        self.as_mut().flush(damage);
    }
}

impl<T: DrawTarget + ?Sized> DrawTarget for &mut T {
    fn info(&self) -> FbInfo {
        (**self).info()
    }

    fn fill_rect(&mut self, rect: Rect, color: u32) {
        (**self).fill_rect(rect, color);
    }

    fn blit(&mut self, rect: Rect, src: &[u32]) {
        (**self).blit(rect, src);
    }

    fn copy_rect(&mut self, src: Rect, dst: Point) {
        (**self).copy_rect(src, dst);
    }

    fn flush(&mut self, damage: Rect) {
        (**self).flush(damage);
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
    pub fn draw_glyph(&self, target: &mut dyn DrawTarget, p: Point, ch: u8, color: u32) {
        debug_assert_eq!(
            Self::CHAR_WIDTH,
            8,
            "Only 8 pixel width characters are supported"
        );

        if let Some(glyph) = self.glyph(ch) {
            let mut buf = [0u32; 8 * 16]; // Buffer for a single glyph (8x16)

            for (row, &row_data) in glyph.iter().enumerate() {
                for col in 0..Self::CHAR_WIDTH {
                    if (row_data >> (7 - col)) & 1 == 1 {
                        buf[row * Self::CHAR_WIDTH + col] = color;
                    }
                }
            }

            target.blit(
                Rect::from_size(p, Self::CHAR_WIDTH as i32, self.char_height as i32),
                &buf,
            );
        }
    }
}

static FRAMEBUFFER: Once<IrqSpinLock<Framebuffer<Box<dyn DrawTarget>>>> = Once::new();

/// Registers a framebuffer device with the kernel.
pub fn register(target: Box<dyn DrawTarget>) {
    FRAMEBUFFER.call_once(|| IrqSpinLock::new(Framebuffer::new(target)));
}

/// Returns a reference to the registered framebuffer, if any.
pub fn get() -> Option<&'static IrqSpinLock<Framebuffer<Box<dyn DrawTarget>>>> {
    FRAMEBUFFER.get()
}
