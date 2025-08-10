use crypto::ark_ec::CurveGroup;

use super::*;

pub const BLS12_381_G1_MSM_PER_POINT_GAS: u64 = 12000;
pub const BLS12_381_G2_MSM_PER_POINT_GAS: u64 = 22500;

pub const G1_MSM_PAIR_LEN: usize = SCALAR_SERIALIZATION_LEN + G1_SERIALIZATION_LEN;
pub const G2_MSM_PAIR_LEN: usize = SCALAR_SERIALIZATION_LEN + G2_SERIALIZATION_LEN;

pub const DISCOUNT_DENOMINATOR: u16 = 1000;

// k -> discount factor
pub const DISCOUNT_TABLE_G1_MSM: [u16; 128] = [
    1000, 949, 848, 797, 764, 750, 738, 728, 719, 712, 705, 698, 692, 687, 682, 677, 673, 669, 665,
    661, 658, 654, 651, 648, 645, 642, 640, 637, 635, 632, 630, 627, 625, 623, 621, 619, 617, 615,
    613, 611, 609, 608, 606, 604, 603, 601, 599, 598, 596, 595, 593, 592, 591, 589, 588, 586, 585,
    584, 582, 581, 580, 579, 577, 576, 575, 574, 573, 572, 570, 569, 568, 567, 566, 565, 564, 563,
    562, 561, 560, 559, 558, 557, 556, 555, 554, 553, 552, 551, 550, 549, 548, 547, 547, 546, 545,
    544, 543, 542, 541, 540, 540, 539, 538, 537, 536, 536, 535, 534, 533, 532, 532, 531, 530, 529,
    528, 528, 527, 526, 525, 525, 524, 523, 522, 522, 521, 520, 520, 519,
];
// k -> discount factor
pub const DISCOUNT_TABLE_G2_MSM: [u16; 128] = [
    1000, 1000, 923, 884, 855, 832, 812, 796, 782, 770, 759, 749, 740, 732, 724, 717, 711, 704,
    699, 693, 688, 683, 679, 674, 670, 666, 663, 659, 655, 652, 649, 646, 643, 640, 637, 634, 632,
    629, 627, 624, 622, 620, 618, 615, 613, 611, 609, 607, 606, 604, 602, 600, 598, 597, 595, 593,
    592, 590, 589, 587, 586, 584, 583, 582, 580, 579, 578, 576, 575, 574, 573, 571, 570, 569, 568,
    567, 566, 565, 563, 562, 561, 560, 559, 558, 557, 556, 555, 554, 553, 552, 552, 551, 550, 549,
    548, 547, 546, 545, 545, 544, 543, 542, 541, 541, 540, 539, 538, 537, 537, 536, 535, 535, 534,
    533, 532, 532, 531, 530, 530, 529, 528, 528, 527, 526, 526, 525, 524, 524,
];

fn compute_cost(
    input_len: usize,
    pair_len: usize,
    per_pair_cost: u64,
    discounts_table: &[u16; 128],
) -> u64 {
    let num_pairs = input_len / pair_len;
    if num_pairs == 0 {
        return 0;
    }
    let discount = if num_pairs > 128 {
        discounts_table[127]
    } else {
        discounts_table[num_pairs - 1]
    };

    let cost =
        (per_pair_cost * num_pairs as u64) * (discount as u64) / (DISCOUNT_DENOMINATOR as u64);

    cost
}

fn log2(x: usize) -> u32 {
    if x == 0 {
        0
    } else if x.is_power_of_two() {
        1usize.leading_zeros() - x.leading_zeros()
    } else {
        0usize.leading_zeros() - x.leading_zeros()
    }
}

fn ln_without_floats(a: usize) -> usize {
    // log2(a) * ln(2)
    (log2(a) * 69 / 100) as usize
}

fn msm<G: CurveGroup, A: core::alloc::Allocator + Clone>(
    bases: &[G::Affine],
    mut bigints: Vec<<Fr as PrimeField>::BigInt, A>,
    allocator: A,
) -> G {
    assert_eq!(bases.len(), bigints.len());
    let size = bases.len();

    let c = if size < 32 {
        3
    } else {
        ln_without_floats(size) + 2
    };

    let num_bits = 256usize;

    let zero = G::zero();
    let mut window_start = 0;
    let num_windows = num_bits.next_multiple_of(c) / c;

    let mut resulable_buckets = Vec::with_capacity_in((1 << c) - 1, allocator.clone());
    resulable_buckets.resize((1 << c) - 1, zero);

    let mut window_sums = Vec::with_capacity_in(num_windows, allocator);
    window_sums.resize(num_windows, zero);

    for window_idx in 0..num_windows {
        let last_window = window_idx == num_windows - 1;

        unsafe {
            core::hint::assert_unchecked(bases.len() == bigints.len());
        }
        for i in 0..bases.len() {
            let bigint = &mut bigints[i];
            // get window
            let scalar = bigint.as_ref()[0] % (1 << c);

            use core::ops::ShrAssign;
            bigint.shr_assign(c as u32);

            if scalar != 0 {
                resulable_buckets[(scalar - 1) as usize] += &bases[i];
            }
        }

        // now sum over buckets
        let mut tmp = zero;
        let mut window_result = zero;
        for el in resulable_buckets.iter_mut().rev() {
            tmp += &*el;
            window_result += &tmp;
            if last_window {
                *el = zero;
            }
        }

        window_sums[window_idx] = window_result;

        window_start += c;
    }

    assert!(window_start >= 256);

    let lowest = *window_sums.first().unwrap();

    lowest
        + &window_sums[1..]
            .iter()
            .rev()
            .fold(zero, |mut total, sum_i| {
                total += sum_i;
                for _ in 0..c {
                    total.double_in_place();
                }
                total
            })
}

pub struct Bls12381G1MSMPrecompile;

impl crate::PurePrecompileInvocation for Bls12381G1MSMPrecompile {
    type Subsystem = Bls12PrecompileErrors;

    fn invoke<'a, R: Resources, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        _caller_ee: u8,
        output: &mut SliceVec<'a, u8>,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Self::Subsystem>> {
        if input.len() == 0 {
            return Err(Bls12PrecompileSubsystemError::LeafUsage(interface_error!(
                Bls12PrecompileInterfaceError::InvalidInputSize
            )));
        }
        let cost = compute_cost(
            input.len(),
            G1_MSM_PAIR_LEN,
            BLS12_381_G1_MSM_PER_POINT_GAS,
            &DISCOUNT_TABLE_G1_MSM,
        );
        let cost_ergs = Ergs(cost * ERGS_PER_GAS);
        let cost_native = 0;
        resources.charge(&R::from_ergs_and_native(
            cost_ergs,
            <R::Native as zk_ee::system::Computational>::from_computational(cost_native),
        ))?;

        if input.len() % G1_MSM_PAIR_LEN != 0 {
            return Err(Bls12PrecompileSubsystemError::LeafUsage(interface_error!(
                Bls12PrecompileInterfaceError::InvalidInputSize
            )));
        }

        let num_pairs = input.len() % G1_MSM_PAIR_LEN;
        let mut scalars = Vec::with_capacity_in(num_pairs, allocator.clone());
        let mut points = Vec::with_capacity_in(num_pairs, allocator.clone());

        // arkworks MSM allocates inside, so we will do it our way, just parse here
        // G1Projective::msm_bigint(bases, bigints)

        // parse to use Peppinger algorithm
        for pair_encoding in input.as_chunks::<G1_MSM_PAIR_LEN>().0.iter() {
            let point = parse_g1_with_subgroup_check(
                pair_encoding[0..G1_SERIALIZATION_LEN].try_into().unwrap(),
            )?;
            let scalar = parse_integer(
                pair_encoding
                    [G1_SERIALIZATION_LEN..(G1_SERIALIZATION_LEN + SCALAR_SERIALIZATION_LEN)]
                    .try_into()
                    .unwrap(),
            );
            points.push(point);
            scalars.push(scalar);
        }

        let result: G1Projective = msm(&points, scalars, allocator);

        let result = result.into_affine();

        write_g1(result, output);

        Ok(())
    }
}

pub struct Bls12381G2MSMPrecompile;

impl crate::PurePrecompileInvocation for Bls12381G2MSMPrecompile {
    type Subsystem = Bls12PrecompileErrors;

    fn invoke<'a, R: Resources, A: core::alloc::Allocator + Clone>(
        input: &[u8],
        _caller_ee: u8,
        output: &mut SliceVec<'a, u8>,
        resources: &mut R,
        allocator: A,
    ) -> Result<(), SubsystemError<Self::Subsystem>> {
        if input.len() == 0 {
            return Err(Bls12PrecompileSubsystemError::LeafUsage(interface_error!(
                Bls12PrecompileInterfaceError::InvalidInputSize
            )));
        }
        let cost = compute_cost(
            input.len(),
            G2_MSM_PAIR_LEN,
            BLS12_381_G2_MSM_PER_POINT_GAS,
            &DISCOUNT_TABLE_G2_MSM,
        );
        let cost_ergs = Ergs(cost * ERGS_PER_GAS);
        let cost_native = 0;
        resources.charge(&R::from_ergs_and_native(
            cost_ergs,
            <R::Native as zk_ee::system::Computational>::from_computational(cost_native),
        ))?;

        if input.len() % G2_MSM_PAIR_LEN != 0 {
            return Err(Bls12PrecompileSubsystemError::LeafUsage(interface_error!(
                Bls12PrecompileInterfaceError::InvalidInputSize
            )));
        }

        let num_pairs = input.len() % G2_MSM_PAIR_LEN;
        let mut scalars = Vec::with_capacity_in(num_pairs, allocator.clone());
        let mut points = Vec::with_capacity_in(num_pairs, allocator.clone());

        // arkworks MSM allocates inside, so we will do it our way, just parse here
        // G1Projective::msm_bigint(bases, bigints)

        // parse to use Peppinger algorithm
        for pair_encoding in input.as_chunks::<G2_MSM_PAIR_LEN>().0.iter() {
            let point = parse_g2_with_subgroup_check(
                pair_encoding[0..G2_SERIALIZATION_LEN].try_into().unwrap(),
            )?;
            let scalar = parse_integer(
                pair_encoding
                    [G2_SERIALIZATION_LEN..(G1_SERIALIZATION_LEN + SCALAR_SERIALIZATION_LEN)]
                    .try_into()
                    .unwrap(),
            );
            points.push(point);
            scalars.push(scalar);
        }

        let result: G2Projective = msm(&points, scalars, allocator);

        let result = result.into_affine();

        write_g2(result, output);

        Ok(())
    }
}
