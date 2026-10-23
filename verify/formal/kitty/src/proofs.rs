//! The harnesses. Each takes every byte string of its length; lengths are
//! fixed per harness so the solver unrolls each loop a known number of
//! times. The framing harnesses did not finish on the development Mac and
//! are follow-ups (README.md).

use super::encodes;

/// `tiles_term::b64::encode` is base64 of every byte string of length `N`:
/// the right length, only the alphabet, and it decodes to its input.
fn b64_round_trips<const N: usize>() {
    let bytes: [u8; N] = kani::any();
    let coded = tiles_term::b64::encode(&bytes);
    assert!(encodes(coded.as_bytes(), &bytes));
}

// One harness per length, so each is solved on its own: 1 and 2 bytes end
// in two and one `=`, 3 bytes in none. 4 bytes did not finish in 3 minutes
// (README.md).
#[kani::proof]
#[kani::unwind(6)]
fn b64_round_trips_1_byte() {
    b64_round_trips::<1>();
}

#[kani::proof]
#[kani::unwind(6)]
fn b64_round_trips_2_bytes() {
    b64_round_trips::<2>();
}

#[kani::proof]
#[kani::unwind(6)]
fn b64_round_trips_3_bytes() {
    b64_round_trips::<3>();
}
