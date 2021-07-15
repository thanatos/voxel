use std::fmt;

pub mod projection;
pub mod transformations;

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

#[cfg(test)]
mod tests {
    use super::Matrix;

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
