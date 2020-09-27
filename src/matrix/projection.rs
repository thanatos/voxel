use crate::matrix::Matrix;

// Useful references:
//   https://www.scratchapixel.com/lessons/3d-basic-rendering/perspective-and-orthographic-projection-matrix/building-basic-perspective-projection-matrix
//   glFrustum's documentation
//   http://www.alexisbreust.fr/2018-game-engine-frustum-culling.html
//   https://ksimek.github.io/2013/06/03/calibrated_cameras_in_opengl/

/// Builds a perspective projection matrix.
pub fn perspective(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) -> Matrix {
    let a = (right + left) / (right - left);
    let b = (top + bottom) / (top - bottom);
    let c = -(far + near) / (far - near);
    let d = -2. * far * near / (far - near);
    let proj = Matrix::from([
        [2. * near / (right - left), 0., a, 0.],
        [0., 2. * near / (top - bottom), b, 0.],
        [0., 0., c, d],
        [0., 0., -1., 0.],
    ]);
    let correction = Matrix::from([
        [1., 0., 0., 0.],
        [0., -1., 0., 0.],
        [0., 0., 0.5, 0.5],
        [0., 0., 0., 1.],
    ]);
    proj * correction
}

pub fn perspective_fov_both(fov_horizontal: f32, fov_vertical: f32, near: f32, far: f32) -> Matrix {
    // Notes:
    //
    // For both the horizontal & the vertical, we need to compute left/right, top/bottom.
    // In either case, this looks like this:
    //
    // A               C
    //  \             /
    //   \─────┬─────/ ← near clip plane
    //    \    │    /
    //     \   │   /
    //      \  │  /
    //       \ │ /
    //        \│/
    //         B
    //
    // The angle formed by ABC is the FoV. From B to the near clip plane is the value in `near`.
    // ½ of the FoV is a right triangle; tan(½ * fov) = opposite / adjacent. Adjacent, here, is
    // `near`, and opposite is ½ the width of the near clip plane, or `0.5 * (right - left)` (for
    // the horizontal case).
    //
    // Then,
    //
    //     1 / tan(fov / 2)
    //   = 1 / ((.5 * (right - left)) / near)
    //   = near / (.5 * (right - left))
    //   = 2 * near / (right - left)
    //
    // … which is the value in cell (0,0) in the normal `perspective` matrix. (And similar, for
    // (1, 1))
    //

    let right = (fov_horizontal / 2.).tan() * near;
    let top = (fov_vertical / 2.).tan() * near;
    perspective(-right, right, -top, top, near, far)
}

/// Builds a perspective transformation matrix given a vertical field of view and an aspect ratio
/// (_width_ / _height_).
pub fn perspective_fov(fov_vertical: f32, aspect_ratio: f32, near: f32, far: f32) -> Matrix {
    let top = (fov_vertical / 2.).tan() * near;
    let right = top * aspect_ratio;
    perspective(-right, right, -top, top, near, far)
}
