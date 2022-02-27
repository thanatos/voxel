use super::Matrix;

pub fn screen_matrix(width: u32, height: u32) -> Matrix {
    // Scale from screen coordinates to (0, 0) to (2, 2)
    let scaling_matrix = Matrix::from([
        [1.0 / (width as f32) * 2., 0., 0., 0.],
        [0., 1.0 / (height as f32) * 2., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ]);
    // Translate to (-1, -1) to (1, 1), Vulkan NDC space:
    let translation_matrix = Matrix::from([
        [1., 0., 0., -1.],
        [0., 1., 0., -1.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ]);
    translation_matrix * scaling_matrix
}

#[cfg(test)]
mod tests {
    use super::super::{Matrix, Vertex3d, Vertex4d};

    #[test]
    fn test_screen_matrix() {
        let mat = super::screen_matrix(800, 600);
        let coord = Vertex3d {
            x: 400.,
            y: 300.,
            z: 0.,
        };
        let result = mat * coord;
        println!("result = {:?}", result);
        assert!(result.x == 0.5);
        assert!(result.y == 0.5);
        assert!(result.z == 0.);

        let mat = super::screen_matrix(800, 600);
        let coord = Vertex4d {
            x: 400.,
            y: 300.,
            z: 0.,
            w: 1.,
        };
        let result = mat * coord;
        println!("result = {:?}", result);
        assert!(result.x == 0.5);
        assert!(result.y == 0.5);
        assert!(result.z == 0.);
        assert!(result.w == 1.);
    }
}
