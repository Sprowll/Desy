//! Turns a raw freehand stroke (mouse/pen samples) into a light, smooth
//! Bézier path: resample, low-pass filter, spline, then curve-fit.

use kurbo::simplify::{SimplifyOptLevel, SimplifyOptions, simplify_bezpath};
use kurbo::{BezPath, Point, Vec2};

/// Fit a smooth path through freehand samples.
///
/// - `smoothing` in `0..=1`: 0 follows the hand closely, 1 rounds heavily
///   and keeps very few points.
/// - `px`: size of one screen pixel in document units, so the result feels
///   the same at every zoom level.
pub fn fit_freehand(points: &[Point], closed: bool, smoothing: f64, px: f64) -> BezPath {
    let s = smoothing.clamp(0.0, 1.0);
    let step = 2.0 * px;
    let mut pts = resample(points, closed, step);
    if pts.len() < 2 {
        return polyline(points, closed);
    }
    if pts.len() < 4 {
        return polyline(&pts, closed);
    }
    let sigma = (1.0 + 14.0 * s) * px / step;
    pts = gaussian(&pts, closed, sigma.min(pts.len() as f64 / 6.0));
    let spline = catmull_rom(&pts, closed);
    let tolerance = (0.5 + 4.5 * s * s) * px;
    let opts = SimplifyOptions::default().opt_level(SimplifyOptLevel::Optimize);
    simplify_bezpath(spline, tolerance, &opts)
}

fn polyline(points: &[Point], closed: bool) -> BezPath {
    let mut out = BezPath::new();
    if let Some((first, rest)) = points.split_first() {
        out.move_to(*first);
        for p in rest {
            out.line_to(*p);
        }
        if closed {
            out.close_path();
        }
    }
    out
}

/// Evenly spaced samples along the polyline (the jitter of the input rate
/// would otherwise bias the filter). A closed stroke includes the segment
/// back to the start but does not repeat the first point.
fn resample(points: &[Point], closed: bool, step: f64) -> Vec<Point> {
    let mut src: Vec<Point> = Vec::with_capacity(points.len() + 1);
    for &p in points {
        if src.last().is_none_or(|l: &Point| l.distance(p) > 1e-9) {
            src.push(p);
        }
    }
    if closed && src.len() > 2 {
        src.push(src[0]);
    }
    let Some(&first) = src.first() else { return Vec::new() };
    let mut out = vec![first];
    let mut carry = 0.0;
    for w in src.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        let mut t = step - carry;
        while t <= len {
            out.push(a.lerp(b, t / len));
            t += step;
        }
        carry = len - (t - step);
    }
    let last = *src.last().unwrap();
    if closed {
        // Drop samples that landed on (or right next to) the start again.
        while out.len() > 1 && out.last().unwrap().distance(first) < step * 0.5 {
            out.pop();
        }
    } else if out.last().unwrap().distance(last) > step * 0.25 {
        out.push(last);
    } else {
        *out.last_mut().unwrap() = last;
    }
    out
}

/// Gaussian low-pass filter. Open strokes are extended by point reflection
/// so the end points stay put; closed ones wrap around.
fn gaussian(pts: &[Point], closed: bool, sigma: f64) -> Vec<Point> {
    if sigma < 0.3 {
        return pts.to_vec();
    }
    let n = pts.len() as isize;
    let radius = (sigma * 3.0).ceil() as isize;
    let kernel: Vec<f64> = (-radius..=radius).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let total: f64 = kernel.iter().sum();
    let at = |i: isize| -> Vec2 {
        if closed {
            return pts[i.rem_euclid(n) as usize].to_vec2();
        }
        if i < 0 {
            let j = (-i).min(n - 1) as usize;
            pts[0].to_vec2() * 2.0 - pts[j].to_vec2()
        } else if i >= n {
            let j = (2 * (n - 1) - i).max(0) as usize;
            pts[n as usize - 1].to_vec2() * 2.0 - pts[j].to_vec2()
        } else {
            pts[i as usize].to_vec2()
        }
    };
    (0..n)
        .map(|i| {
            let mut acc = Vec2::ZERO;
            for (k, w) in kernel.iter().enumerate() {
                acc += at(i + k as isize - radius) * *w;
            }
            (acc / total).to_point()
        })
        .collect()
}

/// C1 spline through the points, so the fitter sees one smooth run instead
/// of a corner at every sample.
fn catmull_rom(pts: &[Point], closed: bool) -> BezPath {
    let n = pts.len();
    let get = |i: isize| -> Point {
        if closed { pts[i.rem_euclid(n as isize) as usize] } else { pts[i.clamp(0, n as isize - 1) as usize] }
    };
    let mut out = BezPath::new();
    out.move_to(pts[0]);
    let segs = if closed { n } else { n - 1 };
    for i in 0..segs as isize {
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        out.curve_to(p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2);
    }
    if closed {
        out.close_path();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{PathEl, Shape};

    fn nodes(p: &BezPath) -> usize {
        p.elements().iter().filter(|e| matches!(e, PathEl::LineTo(_) | PathEl::CurveTo(..))).count()
    }

    /// A shaky hand-drawn arc: a semicircle with ±1.5 px zig-zag noise.
    fn shaky_arc() -> Vec<Point> {
        (0..=300)
            .map(|i| {
                let a = std::f64::consts::PI * i as f64 / 300.0;
                let noise = if i % 2 == 0 { 1.5 } else { -1.5 };
                let r = 100.0 + noise;
                Point::new(r * a.cos(), r * a.sin())
            })
            .collect()
    }

    #[test]
    fn noisy_stroke_becomes_a_few_curves() {
        let out = fit_freehand(&shaky_arc(), false, 0.5, 1.0);
        assert!(nodes(&out) <= 4, "got {} segments", nodes(&out));
        // End points are kept.
        let PathEl::MoveTo(start) = out.elements()[0] else { panic!() };
        assert!(start.distance(Point::new(101.5, 0.0)) < 1e-6);
    }

    #[test]
    fn more_smoothing_means_fewer_points() {
        let low = nodes(&fit_freehand(&shaky_arc(), false, 0.0, 1.0));
        let high = nodes(&fit_freehand(&shaky_arc(), false, 1.0, 1.0));
        assert!(high <= low, "low {low}, high {high}");
    }

    #[test]
    fn closed_loop_stays_closed_and_round() {
        let pts: Vec<Point> = (0..200)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 200.0;
                Point::new(50.0 * a.cos(), 50.0 * a.sin())
            })
            .collect();
        let out = fit_freehand(&pts, true, 0.5, 1.0);
        assert!(matches!(out.elements().last(), Some(PathEl::ClosePath)));
        assert!(nodes(&out) <= 6, "got {} segments", nodes(&out));
        let area = out.area().abs();
        let circle = std::f64::consts::PI * 50.0 * 50.0;
        assert!((area - circle).abs() / circle < 0.03, "area {area}");
    }

    #[test]
    fn tiny_strokes_do_not_panic() {
        assert_eq!(nodes(&fit_freehand(&[Point::ZERO], false, 0.5, 1.0)), 0);
        let two = fit_freehand(&[Point::ZERO, Point::new(3.0, 0.0)], false, 0.5, 1.0);
        assert!(nodes(&two) >= 1);
    }
}
