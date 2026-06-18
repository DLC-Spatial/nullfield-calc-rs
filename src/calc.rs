use astro_float::{BigFloat, Consts, RoundingMode};

/// 256-bit precision gives ~77 decimal digits, well beyond Decimal.js 50-digit.
const PREC: usize = 256;
/// Use no rounding for intermediate ops; only the final f64 conversion rounds.
const RM: RoundingMode = RoundingMode::None;

fn f(v: f64) -> BigFloat {
    BigFloat::from_f64(v, PREC)
}

fn bf_to_f64(x: &BigFloat) -> f64 {
    format!("{}", x).parse().unwrap_or(f64::NAN)
}

/// Convert DMS bearing (DDD.MMSS) to radians at full precision.
fn dms_to_rad(dms: f64, cc: &mut Consts) -> BigFloat {
    let pi = cc.pi(PREC, RM);
    f(dms_to_dd(dms))
        .mul(&pi, PREC, RM)
        .div(&f(180.0), PREC, RM)
}

fn radiate(
    e: &BigFloat,
    n: &BigFloat,
    bearing_dms: f64,
    distance: f64,
    cc: &mut Consts,
) -> (BigFloat, BigFloat) {
    let rad = dms_to_rad(bearing_dms, cc);
    let dist = f(distance);
    let sin_b = rad.sin(PREC, RM, cc);
    let cos_b = rad.cos(PREC, RM, cc);
    (
        e.add(&dist.mul(&sin_b, PREC, RM), PREC, RM),
        n.add(&dist.mul(&cos_b, PREC, RM), PREC, RM),
    )
}

/// atan2(east, north) — surveying bearing from north, result in [0, 2π).
fn atan2_bearing(east: &BigFloat, north: &BigFloat, cc: &mut Consts) -> BigFloat {
    let pi = cc.pi(PREC, RM);
    let two_pi = f(2.0).mul(&pi, PREC, RM);

    let angle = if north.is_positive() {
        east.div(north, PREC, RM).atan(PREC, RM, cc)
    } else if north.is_negative() {
        let base = east.div(north, PREC, RM).atan(PREC, RM, cc);
        if !east.is_negative() {
            base.add(&pi, PREC, RM)
        } else {
            base.sub(&pi, PREC, RM)
        }
    } else if east.is_positive() {
        pi.div(&f(2.0), PREC, RM)
    } else if east.is_negative() {
        pi.div(&f(2.0), PREC, RM).neg()
    } else {
        f(0.0)
    };

    let normed = angle.rem(&two_pi);
    if normed.is_negative() {
        normed.add(&two_pi, PREC, RM)
    } else {
        normed
    }
}

/// Format decimal degrees (f64) as DDD°MM'SS.ss" — extends to four decimal
/// places of seconds when sub-second precision is present, otherwise keeps two.
pub fn dd_to_dms_string(dd: f64) -> String {
    let dd = dd.rem_euclid(360.0);
    let total_sec = dd * 3600.0;
    let d = (total_sec / 3600.0).floor() as u32;
    let rem = total_sec - d as f64 * 3600.0;
    let m = (rem / 60.0).floor() as u32;
    let s = (rem - m as f64 * 60.0).clamp(0.0, 59.9999);
    format!("{:03}°{:02}'{}\"", d, m, format_seconds(s))
}

/// Seconds with two decimal places, extended up to four to show sub-second precision.
fn format_seconds(s: f64) -> String {
    let mut out = format!("{:07.4}", s);
    while out.ends_with('0') && out.split('.').nth(1).is_some_and(|f| f.len() > 2) {
        out.pop();
    }
    out
}

pub struct MiscloseResult {
    pub bearing_dd: f64,
    pub distance: f64,
    pub total_distance: f64,
    pub ratio: f64,
    pub ppm: f64,
}

/// Convert a packed DMS bearing (`DDD.MMSSssss`) to decimal degrees.
///
/// The two digits after the point are minutes, the next two are whole seconds,
/// and any further digits are fractional seconds (`253.01001234` =
/// 253°01′00.1234″). Packing to a 1e-8 integer rounds away f64 representation
/// noise (~1e-13) so the minute/second boundaries don't collapse, while keeping
/// up to four sub-second digits the user types past the SS field. This is the
/// single conversion used by both the calculation and the UI.
pub fn dms_to_dd(dms: f64) -> f64 {
    let sign = if dms < 0.0 { -1.0 } else { 1.0 };
    let packed = (dms.abs() * 1e8).round() as i64;
    let d = packed / 100_000_000;
    let mmss = packed % 100_000_000; // MMSSssss
    let m = mmss / 1_000_000; // MM
    let s = (mmss % 1_000_000) as f64 / 1e4; // SS.ssss
    sign * (d as f64 + m as f64 / 60.0 + s / 3600.0)
}

pub struct DeflectionCheck {
    pub sum_deg: f64,
    pub error_deg: f64,
}

pub fn check_deflection_sum(legs: &[(f64, f64)]) -> Option<DeflectionCheck> {
    if legs.len() < 3 {
        return None;
    }
    let bearings: Vec<f64> = legs.iter().map(|&(b, _)| dms_to_dd(b)).collect();
    let n = bearings.len();
    let sum: f64 = (0..n)
        .map(|i| {
            let b_in = bearings[i];
            let b_out = bearings[(i + 1) % n];
            ((b_out - b_in - 180.0).rem_euclid(360.0)) - 180.0
        })
        .sum();
    let error_deg = sum.abs() - 360.0;
    Some(DeflectionCheck {
        sum_deg: sum,
        error_deg,
    })
}

const BLUNDER_IMPROVEMENT_FACTOR: f64 = 3.0;

pub struct BlunderCandidate {
    pub leg_index: usize,
    pub ratio_without: f64,
}

pub fn detect_blunders(legs: &[(f64, f64)], current_ratio: f64) -> Vec<BlunderCandidate> {
    if legs.len() < 4 {
        return vec![];
    }
    let mut candidates: Vec<BlunderCandidate> = (0..legs.len())
        .filter_map(|i| {
            let reduced: Vec<_> = legs[..i]
                .iter()
                .chain(legs[i + 1..].iter())
                .copied()
                .collect();
            let ratio_without = calculate_misclose(&reduced)?.ratio;
            if ratio_without.is_finite()
                && ratio_without < current_ratio * BLUNDER_IMPROVEMENT_FACTOR
            {
                return None;
            }
            Some(BlunderCandidate {
                leg_index: i,
                ratio_without,
            })
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.ratio_without
            .partial_cmp(&a.ratio_without)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    candidates
}

pub fn calculate_misclose(legs: &[(f64, f64)]) -> Option<MiscloseResult> {
    if legs.is_empty() {
        return None;
    }

    let mut cc = Consts::new().ok()?;
    let mut e = f(0.0);
    let mut n = f(0.0);
    let mut total_distance = f(0.0);

    for &(bearing_dms, distance) in legs {
        let (new_e, new_n) = radiate(&e, &n, bearing_dms, distance, &mut cc);
        e = new_e;
        n = new_n;
        total_distance = total_distance.add(&f(distance), PREC, RM);
    }

    let dist_sq = e.mul(&e, PREC, RM).add(&n.mul(&n, PREC, RM), PREC, RM);
    let misclose_dist = dist_sq.sqrt(PREC, RM);

    let bearing_rad = atan2_bearing(&e, &n, &mut cc);
    let pi = cc.pi(PREC, RM);
    let bearing_dd = bf_to_f64(&bearing_rad.mul(&f(180.0), PREC, RM).div(&pi, PREC, RM));

    let total_f64 = bf_to_f64(&total_distance);
    let misclose_f64 = bf_to_f64(&misclose_dist);

    let (ratio, ppm) = if misclose_f64 > 1e-12 && total_f64 > 0.0 {
        (
            total_f64 / misclose_f64,
            (misclose_f64 / total_f64) * 1_000_000.0,
        )
    } else {
        (f64::INFINITY, 0.0)
    };

    Some(MiscloseResult {
        bearing_dd,
        distance: misclose_f64,
        total_distance: total_f64,
        ratio,
        ppm,
    })
}

#[cfg(test)]
mod tests {
    use super::{calculate_misclose, dd_to_dms_string, dms_to_dd};

    #[test]
    fn dms_to_dd_handles_boundary_minute_values() {
        let expected = 253.0 + (1.0 / 60.0);
        let actual = dms_to_dd(253.0100);
        assert!((actual - expected).abs() < 1e-12);
    }

    #[test]
    fn dms_to_dd_preserves_sub_second_precision() {
        // 253.01001234 = 253°01'00.1234"
        let expected = 253.0 + 1.0 / 60.0 + 0.1234 / 3600.0;
        let actual = dms_to_dd(253.01001234);
        assert!((actual - expected).abs() < 1e-12);
    }

    #[test]
    fn dd_to_dms_string_shows_sub_second_digits() {
        assert_eq!(dd_to_dms_string(dms_to_dd(253.01001234)), "253°01'00.1234\"");
    }

    #[test]
    fn dd_to_dms_string_keeps_two_decimals_without_sub_second() {
        assert_eq!(dd_to_dms_string(dms_to_dd(253.0100)), "253°01'00.00\"");
    }

    #[test]
    fn traverse_result_is_stable_for_tiny_hp_epsilon() {
        let legs = vec![
            (162.5146, 12.0),
            (73.0100, 133.260),
            (337.3927, 12.053),
            (253.0100, 134.353),
        ];
        let legs_with_epsilon = vec![
            (162.5146, 12.0),
            (73.0100, 133.260),
            (337.3927, 12.053),
            (253.010000001, 134.353),
        ];

        let baseline = calculate_misclose(&legs).expect("baseline misclose");
        let epsilon = calculate_misclose(&legs_with_epsilon).expect("epsilon misclose");

        assert!((baseline.distance - epsilon.distance).abs() < 1e-12);
        assert!((baseline.bearing_dd - epsilon.bearing_dd).abs() < 1e-12);
    }
}
