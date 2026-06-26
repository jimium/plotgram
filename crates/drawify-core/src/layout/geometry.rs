//! 布局几何原语
//!
//! 提供 Point / Rect / Axis 统一抽象，消除 (f64,f64) 元组的语义模糊，
//! 并通过 Axis 统一水平/垂直方向的镜像代码。

use std::fmt;
use serde::Serialize;

/// 二维点
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0)
    }

    pub fn add(self, other: Point) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }

    pub fn sub(self, other: Point) -> Self {
        Self::new(self.x - other.x, self.y - other.y)
    }

    pub fn scale(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s)
    }

    pub fn dot(self, other: Point) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn distance_to(self, other: Point) -> f64 {
        (self - other).length()
    }

    pub fn lerp(self, other: Point, t: f64) -> Point {
        self + (other - self).scale(t)
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:.1}, {:.1})", self.x, self.y)
    }
}

impl std::ops::Add for Point {
    type Output = Point;
    fn add(self, rhs: Point) -> Self::Output {
        self.add(rhs)
    }
}

impl std::ops::Sub for Point {
    type Output = Point;
    fn sub(self, rhs: Point) -> Self::Output {
        self.sub(rhs)
    }
}

impl std::ops::Mul<f64> for Point {
    type Output = Point;
    fn mul(self, rhs: f64) -> Self::Output {
        self.scale(rhs)
    }
}

impl From<(f64, f64)> for Point {
    fn from((x, y): (f64, f64)) -> Self {
        Self::new(x, y)
    }
}

impl From<Point> for (f64, f64) {
    fn from(p: Point) -> Self {
        (p.x, p.y)
    }
}

/// 轴对齐矩形
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_points(min: Point, max: Point) -> Self {
        Self::new(min.x, min.y, max.x - min.x, max.y - min.y)
    }

    pub fn left(&self) -> f64 {
        self.x
    }
    pub fn top(&self) -> f64 {
        self.y
    }
    pub fn right(&self) -> f64 {
        self.x + self.width
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
    pub fn top_left(&self) -> Point {
        Point::new(self.x, self.y)
    }
    pub fn top_right(&self) -> Point {
        Point::new(self.right(), self.y)
    }
    pub fn bottom_left(&self) -> Point {
        Point::new(self.x, self.bottom())
    }
    pub fn bottom_right(&self) -> Point {
        Point::new(self.right(), self.bottom())
    }

    pub fn size(&self) -> (f64, f64) {
        (self.width, self.height)
    }

    pub fn expanded(&self, pad: f64) -> Self {
        Self::new(
            self.x - pad,
            self.y - pad,
            self.width + 2.0 * pad,
            self.height + 2.0 * pad,
        )
    }

    pub fn translate(&self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    pub fn contains_point(&self, p: Point, pad: f64) -> bool {
        let r = self.expanded(pad);
        p.x >= r.x && p.x <= r.right() && p.y >= r.y && p.y <= r.bottom()
    }

    pub fn contains_point_strict(&self, p: Point, eps: f64) -> bool {
        p.x > self.x + eps && p.x < self.right() - eps && p.y > self.y + eps && p.y < self.bottom() - eps
    }

    pub fn intersects_rect(&self, other: &Rect, pad: f64) -> bool {
        let a = self.expanded(pad);
        let b = other.expanded(pad);
        a.x < b.right() && a.right() > b.x && a.y < b.bottom() && a.bottom() > b.y
    }

    pub fn intersects_segment(&self, a: Point, b: Point, pad: f64) -> bool {
        let r = self.expanded(pad);
        if r.contains_point(a, 0.0) || r.contains_point(b, 0.0) {
            return true;
        }
        segment_intersects_segment(a, b, r.top_left(), r.top_right())
            || segment_intersects_segment(a, b, r.top_right(), r.bottom_right())
            || segment_intersects_segment(a, b, r.bottom_right(), r.bottom_left())
            || segment_intersects_segment(a, b, r.bottom_left(), r.top_left())
    }

    pub fn segment_crosses_interior(&self, a: Point, b: Point, eps: f64) -> bool {
        if (a.x < self.x - eps && b.x < self.x - eps)
            || (a.x > self.right() + eps && b.x > self.right() + eps)
            || (a.y < self.y - eps && b.y < self.y - eps)
            || (a.y > self.bottom() + eps && b.y > self.bottom() + eps)
        {
            return false;
        }
        let a_in = self.contains_point_strict(a, eps);
        let b_in = self.contains_point_strict(b, eps);
        if a_in && b_in {
            return true;
        }
        let corners = [self.top_left(), self.top_right(), self.bottom_right(), self.bottom_left()];
        for i in 0..4 {
            let p1 = corners[i];
            let p2 = corners[(i + 1) % 4];
            if segments_cross(a, b, p1, p2, eps) {
                return true;
            }
        }
        a_in != b_in
    }

    pub fn range_on_axis(&self, axis: Axis) -> (f64, f64) {
        match axis {
            Axis::Horizontal => (self.left(), self.right()),
            Axis::Vertical => (self.top(), self.bottom()),
        }
    }

    pub fn cross_range_on_axis(&self, axis: Axis) -> (f64, f64) {
        match axis {
            Axis::Horizontal => (self.top(), self.bottom()),
            Axis::Vertical => (self.left(), self.right()),
        }
    }
}

/// 坐标轴方向（用于消除水平/垂直镜像代码）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub fn other(self) -> Self {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }

    pub fn main_coord(self, p: Point) -> f64 {
        match self {
            Axis::Horizontal => p.x,
            Axis::Vertical => p.y,
        }
    }

    pub fn cross_coord(self, p: Point) -> f64 {
        match self {
            Axis::Horizontal => p.y,
            Axis::Vertical => p.x,
        }
    }

    pub fn point(self, main: f64, cross: f64) -> Point {
        match self {
            Axis::Horizontal => Point::new(main, cross),
            Axis::Vertical => Point::new(cross, main),
        }
    }

    pub fn main_dir(self, sign: f64) -> Point {
        match self {
            Axis::Horizontal => Point::new(sign, 0.0),
            Axis::Vertical => Point::new(0.0, sign),
        }
    }

    pub fn cross_dir(self, sign: f64) -> Point {
        match self {
            Axis::Horizontal => Point::new(0.0, sign),
            Axis::Vertical => Point::new(sign, 0.0),
        }
    }
}

pub(crate) fn segments_cross(a1: Point, a2: Point, b1: Point, b2: Point, eps: f64) -> bool {
    let d1 = cross(b2 - b1, a1 - b1);
    let d2 = cross(b2 - b1, a2 - b1);
    let d3 = cross(a2 - a1, b1 - a1);
    let d4 = cross(a2 - a1, b2 - a1);

    if ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps))
        && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps))
    {
        return true;
    }
    false
}

fn segment_intersects_segment(a1: Point, a2: Point, b1: Point, b2: Point) -> bool {
    segments_cross(a1, a2, b1, b2, 0.0)
}

fn cross(a: Point, b: Point) -> f64 {
    a.x * b.y - a.y * b.x
}

impl From<&super::NodeLayout> for Rect {
    fn from(nl: &super::NodeLayout) -> Self {
        Self::new(nl.x, nl.y, nl.width, nl.height)
    }
}

impl From<&super::GroupLayout> for Rect {
    fn from(gl: &super::GroupLayout) -> Self {
        Self::new(gl.x, gl.y, gl.width, gl.height)
    }
}

pub fn node_center(nl: &super::NodeLayout) -> Point {
    Point::new(nl.x + nl.width / 2.0, nl.y + nl.height / 2.0)
}

pub const EPS: f64 = 0.1;
