//! Layout orientation transform (core algorithms stay top-to-bottom).
//!
//! [`Orientation`] maps world coordinates, sizes, and port sides into the
//! algorithm's canonical TB frame and back ([`Orientation::to_tb_point`] /
//! [`Orientation::from_tb_point`], …). Four directions are one pair of exact
//! coordinate swaps/negations — never four copies of the algorithm. All
//! transforms are bit-exact involutive round trips.
//!
//! Chirality: `Bt` (mirror y) and `Lr` (transpose `(y, x)`) are
//! *reflections* — they flip handedness (clockwise ↔ counter-clockwise) —
//! while `Rl` is a proper rotation. Coordinates, sizes, and sides always
//! round-trip exactly; only semantics that reference turn direction (e.g.
//! "label on the left of travel direction") mirror under `Bt`/`Lr`.

/// Minimal 2-D point for orientation mapping (no `plotgram-model` dependency).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Width/height pair; Lr/Rl swap them when entering the TB frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

/// Node/port side in the respective coordinate frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    North,
    East,
    South,
    West,
}

/// Layout flow direction. `Tb` is the canonical algorithm frame; the other
/// three are expressed as exact transforms into/out of it.
///
/// Point transforms (`to_tb_point`), chosen so the flow direction always
/// maps onto TB's `+y`:
///
/// | Orientation | to_tb      | from_tb    |
/// |-------------|------------|------------|
/// | `Tb`        | `( x,  y)` | `( x,  y)` |
/// | `Bt`        | `( x, -y)` | `( x, -y)` |
/// | `Lr`        | `( y,  x)` | `( y,  x)` |
/// | `Rl`        | `( y, -x)` | `(-y,  x)` |
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Orientation {
    /// Top → bottom (canonical).
    Tb,
    /// Bottom → top.
    Bt,
    /// Left → right.
    Lr,
    /// Right → left.
    Rl,
}

impl Orientation {
    /// World frame → algorithm (TB) frame.
    pub fn to_tb_point(self, p: Point) -> Point {
        match self {
            Orientation::Tb => p,
            Orientation::Bt => Point::new(p.x, -p.y),
            Orientation::Lr => Point::new(p.y, p.x),
            Orientation::Rl => Point::new(p.y, -p.x),
        }
    }

    /// Algorithm (TB) frame → world frame. Exact inverse of [`Self::to_tb_point`].
    pub fn from_tb_point(self, p: Point) -> Point {
        match self {
            Orientation::Tb => p,
            Orientation::Bt => Point::new(p.x, -p.y),
            Orientation::Lr => Point::new(p.y, p.x),
            Orientation::Rl => Point::new(-p.y, p.x),
        }
    }

    /// World size → the size the TB algorithm sees (Lr/Rl swap axes).
    pub fn to_tb_size(self, s: Size) -> Size {
        match self {
            Orientation::Tb | Orientation::Bt => s,
            Orientation::Lr | Orientation::Rl => Size::new(s.height, s.width),
        }
    }

    /// TB-frame size → world size. Its own inverse.
    pub fn from_tb_size(self, s: Size) -> Size {
        self.to_tb_size(s)
    }

    /// World side → TB-frame side, derived by applying the point transform
    /// to the side's outward direction vector:
    /// North=(0,-1) East=(1,0) South=(0,1) West=(-1,0).
    ///
    /// E.g. Lr maps (x,y)→(y,x): East (1,0) → (0,1) = South.
    pub fn to_tb_side(self, side: Side) -> Side {
        match self {
            Orientation::Tb => side,
            // (x,-y): North↔South, East/West fixed.
            Orientation::Bt => match side {
                Side::North => Side::South,
                Side::South => Side::North,
                s => s,
            },
            // (y,x): North↔West, East↔South.
            Orientation::Lr => match side {
                Side::North => Side::West,
                Side::West => Side::North,
                Side::East => Side::South,
                Side::South => Side::East,
            },
            // (y,-x): North→(−1,0)=West… full cycle North→West→South→East→North.
            Orientation::Rl => match side {
                Side::North => Side::West,
                Side::West => Side::South,
                Side::South => Side::East,
                Side::East => Side::North,
            },
        }
    }

    /// TB-frame side → world side. Exact inverse of [`Self::to_tb_side`].
    pub fn from_tb_side(self, side: Side) -> Side {
        match self {
            Orientation::Tb => side,
            Orientation::Bt => Orientation::Bt.to_tb_side(side), // involution
            Orientation::Lr => Orientation::Lr.to_tb_side(side), // involution
            // Inverse cycle of Rl's to_tb.
            Orientation::Rl => match side {
                Side::West => Side::North,
                Side::South => Side::West,
                Side::East => Side::South,
                Side::North => Side::East,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ORIENTATIONS: [Orientation; 4] = [
        Orientation::Tb,
        Orientation::Bt,
        Orientation::Lr,
        Orientation::Rl,
    ];
    const ALL_SIDES: [Side; 4] = [Side::North, Side::East, Side::South, Side::West];

    #[test]
    fn point_round_trip_is_exact() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(1.5, -2.25),
            Point::new(-317.0, 42.0),
            Point::new(f64::MIN_POSITIVE, -f64::MIN_POSITIVE),
            Point::new(1e300, -1e-300),
        ];
        for o in ALL_ORIENTATIONS {
            for p in points {
                let rt = o.from_tb_point(o.to_tb_point(p));
                // Exact bit-level equality: transforms only swap/negate.
                assert_eq!(
                    rt.x.to_bits(),
                    p.x.to_bits(),
                    "{o:?} x round trip for {p:?}"
                );
                assert_eq!(
                    rt.y.to_bits(),
                    p.y.to_bits(),
                    "{o:?} y round trip for {p:?}"
                );
                let rt2 = o.to_tb_point(o.from_tb_point(p));
                assert_eq!(
                    (rt2.x.to_bits(), rt2.y.to_bits()),
                    (p.x.to_bits(), p.y.to_bits())
                );
            }
        }
    }

    #[test]
    fn flow_direction_maps_to_tb_plus_y() {
        // Each orientation's flow unit vector must land on (0, 1) in TB.
        let cases = [
            (Orientation::Tb, Point::new(0.0, 1.0)),
            (Orientation::Bt, Point::new(0.0, -1.0)),
            (Orientation::Lr, Point::new(1.0, 0.0)),
            (Orientation::Rl, Point::new(-1.0, 0.0)),
        ];
        for (o, flow) in cases {
            let tb = o.to_tb_point(flow);
            assert_eq!((tb.x, tb.y), (0.0, 1.0), "{o:?} flow vector must map to +y");
        }
    }

    #[test]
    fn size_swap_rules() {
        let s = Size::new(120.0, 40.0);
        for o in [Orientation::Tb, Orientation::Bt] {
            assert_eq!(o.to_tb_size(s), s, "{o:?} keeps size");
        }
        for o in [Orientation::Lr, Orientation::Rl] {
            assert_eq!(o.to_tb_size(s), Size::new(40.0, 120.0), "{o:?} swaps w/h");
            assert_eq!(o.from_tb_size(o.to_tb_size(s)), s, "{o:?} size round trip");
        }
    }

    #[test]
    fn side_full_table_and_inverse() {
        // Expected to_tb_side table, anchored to the direction-vector
        // derivation in the impl docs.
        let expect: [(Orientation, [(Side, Side); 4]); 4] = [
            (
                Orientation::Tb,
                [
                    (Side::North, Side::North),
                    (Side::East, Side::East),
                    (Side::South, Side::South),
                    (Side::West, Side::West),
                ],
            ),
            (
                Orientation::Bt,
                [
                    (Side::North, Side::South),
                    (Side::East, Side::East),
                    (Side::South, Side::North),
                    (Side::West, Side::West),
                ],
            ),
            (
                Orientation::Lr,
                [
                    (Side::North, Side::West),
                    (Side::East, Side::South),
                    (Side::South, Side::East),
                    (Side::West, Side::North),
                ],
            ),
            (
                Orientation::Rl,
                [
                    (Side::North, Side::West),
                    (Side::East, Side::North),
                    (Side::South, Side::East),
                    (Side::West, Side::South),
                ],
            ),
        ];
        for (o, table) in expect {
            for (world, tb) in table {
                assert_eq!(o.to_tb_side(world), tb, "{o:?} to_tb_side({world:?})");
                assert_eq!(o.from_tb_side(tb), world, "{o:?} from_tb_side({tb:?})");
            }
        }
        // Round trips both ways, exhaustively.
        for o in ALL_ORIENTATIONS {
            for s in ALL_SIDES {
                assert_eq!(o.from_tb_side(o.to_tb_side(s)), s);
                assert_eq!(o.to_tb_side(o.from_tb_side(s)), s);
            }
        }
    }
}
