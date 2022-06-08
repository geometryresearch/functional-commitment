use crate::{
    commitment::HomomorphicPolynomialCommitment,
    discrete_log_comparison::{piop::PIOPforDLComparison, proof::Proof},
    error::Error,
    geo_seq::GeoSeqTest,
    label_polynomial,
    non_zero_over_k::NonZeroOverK,
    to_poly,
    virtual_oracle::{
        product_check_oracle::ProductCheckVO, square_check_oracle::SquareCheckOracle,
    },
    zero_over_k::ZeroOverK,
};
use ark_ff::{to_bytes, PrimeField, SquareRootField};
use ark_marlin::rng::FiatShamirRng;
use ark_poly::{
    univariate::DensePolynomial, EvaluationDomain, GeneralEvaluationDomain, UVPolynomial,
};
use ark_poly_commit::{LabeledCommitment, LabeledPolynomial, PCRandomness};
use ark_std::marker::PhantomData;
use digest::Digest; // Note that in the latest Marlin commit, Digest has been replaced by an arkworks trait `FiatShamirRng`
use rand::Rng;

pub mod piop;
pub mod proof;
mod tests;

pub struct DLComparison<
    F: PrimeField + SquareRootField,
    PC: HomomorphicPolynomialCommitment<F>,
    D: Digest,
> {
    _field: PhantomData<F>,
    _polynomial_commitment_scheme: PhantomData<PC>,
    _digest: PhantomData<D>,
}

impl<F, PC, D> DLComparison<F, PC, D>
where
    F: PrimeField + SquareRootField,
    PC: HomomorphicPolynomialCommitment<F>,
    D: Digest,
{
    pub const PROTOCOL_NAME: &'static [u8] = b"Discrete-log Comparison";

    pub fn prove<R: Rng>(
        ck: &PC::CommitterKey,
        domain_k: &GeneralEvaluationDomain<F>,
        domain_h: &GeneralEvaluationDomain<F>,
        f: &LabeledPolynomial<F, DensePolynomial<F>>,
        g: &LabeledPolynomial<F, DensePolynomial<F>>,
        f_commit: &LabeledCommitment<PC::Commitment>,
        g_commit: &LabeledCommitment<PC::Commitment>,
        fs_rng: &mut FiatShamirRng<D>,
        rng: &mut R,
        vk: &PC::VerifierKey, //TODO remove after verifications
    ) -> Result<Proof<F, PC>, Error> {
        let prover_initial_state = PIOPforDLComparison::prover_init(domain_k, domain_h, f, g)?;

        //------------------------------------------------------------------
        // First Round
        let (_, prover_first_oracles, _) =
            PIOPforDLComparison::prover_first_round(prover_initial_state, rng)?;

        // commit to s and p where p in {f_prime, g_prime, s_prime}
        // order of commitments is: s, f_prime, g_prime, s_prime, h
        let (commitments, _) = PC::commit(ck, prover_first_oracles.iter(), None).unwrap();
        fs_rng.absorb(&to_bytes![Self::PROTOCOL_NAME, commitments].unwrap());

        let square_check_vo = SquareCheckOracle::new();

        let alphas = [F::one(), F::one()];

        // Zero over K for f = (f')^2
        let f_prime_square_proof = ZeroOverK::<F, PC, D>::prove(
            &[f.clone(), prover_first_oracles.f_prime.clone()],
            &[f_commit.clone(), commitments[1].clone()], // f and f'
            &[PC::Randomness::empty(), PC::Randomness::empty()],
            &square_check_vo,
            &alphas.to_vec(),
            &domain_k,
            &ck,
            rng,
        )?;

        // Zero over K for g = (g')^2
        let g_prime_square_proof = ZeroOverK::<F, PC, D>::prove(
            &[g.clone(), prover_first_oracles.g_prime.clone()],
            &[g_commit.clone(), commitments[2].clone()], // g and g'
            &[PC::Randomness::empty(), PC::Randomness::empty()],
            &square_check_vo,
            &alphas.to_vec(),
            &domain_k,
            &ck,
            rng,
        )?;

        // Zero over K for s = (s')^2
        let s_prime_square_proof = ZeroOverK::<F, PC, D>::prove(
            &[
                prover_first_oracles.s.clone(),
                prover_first_oracles.s_prime.clone(),
            ],
            &[commitments[0].clone(), commitments[3].clone()], // s and s'
            &[PC::Randomness::empty(), PC::Randomness::empty()],
            &square_check_vo,
            &alphas.to_vec(),
            &domain_k,
            &ck,
            rng,
        )?;

        // // SANITY CHECK
        // for element in domain_k.elements() {
        //     let eval = prover_first_oracles.f_prime.evaluate(&element)
        //         - prover_first_oracles.s_prime.evaluate(&element)
        //             * prover_first_oracles.g_prime.evaluate(&element);
        //     assert_eq!(eval, F::zero());
        // }

        // Zero over K for f' = (s')*(g')
        let product_check_vo = ProductCheckVO::new();
        let alphas = [F::one(), F::one(), F::one()];
        let f_prime_product_proof = ZeroOverK::<F, PC, D>::prove(
            &[
                prover_first_oracles.f_prime.clone(),
                prover_first_oracles.s_prime.clone(),
                prover_first_oracles.g_prime.clone(),
            ],
            &[
                commitments[1].clone(),
                commitments[3].clone(),
                commitments[2].clone(),
            ], // f', s' and g'
            &[
                PC::Randomness::empty(),
                PC::Randomness::empty(),
                PC::Randomness::empty(),
            ],
            &product_check_vo,
            &alphas.to_vec(),
            &domain_k,
            &ck,
            rng,
        )?;

        // Geometric Sequence Test for h
        let omega: F = domain_h.element(1);
        let delta = omega.sqrt().unwrap();
        let mut a_s = vec![F::one()];
        let mut c_s = vec![domain_h.size()];

        let to_pad = domain_k.size() - domain_h.size();
        if to_pad > 0 {
            a_s.push(F::zero());
            c_s.push(to_pad);
        }

        let h_proof =
            GeoSeqTest::<F, PC, D>::prove(&ck, delta, &mut a_s, &mut c_s, &domain_k, rng)?;

        // Non-zero over K for f′
        let nzk_f_prime_proof = NonZeroOverK::<F, PC, D>::prove(
            ck,
            domain_k,
            prover_first_oracles.f_prime.clone(),
            rng,
        )?;

        // Non-zero over K for g′
        let nzk_g_prime_proof = NonZeroOverK::<F, PC, D>::prove(
            ck,
            domain_k,
            prover_first_oracles.g_prime.clone(),
            rng,
        )?;

        // Non-zero over K for s′
        let nzk_s_prime_proof = NonZeroOverK::<F, PC, D>::prove(
            ck,
            domain_k,
            prover_first_oracles.s_prime.clone(),
            rng,
        )?;

        // Non-zero over K for s(X) − 1
        // it's important to note that verifier will derive S(x) - 1 commitment on it's own side
        // we can use msm from our HomomorphicPolynomial trait and generator from ck
        let one_poly = label_polynomial!(to_poly!(F::one()));
        let s_minus_one = prover_first_oracles.s.polynomial() - one_poly.polynomial();
        let s_minus_one = label_polynomial!(s_minus_one);
        let nzk_s_minus_one_proof =
            NonZeroOverK::<F, PC, D>::prove(ck, domain_k, s_minus_one.clone(), rng)?;

        // TODO here we need to do also subset checks

        let proof = Proof {
            // Commitments
            s_commit: commitments[0].commitment().clone(),
            f_prime_commit: commitments[1].commitment().clone(),
            g_prime_commit: commitments[2].commitment().clone(),
            s_prime_commit: commitments[3].commitment().clone(),
            h_commit: commitments[4].commitment().clone(),

            // Proofs
            f_prime_square_proof,
            g_prime_square_proof,
            s_prime_square_proof,
            f_prime_product_proof,
            h_proof,
            nzk_f_prime_proof,
            nzk_g_prime_proof,
            nzk_s_prime_proof,
            nzk_s_minus_one_proof,
        };

        Ok(proof)
    }

    pub fn verify(
        vk: &PC::VerifierKey,
        ck: &PC::CommitterKey,
        domain_k: &GeneralEvaluationDomain<F>,
        domain_h: &GeneralEvaluationDomain<F>,
        f_commit: &LabeledCommitment<PC::Commitment>,
        g_commit: &LabeledCommitment<PC::Commitment>,
        proof: Proof<F, PC>,
        fs_rng: &mut FiatShamirRng<D>,
    ) -> Result<(), Error> {
        let commitments = vec![
            LabeledCommitment::new(String::from("s"), proof.s_commit, None),
            LabeledCommitment::new(String::from("f_prime"), proof.f_prime_commit, None),
            LabeledCommitment::new(String::from("g_prime"), proof.g_prime_commit, None),
            LabeledCommitment::new(String::from("s_prime"), proof.s_prime_commit, None),
            LabeledCommitment::new(String::from("h"), proof.h_commit, None),
        ];

        fs_rng.absorb(&to_bytes![Self::PROTOCOL_NAME, commitments].unwrap());

        let square_check_vo = SquareCheckOracle::new();

        let alphas = [F::one(), F::one()];

        // Zero over K for f_prime
        ZeroOverK::<F, PC, D>::verify(
            proof.f_prime_square_proof,
            &[f_commit.clone(), commitments[1].clone()],
            &square_check_vo,
            &domain_k,
            &alphas,
            vk,
        )?;

        // Zero over K for g_prime
        ZeroOverK::<F, PC, D>::verify(
            proof.g_prime_square_proof,
            &[g_commit.clone(), commitments[2].clone()],
            &square_check_vo,
            &domain_k,
            &alphas,
            vk,
        )?;

        // Zero over K for s_prime
        ZeroOverK::<F, PC, D>::verify(
            proof.s_prime_square_proof,
            &[commitments[0].clone(), commitments[3].clone()],
            &square_check_vo,
            &domain_k,
            &alphas,
            vk,
        )?;

        let product_check_vo = ProductCheckVO::new();
        let mut alphas = [F::one(), F::one(), F::one()];

        // Zero over K for f' = (s')*(g')
        ZeroOverK::<F, PC, D>::verify(
            proof.f_prime_product_proof,
            &[
                commitments[1].clone(),
                commitments[3].clone(),
                commitments[2].clone(),
            ],
            &product_check_vo,
            &domain_k,
            &alphas,
            vk,
        )?;

        // Geometric Sequence Test for h
        let omega: F = domain_h.element(1);
        let delta = omega.sqrt().unwrap();
        let mut a_s = vec![F::one()];
        let mut c_s = vec![domain_h.size()];

        let to_pad = domain_k.size() - domain_h.size();
        if to_pad > 0 {
            a_s.push(F::zero());
            c_s.push(to_pad);
        }
        GeoSeqTest::<F, PC, D>::verify(delta, &mut a_s, &mut c_s, &domain_k, proof.h_proof, &vk)?;

        // Non-zero over K for f′
        NonZeroOverK::<F, PC, D>::verify(
            &vk,
            &domain_k,
            commitments[1].clone(),
            proof.nzk_f_prime_proof,
        )?;

        // Non-zero over K for g′
        NonZeroOverK::<F, PC, D>::verify(
            &vk,
            &domain_k,
            commitments[2].clone(),
            proof.nzk_g_prime_proof,
        )?;

        // Non-zero over K for s′
        NonZeroOverK::<F, PC, D>::verify(
            &vk,
            &domain_k,
            commitments[3].clone(),
            proof.nzk_s_prime_proof,
        )?;

        // Non-zero over K for s(X) − 1
        let one_poly = label_polynomial!(to_poly!(F::one()));
        let (commit_to_one, _) = PC::commit(ck, &[one_poly], None).unwrap();

        let s_minus_one_commitment = PC::multi_scalar_mul(
            &[commitments[0].clone(), commit_to_one[0].clone()],
            &[F::one(), -F::one()],
        );
        let s_minus_one_commitment =
            LabeledCommitment::new(String::from("s_minus_one"), s_minus_one_commitment, None);

        NonZeroOverK::<F, PC, D>::verify(
            &vk,
            &domain_k,
            s_minus_one_commitment.clone(),
            proof.nzk_s_minus_one_proof,
        )?;

        Ok(())
    }
}