use crate::matrix::{self, Matrix};

pub fn camera(x: f32, y: f32, z: f32, rotation_horizontal: f32, rotation_vertical: f32) -> Matrix {
    let translation = matrix::transformations::translate(-x, -y, -z);
    let rotation = matrix::transformations::rotate_x(rotation_vertical)
        * matrix::transformations::rotate_y(rotation_horizontal);

    rotation * translation
}
