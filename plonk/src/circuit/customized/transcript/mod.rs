//! Implementing *native* circuit for rescue transcript

use super::ultraplonk::mod_arith::FpElemVar;
use crate::{
    circuit::{
        customized::{
            ecc::{PointVariable, SWToTEConParam},
            rescue::RescueGadget,
            ultraplonk::plonk_verifier::*,
        },
        Circuit, PlonkCircuit, Variable,
    },
    errors::{PlonkError, SnarkError::ParameterError},
};
use ark_ec::{short_weierstrass_jacobian::GroupAffine, PairingEngine, SWModelParameters};
use ark_ff::PrimeField;
use ark_std::{string::ToString, vec::Vec};
use core::marker::PhantomData;
use jf_rescue::{RescueParameter, STATE_SIZE};

pub struct RescueTranscriptVar<F: RescueParameter> {
    transcript_var: Vec<Variable>,
    state_var: [Variable; STATE_SIZE],
    _phantom: PhantomData<F>,
}

impl<F> RescueTranscriptVar<F>
where
    F: RescueParameter + SWToTEConParam,
{
    /// create a new RescueTranscriptVar for a given circuit.
    pub(crate) fn new(circuit: &mut PlonkCircuit<F>) -> Self {
        Self {
            transcript_var: Vec::new(),
            state_var: [circuit.zero(); STATE_SIZE],
            _phantom: PhantomData::default(),
        }
    }

    // append the verification key and the public input
    pub(crate) fn append_vk_and_pub_input_vars<E: PairingEngine<Fq = F>>(
        &mut self,
        circuit: &mut PlonkCircuit<F>,
        vk_var: &VerifyingKeyVar<E>,
        pub_input: &[FpElemVar<F>],
    ) -> Result<(), PlonkError> {
        // to enable a more efficient verifier circuit, we remove
        // the following messages (c.f. merlin transcript)
        //  - field_size_in_bits
        //  - domain size
        //  - number of inputs
        //  - wire subsets separators

        // selector commitments
        for com in vk_var.selector_comms.iter() {
            // the commitment vars are already in TE form
            self.transcript_var.push(com.get_x());
            self.transcript_var.push(com.get_y());
        }
        // sigma commitments
        for com in vk_var.sigma_comms.iter() {
            // the commitment vars are already in TE form
            self.transcript_var.push(com.get_x());
            self.transcript_var.push(com.get_y());
        }
        // public input
        for e in pub_input {
            let pub_var = e.convert_to_var(circuit)?;
            self.transcript_var.push(pub_var)
        }
        Ok(())
    }

    // Append the variable to the transcript.
    // For efficiency purpose, label is not used for rescue FS.
    pub(crate) fn append_variable(
        &mut self,
        _label: &'static [u8],
        var: &Variable,
    ) -> Result<(), PlonkError> {
        self.transcript_var.push(*var);

        Ok(())
    }

    // Append the message variables to the transcript.
    // For efficiency purpose, label is not used for rescue FS.
    pub(crate) fn append_message_vars(
        &mut self,
        _label: &'static [u8],
        msg_vars: &[Variable],
    ) -> Result<(), PlonkError> {
        for e in msg_vars.iter() {
            self.append_variable(_label, e)?;
        }

        Ok(())
    }

    // Append a commitment variable (in the form of PointVariable) to the
    // transcript. The caller needs to make sure that the commitment is
    // already converted to TE form before generating the variables.
    // For efficiency purpose, label is not used for rescue FS.
    pub(crate) fn append_commitment_var<E, P>(
        &mut self,
        _label: &'static [u8],
        poly_comm_var: &PointVariable,
    ) -> Result<(), PlonkError>
    where
        E: PairingEngine<G1Affine = GroupAffine<P>>,
        P: SWModelParameters<BaseField = F>,
    {
        // push the x and y coordinate of comm to the transcript
        self.transcript_var.push(poly_comm_var.get_x());
        self.transcript_var.push(poly_comm_var.get_y());

        Ok(())
    }

    // Append  a slice of commitment variables (in the form of PointVariable) to the
    // The caller needs to make sure that the commitment is
    // already converted to TE form before generating the variables.
    // transcript For efficiency purpose, label is not used for rescue FS.
    pub(crate) fn append_commitments_vars<E, P>(
        &mut self,
        _label: &'static [u8],
        poly_comm_vars: &[PointVariable],
    ) -> Result<(), PlonkError>
    where
        E: PairingEngine<G1Affine = GroupAffine<P>>,
        P: SWModelParameters<BaseField = F>,
    {
        for poly_comm_var in poly_comm_vars.iter() {
            // push the x and y coordinate of comm to the transcript
            self.transcript_var.push(poly_comm_var.get_x());
            self.transcript_var.push(poly_comm_var.get_y());
        }
        Ok(())
    }

    // Append a challenge variable to the transcript.
    // For efficiency purpose, label is not used for rescue FS.
    pub(crate) fn append_challenge_var(
        &mut self,
        _label: &'static [u8],
        challenge_var: &Variable,
    ) -> Result<(), PlonkError> {
        self.append_variable(_label, challenge_var)
    }

    // Append the proof evaluation to the transcript
    pub(crate) fn append_proof_evaluations_vars<E: PairingEngine>(
        &mut self,
        circuit: &mut PlonkCircuit<F>,
        evals: &ProofEvaluationsVar<F>,
    ) -> Result<(), PlonkError> {
        for e in &evals.wires_evals {
            let tmp = e.convert_to_var(circuit)?;
            self.transcript_var.push(tmp);
        }
        for e in &evals.wire_sigma_evals {
            let tmp = e.convert_to_var(circuit)?;
            self.transcript_var.push(tmp);
        }
        let tmp = evals.perm_next_eval.convert_to_var(circuit)?;
        self.transcript_var.push(tmp);
        Ok(())
    }

    // generate the challenge for the current transcript
    // and append it to the transcript
    // For efficiency purpose, label is not used for rescue FS.
    // Note that this function currently only supports bls12-377
    // curve due to its decomposition method.
    pub(crate) fn get_and_append_challenge_var<E>(
        &mut self,
        _label: &'static [u8],
        circuit: &mut PlonkCircuit<F>,
    ) -> Result<Variable, PlonkError>
    where
        E: PairingEngine,
    {
        if !circuit.support_lookup() {
            return Err(ParameterError("does not support range table".to_string()).into());
        }

        if E::Fr::size_in_bits() != 253 || E::Fq::size_in_bits() != 377 {
            return Err(ParameterError(
                "Curve Parameter does not support for rescue transcript circuit".to_string(),
            )
            .into());
        }

        // ==================================
        // This algorithm takes in 3 steps
        // 1. state: [F: STATE_SIZE] = hash(state|transcript)
        // 2. challenge = state[0] in Fr
        // 3. transcript = vec![challenge]
        // ==================================

        // step 1. state: [F: STATE_SIZE] = hash(state|transcript)
        let input_var = [self.state_var.as_ref(), self.transcript_var.as_ref()].concat();
        let res_var = circuit
            .rescue_sponge_with_padding(&input_var, STATE_SIZE)
            .unwrap();
        let out_var = res_var[0];

        // step 2. challenge = state[0] in Fr
        let challenge_var = circuit.truncate(out_var, 248)?;

        // 3. transcript = vec![challenge]
        // finish and update the states
        self.state_var.copy_from_slice(&res_var[0..STATE_SIZE]);
        self.transcript_var = Vec::new();
        self.append_challenge_var(_label, &challenge_var)?;

        Ok(challenge_var)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        circuit::customized::ecc::Point,
        proof_system::structs::VerifyingKey,
        transcript::{PlonkTranscript, RescueTranscript},
    };
    use ark_bls12_377::Bls12_377;
    use ark_ec::{AffineCurve, ProjectiveCurve};
    use ark_poly_commit::kzg10::{Commitment, VerifierKey};
    use ark_std::{format, test_rng, UniformRand};
    use jf_utils::{bytes_to_field_elements, field_switching};

    const RANGE_BIT_LEN_FOR_TEST: usize = 16;
    #[test]
    fn test_rescue_transcript_challenge_circuit() {
        test_rescue_transcript_challenge_circuit_helper::<Bls12_377, _, _>()
    }
    fn test_rescue_transcript_challenge_circuit_helper<E, F, P>()
    where
        E: PairingEngine<Fq = F, G1Affine = GroupAffine<P>>,
        F: RescueParameter + SWToTEConParam,
        P: SWModelParameters<BaseField = F>,
    {
        let mut circuit = PlonkCircuit::<F>::new_ultra_plonk(RANGE_BIT_LEN_FOR_TEST);

        let label = "testing".as_ref();

        let mut transcipt_var = RescueTranscriptVar::new(&mut circuit);
        let mut transcript = RescueTranscript::<F>::new(label);

        for _ in 0..10 {
            for i in 0..10 {
                let msg = format!("message {}", i);
                let vals = bytes_to_field_elements(&msg);
                let message_vars: Vec<Variable> = vals
                    .iter()
                    .map(|x| circuit.create_variable(*x).unwrap())
                    .collect();

                transcript.append_message(label, msg.as_bytes()).unwrap();

                transcipt_var
                    .append_message_vars(label, &message_vars)
                    .unwrap();
            }

            let challenge = transcript.get_and_append_challenge::<E>(label).unwrap();

            let challenge_var = transcipt_var
                .get_and_append_challenge_var::<E>(label, &mut circuit)
                .unwrap();

            assert_eq!(
                circuit.witness(challenge_var).unwrap().into_repr(),
                field_switching::<_, F>(&challenge).into_repr()
            );
        }
    }

    #[test]
    fn test_rescue_transcript_append_vk_and_input_circuit() {
        test_rescue_transcript_append_vk_and_input_circuit_helper::<Bls12_377, _, _>()
    }
    fn test_rescue_transcript_append_vk_and_input_circuit_helper<E, F, P>()
    where
        E: PairingEngine<Fq = F, G1Affine = GroupAffine<P>>,
        F: RescueParameter + SWToTEConParam,
        P: SWModelParameters<BaseField = F> + Clone,
    {
        let mut circuit = PlonkCircuit::<F>::new_ultra_plonk(RANGE_BIT_LEN_FOR_TEST);

        let mut rng = test_rng();

        let label = "testing".as_ref();

        let mut transcript_var = RescueTranscriptVar::new(&mut circuit);
        let mut transcript = RescueTranscript::<F>::new(label);

        let open_key: VerifierKey<E> = VerifierKey {
            g: E::G1Affine::prime_subgroup_generator(),
            gamma_g: E::G1Projective::rand(&mut rng).into_affine(),
            h: E::G2Affine::prime_subgroup_generator(),
            beta_h: E::G2Projective::rand(&mut rng).into_affine(),
            prepared_h: E::G2Affine::prime_subgroup_generator().into(),
            prepared_beta_h: E::G2Projective::rand(&mut rng).into_affine().into(),
        };

        let dummy_vk = VerifyingKey {
            domain_size: 512,
            num_inputs: 0,
            sigma_comms: Vec::new(),
            selector_comms: Vec::new(),
            k: Vec::new(),
            open_key: open_key.clone(),
            is_merged: false,
            plookup_vk: None,
        };

        let dummy_vk_var = VerifyingKeyVar::new(&mut circuit, &dummy_vk).unwrap();

        // build challenge from transcript and check for correctness
        transcript.append_vk_and_pub_input(&dummy_vk, &[]).unwrap();
        transcript_var
            .append_vk_and_pub_input_vars::<E>(&mut circuit, &dummy_vk_var, &[])
            .unwrap();

        let challenge = transcript.get_and_append_challenge::<E>(label).unwrap();

        let challenge_var = transcript_var
            .get_and_append_challenge_var::<E>(label, &mut circuit)
            .unwrap();

        assert_eq!(
            circuit.witness(challenge_var).unwrap(),
            field_switching(&challenge)
        );

        for _ in 0..10 {
            // inputs
            let input: Vec<E::Fr> = (0..16).map(|_| E::Fr::rand(&mut rng)).collect();

            // sigma commitments
            let sigma_comms: Vec<Commitment<E>> = (0..42)
                .map(|_| Commitment(E::G1Projective::rand(&mut rng).into_affine()))
                .collect();
            let mut sigma_comms_vars: Vec<PointVariable> = Vec::new();
            for e in sigma_comms.iter() {
                // convert point into TE form
                let p: Point<F> = (&e.0).into();
                sigma_comms_vars.push(circuit.create_point_variable(p).unwrap());
            }

            // selector commitments
            let selector_comms: Vec<Commitment<E>> = (0..33)
                .map(|_| Commitment(E::G1Projective::rand(&mut rng).into_affine()))
                .collect();
            let mut selector_comms_vars: Vec<PointVariable> = Vec::new();
            for e in selector_comms.iter() {
                // convert point into TE form
                let p: Point<F> = (&e.0).into();
                selector_comms_vars.push(circuit.create_point_variable(p).unwrap());
            }

            // k
            let k: Vec<E::Fr> = (0..5).map(|_| E::Fr::rand(&mut rng)).collect();

            let vk = VerifyingKey {
                domain_size: 512,
                num_inputs: input.len(),
                sigma_comms,
                selector_comms,
                k,
                open_key: open_key.clone(),
                is_merged: false,
                plookup_vk: None,
            };
            let vk_var = VerifyingKeyVar::new(&mut circuit, &vk).unwrap();

            // build challenge from transcript and check for correctness
            transcript.append_vk_and_pub_input(&vk, &input).unwrap();
            let m = 128;
            let input_vars: Vec<Variable> = input
                .iter()
                .map(|&x| circuit.create_public_variable(field_switching(&x)).unwrap())
                .collect();

            let input_fp_elem_vars: Vec<FpElemVar<F>> = input_vars
                .iter()
                .map(|&x| FpElemVar::new_unchecked(&mut circuit, x, m, None).unwrap())
                .collect();
            transcript_var
                .append_vk_and_pub_input_vars::<E>(&mut circuit, &vk_var, &input_fp_elem_vars)
                .unwrap();

            let challenge = transcript.get_and_append_challenge::<E>(label).unwrap();

            let challenge_var = transcript_var
                .get_and_append_challenge_var::<E>(label, &mut circuit)
                .unwrap();

            assert_eq!(
                circuit.witness(challenge_var).unwrap(),
                field_switching(&challenge)
            );
        }
    }
}
