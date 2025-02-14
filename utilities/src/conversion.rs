// Copyright (c) 2022 Espresso Systems (espressosys.com)
// This file is part of the Jellyfish library.

// You should have received a copy of the MIT License
// along with the Jellyfish library. If not, see <https://mit-license.org/>.

use ark_ec::ModelParameters;
use ark_ff::{BigInteger, PrimeField};
use ark_std::{cmp::min, vec::Vec};
use sha2::{Digest, Sha512};

/// Convert a scalar field element to a base field element.
/// Mod reduction is not performed since the conversion occurs
/// for fields on a same curve.
pub fn fr_to_fq<F, P>(scalar: &P::ScalarField) -> F
where
    F: PrimeField,
    P: ModelParameters<BaseField = F> + Clone,
{
    // sanity checks:
    // ensure | jubjub scalar field | <= | BLS Scalar field |
    // jubjub scalar field:
    // 6554484396890773809930967563523245729705921265872317281365359162392183254199
    // BLS12-381 scalar field:
    // 52435875175126190479447740508185965837690552500527637822603658699938581184513
    // jubjub377 scalar field:
    // 2111115437357092606062206234695386632838870926408408195193685246394721360383
    // BLS12-377 scalar field:
    // 8444461749428370424248824938781546531375899335154063827935233455917409239041
    F::from_le_bytes_mod_order(&scalar.into_repr().to_bytes_le())
}

/// Convert a base field element to a scalar field element.
/// Perform a mod reduction if the base field element is greater than
/// the modulus of the scalar field.
pub fn fq_to_fr<F, P>(base: &F) -> P::ScalarField
where
    F: PrimeField,
    P: ModelParameters<BaseField = F> + Clone,
{
    P::ScalarField::from_le_bytes_mod_order(&base.into_repr().to_bytes_le())
}

/// Convert a field element in F(rom) to a field element in T(o),
/// with |T| < |F|; truncating the element via masking the top
/// F::size_in_bits() - T::size_in_bits() with 0s
pub fn fq_to_fr_with_mask<F, T>(base: &F) -> T
where
    F: PrimeField,
    T: PrimeField,
{
    assert!(T::size_in_bits() < F::size_in_bits());
    let length = T::size_in_bits() >> 3;
    // ensure that no mod reduction happened
    T::from_le_bytes_mod_order(&base.into_repr().to_bytes_le()[0..length])
}

// convert a field element in F(rom)
// to a field element in T(o).
// return an error if a mod reduction occurs.
#[inline]
pub fn field_switching<F, T>(base: &F) -> T
where
    F: PrimeField,
    T: PrimeField,
{
    let bytes = base.into_repr().to_bytes_le();
    let t = T::from_le_bytes_mod_order(&bytes);

    // check t == base
    // i.e., t did not overflow the target field
    let bytes_rec = t.into_repr().to_bytes_le();
    let length = min(bytes.len(), bytes_rec.len());
    assert_eq!(bytes_rec[0..length], bytes[0..length],);
    t
}

/// Hash a sequence of bytes to into a field
/// element, whose order is less than 256 bits.
pub fn hash_to_field<B, F>(bytes: B) -> F
where
    B: AsRef<[u8]>,
    F: PrimeField,
{
    // we extract a random `rand_byte_len` bytes from the hash
    // the compute res = OS2IP(output) mod p
    // which is less than 2^-128 from uniform
    let rand_byte_len = (F::size_in_bits() + 7) / 8 + 128 / 8;
    let mut hasher = Sha512::default();
    hasher.update(bytes.as_ref());
    let output = &hasher.finalize()[0..rand_byte_len];

    F::from_le_bytes_mod_order(output)
}

/// One-way, deterministic, infallible conversion between arbitrary bytes (of
/// unknown length and potentially non-canonical) to field elements.
/// This function converts bytes to vector of BaseField.
pub fn bytes_to_field_elements<B, F>(bytes: B) -> Vec<F>
where
    B: AsRef<[u8]> + Clone,
    F: PrimeField,
{
    // segment the bytes into trunks of bytes, each trunk is of size
    // that is floor(F::size_in_bits/8). then, cast each trunk
    // into F via F::from_le_bytes_mod_order
    // note that mod_reduction is guaranteed to not occur

    // Field order is never a multiple of 8
    let trunk_length = F::size_in_bits() / 8;

    // pad the input to a multiple of trunk_length
    let padded_length = (bytes.as_ref().len() + trunk_length - 1) / trunk_length * trunk_length;
    let mut padded_bytes: Vec<u8> = bytes.as_ref().to_vec();
    padded_bytes.resize(padded_length, 0u8);
    assert!(padded_bytes.len() % trunk_length == 0);

    let mut result = Vec::new();
    for chunk in padded_bytes.chunks(trunk_length) {
        result.push(F::from_le_bytes_mod_order(chunk));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ark_ed_on_bls12_377::{EdwardsParameters as Param377, Fr as Fr377};
    use ark_ed_on_bls12_381::{EdwardsParameters as Param381, Fr as Fr381};
    use ark_ed_on_bn254::{EdwardsParameters as Param254, Fr as Fr254};
    use ark_std::UniformRand;

    #[test]
    fn test_bn254_scalar_conversion() {
        let mut rng = ark_std::test_rng();
        for _ in 0..6 {
            let jj = Fr254::rand(&mut rng);
            let jj_bls = fr_to_fq::<_, Param254>(&jj);
            assert!(jj.into_repr() == jj_bls.into_repr());
        }
    }

    #[test]
    fn test_jubjub_bls_scalar_conversion_377() {
        let mut rng = ark_std::test_rng();
        for _ in 0..6 {
            let jj = Fr377::rand(&mut rng);
            let jj_bls = fr_to_fq::<_, Param377>(&jj);
            assert!(jj.into_repr() == jj_bls.into_repr());
        }
    }

    #[test]
    fn test_jubjub_bls_scalar_conversion_381() {
        let mut rng = ark_std::test_rng();
        for _ in 0..6 {
            let jj = Fr381::rand(&mut rng);
            let jj_bls = fr_to_fq::<_, Param381>(&jj);
            assert!(jj.into_repr() == jj_bls.into_repr());
        }
    }
}
