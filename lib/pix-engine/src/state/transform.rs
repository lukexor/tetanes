use crate::state::StateData;
use image::{DynamicImage, GenericImage, GenericImageView};

type Matrix = [[f32; 3]; 3];

// Represents an Affine Transformation
pub struct Transform {
    // [row][col] or [y][x]
    source: Matrix,    // Current matrix
    target: Matrix,    // Temporary matrix for multiplication
    transform: Matrix, // Current transformation
    inverted: Matrix,  // Inverted source matrix
    dirty: bool,
}

impl Transform {
    /// Create a new transformation object
    pub fn new() -> Self {
        Self {
            source: Self::identity(),
            target: Self::identity(),
            transform: Self::identity(),
            inverted: Self::identity(),
            dirty: false,
        }
    }

    /// Reset the transformation back to unit
    pub fn reset(&mut self) {
        self.target = Self::identity();
        self.source = Self::identity();
        self.dirty = true;
    }

    /// Translate by (ox, oy)
    pub fn translate(&mut self, ox: f32, oy: f32) {
        self.transform = [[1.0, 0.0, ox], [0.0, 1.0, oy], [0.0, 0.0, 1.0]];
        self.multiply();
    }

    /// Rotate by theta (in radians)
    pub fn rotate(&mut self, theta: f32) {
        let cos = (-theta).cos();
        let sin = (-theta).sin();
        self.transform = [[cos, sin, 0.0], [-sin, cos, 0.0], [0.0, 0.0, 1.0]];
        self.multiply();
    }

    /// Scale x by sx and y by sy
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.transform = [[sx, 0.0, 0.0], [0.0, sy, 0.0], [0.0, 0.0, 1.0]];
        self.multiply();
    }

    /// Shear x by sx and y by sy
    pub fn shear(&mut self, sx: f32, sy: f32) {
        self.transform = [[1.0, sx, 0.0], [sy, 0.0, 0.0], [0.0, 0.0, 1.0]];
        self.multiply();
    }

    /// Transform into perspective at (ox, oy)
    pub fn perspective(&mut self, ox: f32, oy: f32) {
        self.transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [ox, oy, 1.0]];
        self.multiply();
    }
}

impl Transform {
    fn identity() -> Matrix {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    }

    fn multiply(&mut self) {
        for c in 0..3 {
            for r in 0..3 {
                self.target[r][c] = self.transform[r][0] * self.source[0][c]
                    + self.transform[r][1] * self.source[1][c]
                    + self.transform[r][2] * self.source[2][c];
            }
        }
        std::mem::swap(&mut self.target, &mut self.source);
        self.dirty = true;
    }

    fn forward(&self, x: f32, y: f32) -> (f32, f32) {
        let mut ox = x * self.source[0][0] + y * self.source[0][1] + self.source[0][2];
        let mut oy = x * self.source[1][0] + y * self.source[1][1] + self.source[1][2];
        let oz = x * self.source[2][0] + y * self.source[2][1] + self.source[2][2];
        if oz != 0.0 {
            ox /= oz;
            oy /= oz;
        }
        (ox, oy)
    }

    fn backward(&self, x: f32, y: f32) -> (f32, f32) {
        let mut ox = x * self.inverted[0][0] + y * self.inverted[0][1] + self.inverted[0][2];
        let mut oy = x * self.inverted[1][0] + y * self.inverted[1][1] + self.inverted[1][2];
        let oz = x * self.inverted[2][0] + y * self.inverted[2][1] + self.inverted[2][2];
        if oz != 0.0 {
            ox /= oz;
            oy /= oz;
        }
        (ox, oy)
    }

    #[rustfmt::skip]
    fn invert(&mut self) {
        if self.dirty {
            let det = self.source[0][0] * (self.source[1][1] * self.source[2][2] - self.source[2][1] * self.source[1][2])
                - self.source[0][1] * (self.source[1][0] * self.source[2][2] - self.source[1][2] * self.source[2][0])
                + self.source[0][2] * (self.source[1][0] * self.source[2][1] - self.source[1][1] * self.source[2][0]);

            let idet = 1.0 / det;
            self.inverted[0][0] = (self.source[1][1] * self.source[2][2] - self.source[2][1] * self.source[1][2]) * idet;
            self.inverted[0][1] = (self.source[0][2] * self.source[2][1] - self.source[0][1] * self.source[2][2]) * idet;
            self.inverted[0][2] = (self.source[0][1] * self.source[1][2] - self.source[0][2] * self.source[1][1]) * idet;
            self.inverted[1][0] = (self.source[1][2] * self.source[2][0] - self.source[1][0] * self.source[2][2]) * idet;
            self.inverted[1][1] = (self.source[0][0] * self.source[2][2] - self.source[0][2] * self.source[2][0]) * idet;
            self.inverted[1][2] = (self.source[1][0] * self.source[0][2] - self.source[0][0] * self.source[1][2]) * idet;
            self.inverted[2][0] = (self.source[1][0] * self.source[2][1] - self.source[2][0] * self.source[1][1]) * idet;
            self.inverted[2][1] = (self.source[2][0] * self.source[0][1] - self.source[0][0] * self.source[2][1]) * idet;
            self.inverted[2][2] = (self.source[0][0] * self.source[1][1] - self.source[1][0] * self.source[0][1]) * idet;
        }
    }
}

impl StateData {
    /// Draws a sprite using the transform matrix
    pub fn draw_transform(&mut self, transform: &mut Transform, sprite: &DynamicImage) {
        // Top Left pixel bounds
        let (px, py) = transform.forward(0.0, 0.0);
        let sx = px;
        let sy = py;
        let ex = px;
        let ey = py;

        // Bottom Right pixel bounds
        let (px, py) = transform.forward(sprite.width() as f32, sprite.height() as f32);
        let sx = sx.max(px);
        let sy = sy.max(py);
        let ex = ex.min(px);
        let ey = ey.min(py);

        // Bottom Left pixel bounds
        let (px, py) = transform.forward(0.0, sprite.height() as f32);
        let sx = sx.max(px);
        let sy = sy.max(py);
        let ex = ex.min(px);
        let ey = ey.min(py);

        // Top Right pixel bounds
        let (px, py) = transform.forward(sprite.width() as f32, 0.0);

        // Take final min/max and round up
        let mut sx = sx.max(px).ceil() as u32;
        let mut sy = sy.max(py).ceil() as u32;
        let mut ex = ex.min(px).ceil() as u32;
        let mut ey = ey.min(py).ceil() as u32;

        // Invert source if needed
        transform.invert();

        if ex < sx {
            std::mem::swap(&mut ex, &mut sx);
        }
        if ey < sy {
            std::mem::swap(&mut ey, &mut sy);
        }

        for x in sx..ex {
            for y in sy..ey {
                let (nx, ny) = transform.backward(x as f32, y as f32);
                let p = sprite.get_pixel(nx.ceil() as u32, ny.ceil() as u32);
                self.draw_color(x, y, p);
            }
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new()
    }
}
