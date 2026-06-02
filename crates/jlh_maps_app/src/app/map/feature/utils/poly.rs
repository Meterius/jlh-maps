pub fn ring_without_closing_position(ring: &[Vec<f64>]) -> &[Vec<f64>] {
    let Some((first, rest)) = ring.split_first() else {
        return ring;
    };
    let Some(last) = rest.last() else {
        return ring;
    };

    if positions_equal_2d(first, last) {
        &ring[..ring.len() - 1]
    } else {
        ring
    }
}

fn positions_equal_2d(left: &[f64], right: &[f64]) -> bool {
    left.len() >= 2
        && right.len() >= 2
        && (left[0] - right[0]).abs() <= f64::EPSILON
        && (left[1] - right[1]).abs() <= f64::EPSILON
}
