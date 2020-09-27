use crate::matrix::Matrix;

// https://www.cs.cornell.edu/courses/cs4620/2010fa/lectures/03transforms3d.pdf

pub fn translate(x: f32, y: f32, z: f32) -> Matrix {
    Matrix::from([
        [1., 0., 0., x],
        [0., 1., 0., y],
        [0., 0., 1., z],
        [0., 0., 0., 1.],
    ])
}

pub fn rotate_x(by: f32) -> Matrix {
    Matrix::from([
        [1., 0., 0., 0.],
        [0., by.cos(), -by.sin(), 0.],
        [0., by.sin(), by.cos(), 0.],
        [0., 0., 0., 1.],
    ])
}

pub fn rotate_y(by: f32) -> Matrix {
    Matrix::from([
        [by.cos(), 0., by.sin(), 0.],
        [0., 1., 0., 0.],
        [-by.sin(), 0., by.cos(), 0.],
        [0., 0., 0., 1.],
    ])
}

pub fn rotate_z(by: f32) -> Matrix {
    Matrix::from([
        [by.cos(), -by.sin(), 0., 0.],
        [by.sin(), by.cos(), 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ])
}
