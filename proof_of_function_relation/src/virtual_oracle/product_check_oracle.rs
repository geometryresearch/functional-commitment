use crate::error::Error;
use crate::virtual_oracle::VirtualOracle;
use ark_ff::PrimeField;
use ark_poly::univariate::DensePolynomial;
use ark_poly_commit::LabeledPolynomial;

/// A virtual oracle that accepts 3 concrete oracles f, g, h and computes: v(x) = f(x) - g(x)h(x).
/// If this virtual oracle evaluates to 0 over a given set, then f = g*h at all points in that set.
pub struct ProductCheckVO {}

impl ProductCheckVO {
    pub fn new() -> Self {
        Self {}
    }
}

impl<F: PrimeField> VirtualOracle<F> for ProductCheckVO {
    fn instantiate_in_coeffs_form(
        &self,
        concrete_oracles: &[LabeledPolynomial<F, DensePolynomial<F>>],
        _alphas: &[F],
    ) -> Result<DensePolynomial<F>, Error> {
        if concrete_oracles.len() != 3 {
            return Err(Error::InstantiationError);
        }

        Ok(concrete_oracles[0].polynomial()
            - &(concrete_oracles[1].polynomial() * concrete_oracles[2].polynomial()))
    }

    fn num_of_oracles(&self) -> usize {
        return 3;
    }

    fn query(&self, evals: &[F], _point: F) -> Result<F, Error> {
        if evals.len() != 3 {
            return Err(Error::EvaluationError);
        }

        Ok(evals[0] - evals[1] * evals[2])
    }

    /// this map encodes at which concrete oracle should h_i point
    fn mapping_vector(&self) -> Vec<usize> {
        // h0 = f0, h1 = f1, h2 = f2
        Vec::from([0, 1, 2])
    }
}
