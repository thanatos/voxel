use std::fmt;

pub mod projection;
mod screen;
pub mod transformations;

pub use screen::screen_matrix;

/// A 4x4 matrix of single-precision floats.
// repr(C) because vulkano will transmit it to the GPU via memcpy().
#[repr(C)]
#[derive(Clone)]
pub struct Matrix {
    /// Column-major 2D matrix data
    data: [[f32; 4]; 4],
}

impl Matrix {
    pub fn identity() -> Matrix {
        Matrix::from([
            [1., 0., 0., 0.],
            [0., 1., 0., 0.],
            [0., 0., 1., 0.],
            [0., 0., 0., 1.],
        ])
    }

    /*
    fn transpose(mut self) -> Matrix {
        std::mem::swap(&mut self.data[1][0], &mut self.data[0][1]);

        std::mem::swap(&mut self.data[2][0], &mut self.data[0][2]);
        std::mem::swap(&mut self.data[2][1], &mut self.data[1][2]);

        std::mem::swap(&mut self.data[3][0], &mut self.data[0][3]);
        std::mem::swap(&mut self.data[3][1], &mut self.data[1][3]);
        std::mem::swap(&mut self.data[3][2], &mut self.data[2][3]);

        self
    }
    */
}

impl From<[[f32; 4]; 4]> for Matrix {
    fn from(matrix: [[f32; 4]; 4]) -> Matrix {
        Matrix {
            data: [
                [matrix[0][0], matrix[1][0], matrix[2][0], matrix[3][0]],
                [matrix[0][1], matrix[1][1], matrix[2][1], matrix[3][1]],
                [matrix[0][2], matrix[1][2], matrix[2][2], matrix[3][2]],
                [matrix[0][3], matrix[1][3], matrix[2][3], matrix[3][3]],
            ],
        }
    }
}

impl std::ops::Mul for Matrix {
    type Output = Matrix;

    fn mul(self, rhs: Matrix) -> Matrix {
        let mut output = Matrix {
            data: [[0.0; 4], [0.0; 4], [0.0; 4], [0.0; 4]],
        };
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    output.data[j][i] += self.data[k][i] * rhs.data[j][k];
                }
            }
        }
        output
    }
}

impl PartialEq for Matrix {
    fn eq(&self, other: &Matrix) -> bool {
        for c in 0..4 {
            for r in 0..4 {
                if self.data[c][r] != other.data[c][r] {
                    return false;
                }
            }
        }
        true
    }
}

impl Eq for Matrix {}

impl fmt::Debug for Matrix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            writeln!(f, "Matrix {{ data: [")?;
            for r in 0..4 {
                writeln!(f, "\t{:?}", self.data[r])?;
            }
            writeln!(f, "]}}")
        } else {
            write!(f, "Matrix {{ data: {:?} }}", &self.data)
        }
    }
}

/// A 3D vertex.
// repr(C) because vulkano will transmit it to the GPU via memcpy().
#[repr(C)]
#[derive(Clone, Debug)]
pub struct Vertex3d {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vertex3d {
    pub fn new(x: f32, y: f32, z: f32) -> Vertex3d {
        Vertex3d { x, y, z }
    }
}

impl PartialEq for Vertex3d {
    fn eq(&self, other: &Vertex3d) -> bool {
        self.x == other.x && self.y == other.y && self.x == other.z
    }
}

impl Eq for Vertex3d {}

impl std::ops::Mul<Vertex3d> for Matrix {
    type Output = Vertex3d;

    fn mul(self, rhs: Vertex3d) -> Vertex3d {
        let x = self.data[0][0] * rhs.x
            + self.data[1][0] * rhs.y
            + self.data[2][0] * rhs.z
            + self.data[3][0];
        let y = self.data[0][1] * rhs.x
            + self.data[1][1] * rhs.y
            + self.data[2][1] * rhs.z
            + self.data[3][1];
        let z = self.data[0][2] * rhs.x
            + self.data[1][2] * rhs.y
            + self.data[2][2] * rhs.z
            + self.data[3][2];
        Vertex3d { x, y, z }
    }
}

/// A 4D vertex.
// repr(C) because vulkano will transmit it to the GPU via memcpy().
#[repr(C)]
#[derive(Clone, Debug)]
pub struct Vertex4d {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Vertex4d {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Vertex4d {
        Vertex4d { x, y, z, w }
    }
}

impl PartialEq for Vertex4d {
    fn eq(&self, other: &Vertex4d) -> bool {
        self.x == other.x && self.y == other.y && self.x == other.z && self.w == other.w
    }
}

impl Eq for Vertex4d {}

impl std::ops::Mul<Vertex4d> for Matrix {
    type Output = Vertex4d;

    fn mul(self, rhs: Vertex4d) -> Vertex4d {
        let x = self.data[0][0] * rhs.x
            + self.data[1][0] * rhs.y
            + self.data[2][0] * rhs.z
            + self.data[3][0] * rhs.w;
        let y = self.data[0][1] * rhs.x
            + self.data[1][1] * rhs.y
            + self.data[2][1] * rhs.z
            + self.data[3][1] * rhs.w;
        let z = self.data[0][2] * rhs.x
            + self.data[1][2] * rhs.y
            + self.data[2][2] * rhs.z
            + self.data[3][2] * rhs.w;
        let w = self.data[0][3] * rhs.x
            + self.data[1][3] * rhs.y
            + self.data[2][3] * rhs.z
            + self.data[3][3] * rhs.w;
        Vertex4d { x, y, z, w }
    }
}

#[cfg(test)]
mod tests {
    use super::Matrix;

    #[test]
    fn test_matrix_debug() {
        let a = Matrix::from([
            [1., 2., 3., 4.],
            [0., 1., 0., 5.],
            [0., 0., 1., 6.],
            [0., 0., 0., 7.],
        ]);
        println!("{:#?}", a);
    }

    #[test]
    fn test_matrix_mul() {
        // Test vector from: https://opentk.net/learn/chapter1/6-transformations.html
        let a = Matrix::from([
            [4., 2., 0., 0.],
            [0., 8., 1., 0.],
            [0., 1., 0., 0.],
            [0., 0., 0., 0.],
        ]);
        let b = Matrix::from([
            [4., 2., 1., 0.],
            [2., 0., 4., 0.],
            [9., 4., 2., 0.],
            [0., 0., 0., 0.],
        ]);
        let c = a * b;
        let expected = Matrix::from([
            [20., 8., 12., 0.],
            [25., 4., 34., 0.],
            [2., 0., 4., 0.],
            [0., 0., 0., 0.],
        ]);
        assert!(c == expected);
    }
}
