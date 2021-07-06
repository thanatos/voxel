use std::convert::TryFrom;
use std::fmt;

/// Indicates a particular corner in an octree subdivision.
#[derive(Clone, Copy, Debug)]
pub enum SubCube {
    LowerSw,
    LowerSe,
    LowerNw,
    LowerNe,
    UpperSw,
    UpperSe,
    UpperNw,
    UpperNe,
}

impl SubCube {
    /// Each sub cube has a corresponding bit pattern; this decodes a `SubCube` from its bit
    /// pattern.
    pub(crate) fn from_bits(bits: u8) -> SubCube {
        use SubCube::*;

        match bits {
            0b000 => LowerSw,
            0b001 => LowerSe,
            0b010 => LowerNw,
            0b011 => LowerNe,
            0b100 => UpperSw,
            0b101 => UpperSe,
            0b110 => UpperNw,
            0b111 => UpperNe,
            _ => panic!("bit codes for SubCube are only 3 bits"),
        }
    }

    pub fn from_xyz(x: u8, y: u8, z: u8) -> Option<SubCube> {
        if 1 < x || 1 < y || 1 < z {
            None
        } else {
            let code = (y << 2)
                | (z << 1)
                | x;
            Some(Self::from_bits(code))
        }
    }

    pub(crate) fn to_bits(self) -> u8 {
        use SubCube::*;

        match self {
            LowerSw => 0b000,
            LowerSe => 0b001,
            LowerNw => 0b010,
            LowerNe => 0b011,
            UpperSw => 0b100,
            UpperSe => 0b101,
            UpperNw => 0b110,
            UpperNe => 0b111,
        }
    }

    pub(crate) fn next_sibling(self) -> Option<SubCube> {
        use SubCube::*;

        match self {
            LowerSw => Some(LowerSe),
            LowerSe => Some(LowerNw),
            LowerNw => Some(LowerNe),
            LowerNe => Some(UpperSw),
            UpperSw => Some(UpperSe),
            UpperSe => Some(UpperNw),
            UpperNw => Some(UpperNe),
            UpperNe => None,
        }
    }

    /// Returns an iterator that iterates through all possible `SubCube`s.
    pub fn all_sub_cubes() -> impl Iterator<Item = SubCube> {
        AllSubCubes(0)
    }
}

struct AllSubCubes(u8);

impl Iterator for AllSubCubes {
    type Item = SubCube;

    fn next(&mut self) -> Option<Self::Item> {
        match self.0 {
            0b000..=0b111 => {
                let sub_cube_code = SubCube::from_bits(self.0);
                self.0 += 1;
                Some(sub_cube_code)
            }
            _ => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = match self.0 {
            0b000..=0b111 => usize::from(8 - self.0),
            _ => 0,
        };
        (remaining, Some(remaining))
    }
}

impl std::iter::FusedIterator for AllSubCubes {}
impl std::iter::ExactSizeIterator for AllSubCubes {}

/// A cube within an octree.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct LocationCode(u32);

impl LocationCode {
    pub const ROOT: LocationCode = LocationCode(1);

    pub fn push_sub_cube(self, sub_cube_code: SubCube) -> LocationCode {
        if self.0 & 0b11100000_00000000_00000000_00000000 != 0 {
            panic!("Location code too small; cannot subdivide further.");
        }
        LocationCode((self.0 << 3) | u32::from(sub_cube_code.to_bits()))
    }

    pub fn sub_cube(self) -> Option<(LocationCode, SubCube)> {
        if self.0 == 1 {
            None
        } else {
            let sub_cube = SubCube::from_bits(u8::try_from(self.0 & 0b111).unwrap());
            let parent = LocationCode(self.0 >> 3);
            Some((parent, sub_cube))
        }
    }

    fn from_root_to_here_impl(self) -> FromRootToLocationCode {
        // We always have a leading 1, so we can count zeros to figure out the bits that form the
        // code:
        // (+1 for the leading 1)
        let bits_not_part_of_code = u8::try_from(self.0.leading_zeros() + 1).unwrap();
        // Subtract from the bit length of the variable
        let code_bits =
            u8::try_from(std::mem::size_of::<u32>() * 8).unwrap() - bits_not_part_of_code;
        // Divide to get number of SubCubes
        let shifts = code_bits / 3;
        // +1 for the root
        FromRootToLocationCode(self, shifts + 1)
    }

    /// Iterate through the location codes from the `ROOT` code to this one, inclusively. (That is,
    /// the first item emitted by the iterator will be `ROOT`, and the final item will be `self`.)
    pub fn from_root_to_here(self) -> impl Iterator<Item = LocationCode> {
        self.from_root_to_here_impl()
    }

    /// Iterate through the location codes from the `ROOT` code down to just before this one.
    pub fn from_root_to_just_above_here(self) -> impl Iterator<Item = LocationCode> {
        match self.containing_cube() {
            Some(cc) => cc.from_root_to_here_impl(),
            // We're at the root; this is equivalent to an empty iterator.
            None => FromRootToLocationCode(LocationCode::ROOT, 0),
        }
    }

    pub fn containing_cube(self) -> Option<LocationCode> {
        if self.0 == 1 {
            None
        } else {
            Some(LocationCode(self.0 >> 3))
        }
    }

    fn to_details(mut self, mut size: u16) -> ((u32, u32, u32), u16) {
        let mut x = 0;
        let mut y = 0;
        let mut z = 0;

        while self.0 != 1 {
            x <<= 1;
            y <<= 1;
            z <<= 1;
            size >>= 1;

            let (parent, sub_cube_code) = self.sub_cube().unwrap();
            self = parent;

            match sub_cube_code {
                SubCube::LowerSw => (),
                SubCube::LowerSe => x += 1,
                SubCube::LowerNw => z += 1,
                SubCube::LowerNe => {
                    x += 1;
                    z += 1;
                }
                SubCube::UpperSw => y += 1,
                SubCube::UpperSe => {
                    x += 1;
                    y += 1;
                }
                SubCube::UpperNw => {
                    y += 1;
                    z += 1;
                }
                SubCube::UpperNe => {
                    x += 1;
                    y += 1;
                    z += 1;
                }
            }
        }

        ((x, y, z), size)
    }
}

impl fmt::Debug for LocationCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LocationCode({:?} /* ROOT", self.0)?;
        if self.0 == 1 {
            f.write_str(" */)")
        } else {
            let mut cubes = Vec::new();
            let mut loc = *self;
            while let Some((parent, sub_cube)) = loc.sub_cube() {
                loc = parent;
                cubes.push(sub_cube);
            }
            cubes.reverse();
            for sub_cube in cubes {
                f.write_str(", ")?;
                let string_version = match sub_cube {
                    SubCube::LowerSw => "↓SW",
                    SubCube::LowerSe => "↓SE",
                    SubCube::LowerNw => "↓NW",
                    SubCube::LowerNe => "↓NE",
                    SubCube::UpperSw => "↑SW",
                    SubCube::UpperSe => "↑SE",
                    SubCube::UpperNw => "↑NW",
                    SubCube::UpperNe => "↑NE",
                };
                f.write_str(string_version)?;
            }
            f.write_str(" */)")
        }
    }
}

struct FromRootToLocationCode(LocationCode, u8);

impl Iterator for FromRootToLocationCode {
    type Item = LocationCode;

    fn next(&mut self) -> Option<Self::Item> {
        if self.1 == 0 {
            None
        } else {
            self.1 -= 1;
            let code = LocationCode((self.0).0 >> (self.1 * 3));
            Some(code)
        }
    }
}

impl std::iter::FusedIterator for FromRootToLocationCode {}

#[cfg(test)]
mod tests {
    use super::{LocationCode, SubCube};

    #[test]
    fn test_location_code_from_root_to_here() {
        let loc_code = LocationCode::ROOT;
        let items = loc_code.from_root_to_here().collect::<Vec<_>>();
        assert!(items == &[LocationCode::ROOT]);

        let loc_code = LocationCode::ROOT
            .push_sub_cube(SubCube::LowerSe)
            .push_sub_cube(SubCube::UpperNw);

        let items = loc_code.from_root_to_here().collect::<Vec<_>>();
        let expect = &[
            LocationCode::ROOT,
            LocationCode::ROOT.push_sub_cube(SubCube::LowerSe),
            LocationCode::ROOT
                .push_sub_cube(SubCube::LowerSe)
                .push_sub_cube(SubCube::UpperNw),
        ];
        println!("{:?}", items);
        println!("{:?}", expect);
        assert!(items == expect);
    }
    
    #[test]
    fn test_location_code_from_root_to_just_above_here() {
        let sub_area = LocationCode::ROOT.push_sub_cube(SubCube::LowerNe);
        let items = sub_area.from_root_to_just_above_here().collect::<Vec<_>>();
        assert!(items == &[LocationCode::ROOT]);

        let items = LocationCode::ROOT.from_root_to_just_above_here().collect::<Vec<_>>();
        assert!(items == &[]);
    }
}
