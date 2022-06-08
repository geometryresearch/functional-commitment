use crate::non_zero_over_k::proof::Proof;
use crate::{
    commitment::HomomorphicPolynomialCommitment,
    error::{to_pc_error, Error},
    virtual_oracle::{inverse_check_oracle::InverseCheckOracle, VirtualOracle},
    zero_over_k::ZeroOverK,
};
use ark_ff::PrimeField;
use ark_poly::{
    univariate::DensePolynomial, EvaluationDomain, GeneralEvaluationDomain, UVPolynomial,
};
use ark_poly_commit::{LabeledCommitment, LabeledPolynomial};
use digest::Digest; // Note that in the latest Marlin commit, Digest has been replaced by an arkworks trait `FiatShamirRng`
use rand::Rng;
use std::marker::PhantomData;

pub mod proof;
mod tests;

struct NonZeroOverK<F: PrimeField, PC: HomomorphicPolynomialCommitment<F>, D: Digest> {
    _field: PhantomData<F>,
    _pc: PhantomData<PC>,
    _diges: PhantomData<D>,
}

impl<F: PrimeField, PC: HomomorphicPolynomialCommitment<F>, D: Digest> NonZeroOverK<F, PC, D> {
    pub fn prove<R: Rng>(
        ck: &PC::CommitterKey,
        domain: &GeneralEvaluationDomain<F>,
        f: LabeledPolynomial<F, DensePolynomial<F>>,
        rng: &mut R,
    ) -> Result<Proof<F, PC>, Error> {
        let f_evals = domain.fft(f.coeffs());

        let g_evals = f_evals
            .iter()
            .map(|x| x.inverse().unwrap())
            .collect::<Vec<_>>();

        let g = DensePolynomial::<F>::from_coefficients_slice(&domain.ifft(&g_evals));
        let g = LabeledPolynomial::new(String::from("g"), g.clone(), None, None);

        let concrete_oracles = [f, g];
        let alphas = vec![F::one(), F::one()];
        let (commitments, rands) =
            PC::commit(ck, &concrete_oracles, None).map_err(to_pc_error::<F, PC>)?;

        let zero_over_k_vo = InverseCheckOracle {};

        let zero_over_k_proof = ZeroOverK::<F, PC, D>::prove(
            &concrete_oracles,
            &commitments,
            &rands,
            &zero_over_k_vo,
            &alphas,
            &domain,
            ck,
            rng,
        )?;

        let proof = Proof {
            g_commit: commitments[1].commitment().clone(),
            zero_over_k_proof,
        };

        Ok(proof)
    }

    pub fn verify(
        vk: &PC::VerifierKey,
        domain: &GeneralEvaluationDomain<F>,
        f_commit: LabeledCommitment<PC::Commitment>,
        proof: Proof<F, PC>,
    ) -> Result<(), Error> {
        //TODO check g bound
        let g_commit = LabeledCommitment::new(String::from("g"), proof.g_commit.clone(), None);

        let concrete_oracles_commitments = [f_commit, g_commit];
        let zero_over_k_vo = InverseCheckOracle {};
        let alphas = vec![F::one(), F::one()];

        ZeroOverK::<F, PC, D>::verify(
            proof.zero_over_k_proof,
            &concrete_oracles_commitments,
            &zero_over_k_vo,
            &domain,
            &alphas,
            &vk,
        )
    }
}
