use crate::secp256r1::field::FieldElementConst;

use super::{
    points::{Affine, JacobianConst, Storage},
    ECMULT_TABLE_SIZE_G,
};

pub(super) struct GeneratorMultiplesTable([Storage; ECMULT_TABLE_SIZE_G]);

pub(super) const TABLE_G: GeneratorMultiplesTable = GeneratorMultiplesTable::new();

impl GeneratorMultiplesTable {
    const fn new() -> Self {
        let mut pre_g = [Storage::DEFAULT; ECMULT_TABLE_SIZE_G];
        let g = JacobianConst::GENERATOR;

        odd_multiples(&mut pre_g, &g);

        Self(pre_g)
    }

    pub(super) fn get_ge(&self, n: i32) -> Affine {
        if n > 0 {
            self.0[(n - 1) as usize / 2].clone().into_affine()
        } else {
            -(self.0[(-n - 1) as usize / 2].clone().into_affine())
        }
    }
}

const fn odd_multiples(table: &mut [Storage; ECMULT_TABLE_SIZE_G], gen: &JacobianConst) {
    use const_for::const_for;
    let mut gj = JacobianConst {
        x: FieldElementConst(gen.x.0),
        y: FieldElementConst(gen.y.0),
        z: FieldElementConst(gen.z.0),
    };

    table[0] = JacobianConst {
        x: FieldElementConst(gj.x.0),
        y: FieldElementConst(gj.y.0),
        z: FieldElementConst(gj.z.0),
    }
    .into_storage();

    let g_double = gen.double();

    const_for!(i in 1..ECMULT_TABLE_SIZE_G => {
        gj = gj.add(&g_double);
        table[i] = JacobianConst {
            x: FieldElementConst(gj.x.0),
            y: FieldElementConst(gj.y.0),
            z: FieldElementConst(gj.z.0),
        }.into_storage();
    });
}
