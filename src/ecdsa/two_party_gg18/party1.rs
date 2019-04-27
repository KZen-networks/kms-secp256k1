#![allow(non_snake_case)]
/*
    KMS-ECDSA
    Copyright 2018 by Kzen Networks
    This file is part of KMS library
    (https://github.com/KZen-networks/kms)
    Cryptography utilities is free software: you can redistribute
    it and/or modify it under the terms of the GNU General Public
    License as published by the Free Software Foundation, either
    version 3 of the License, or (at your option) any later version.
    @license GPL-3.0+ <https://github.com/KZen-networks/kms/blob/master/LICENSE>
*/

use super::hd_key;
use super::*;
use curv::cryptographic_primitives::proofs::sigma_correct_homomorphic_elgamal_enc::HomoELGamalProof;
use curv::cryptographic_primitives::secret_sharing::feldman_vss::VerifiableSS;
use curv::elliptic::curves::traits::ECPoint;
use curv::{BigInt, FE, GE};
use ecdsa::two_party_gg18::{MasterKey1, MasterKeyPublic};
use ecdsa::two_party_lindell17::MasterKey2 as MasterKey2L;
use multi_party_ecdsa::protocols::multi_party_ecdsa::gg_2018::mta::{MessageA, MessageB};
use paillier::EncryptionKey;
use rotation::two_party::Rotation;

#[derive(Clone, Serialize, Deserialize)]
pub struct KeyGenMessage0Party1Transform {
    pub p1m1: KeyGenMessage1,
    pub message_b: MessageB,
}

impl MasterKey1 {
    pub fn key_gen_zero_message_transform(
        lindell_mk2: &MasterKey2L,
    ) -> (KeyGenMessage0Party1Transform, Keys, KeyGenDecommitMessage1) {
        let (message_b, beta) = lindell_mk2
            .private
            .to_mta_message_b(&lindell_mk2.public.paillier_pub, &lindell_mk2.public.c_key);
        let party_keys = Keys::create_from(beta, 1 as usize);
        let (bc_i, decom_i) = party_keys.phase1_broadcast_phase3_proof_of_correct_key();
        let party1_message1 = KeyGenMessage1 { bc_i };
        let party1_message1 = KeyGenMessage0Party1Transform {
            p1m1: party1_message1,
            message_b,
        };
        (party1_message1, party_keys, decom_i)
    }

    pub fn key_gen_first_message(u: FE) -> (KeyGenMessage1, Keys, KeyGenDecommitMessage1) {
        let party_keys = if u == FE::zero() {
            Keys::create(1 as usize)
        } else {
            Keys::create_from(u, 1 as usize)
        };
        let (bc_i, decom_i) = party_keys.phase1_broadcast_phase3_proof_of_correct_key();
        let party1_message1 = KeyGenMessage1 { bc_i };
        (party1_message1, party_keys, decom_i)
    }

    pub fn keygen_second_message(decom_i: KeyGenDecommitMessage1) -> KeyGenMessage2 {
        KeyGenMessage2 { decom_i }
    }

    pub fn key_gen_third_message(
        party1_keys: &Keys,
        party1_message1: KeyGenMessage1,
        party2_message1: KeyGenMessage1,
        party1_message2: KeyGenMessage2,
        party2_message2: KeyGenMessage2,
    ) -> (KeyGenMessage3, FE, Vec<GE>, Vec<EncryptionKey>) {
        let y_slice = &[party1_keys.y_i, party2_message2.decom_i.y_i];
        let decom_slice = &[party1_message2.decom_i, party2_message2.decom_i];
        let bc1_slice = &[party1_message1.bc_i.clone(), party2_message1.bc_i.clone()];
        let paillier_enc_slice = &[
            party1_message1.bc_i.e.clone(),
            party2_message1.bc_i.e.clone(),
        ];
        let parames = Parameters {
            threshold: 1 as usize,
            share_count: 2 as usize,
        };
        let (vss_scheme, secret_shares, _index) = party1_keys
            .phase1_verify_com_phase3_verify_correct_key_phase2_distribute(
                &parames,
                &decom_slice.to_vec(),
                &bc1_slice.to_vec(),
            )
            .expect("invalid key");
        let key_gen_message3 = KeyGenMessage3 {
            vss_scheme,
            secret_share: secret_shares[1],
        };
        (
            key_gen_message3,
            secret_shares[0],
            y_slice.to_vec(),
            paillier_enc_slice.to_vec(),
        )
    }

    pub fn key_gen_fourth_message(
        party1_keys: &Keys,
        party1_message3: KeyGenMessage3,
        party2_message3: KeyGenMessage3,
        party1_ss_share_0: FE,
        y_vec: &Vec<GE>,
    ) -> (KeyGenMessage4, SharedKeys, Vec<VerifiableSS>) {
        let params = Parameters {
            threshold: 1 as usize,
            share_count: 2 as usize,
        };
        let vss_slice = &[party1_message3.vss_scheme, party2_message3.vss_scheme];
        let ss_slice = &[party1_ss_share_0, party2_message3.secret_share];
        let (shared_keys, dlog_proof) = party1_keys
            .phase2_verify_vss_construct_keypair_phase3_pok_dlog(
                &params,
                y_vec,
                &ss_slice.to_vec(),
                &vss_slice.to_vec(),
                &(1 as usize),
            )
            .expect("invalid vss");

        let party1_message4 = KeyGenMessage4 { dlog_proof };
        (party1_message4, shared_keys, vss_slice.to_vec())
    }

    pub fn set_master_key(
        party1_message4: KeyGenMessage4,
        party2_message4: KeyGenMessage4,
        y_vec: Vec<GE>,
        party1_keys: Keys,
        party1_shared_keys: SharedKeys,
        vss_vec: Vec<VerifiableSS>,
        paillier_enc_vec: Vec<EncryptionKey>,
        chain_code: &BigInt,
    ) -> Self {
        let dlog_proof = [party1_message4.dlog_proof, party2_message4.dlog_proof].to_vec();

        let parames = Parameters {
            threshold: 1 as usize,
            share_count: 2 as usize,
        };
        Keys::verify_dlog_proofs(&parames, &dlog_proof, &y_vec).expect("bad dlog proof");

        let master_key_public = MasterKeyPublic {
            q: y_vec[0] + y_vec[1],
            vss_scheme_vec: vss_vec,
            paillier_key_vec: paillier_enc_vec,
        };

        let master_key_private = PartyPrivate::set_private(party1_keys, party1_shared_keys);

        let master_key1 = MasterKey1 {
            public: master_key_public,
            private: master_key_private,
            chain_code: chain_code.clone(),
        };
        master_key1
    }

    pub fn sign_first_message(&self) -> (SignMessage1, SignDecommitPhase1, SignKeys) {
        let index: usize = 0;
        let index_list = [0 as usize, 1 as usize].to_vec();

        let sign_keys = SignKeys::create(
            &self.private,
            &self.public.vss_scheme_vec[0],
            index,
            &index_list,
        );

        let (com, decommit) = sign_keys.phase1_broadcast();
        let m_a_k = MessageA::a(&sign_keys.k_i, &self.public.paillier_key_vec[0]);

        let sign_message1 = SignMessage1 { com, m_a_k };
        (sign_message1, decommit, sign_keys)
    }

    pub fn sign_second_message(
        &self,
        party2_message1: &SignMessage1,
        party1_sign_keys: &SignKeys,
    ) -> (SignMessage2, FE, FE) {
        let (m_b_gamma, beta) = MessageB::b(
            &party1_sign_keys.gamma_i,
            &self.public.paillier_key_vec[1],
            party2_message1.m_a_k.clone(),
        );
        let (m_b_w, ni) = MessageB::b(
            &party1_sign_keys.w_i,
            &&self.public.paillier_key_vec[1],
            party2_message1.m_a_k.clone(),
        );
        let party1_message2 = SignMessage2 { m_b_gamma, m_b_w };

        (party1_message2, beta, ni)
    }

    pub fn sign_third_message(
        &self,
        party2_message2: &SignMessage2,
        party1_sign_keys: &SignKeys,
        beta: FE,
        ni: FE,
    ) -> (SignMessage3, FE) {
        let alpha = party2_message2
            .m_b_gamma
            .verify_proofs_get_alpha_gg18(&self.private, &party1_sign_keys.k_i)
            .expect("wrong dlog or m_b");;
        let miu = party2_message2
            .m_b_w
            .verify_proofs_get_alpha_gg18(&self.private, &party1_sign_keys.k_i)
            .expect("wrong dlog or m_b");;

        let index: usize = 1;
        let index_list = [0 as usize, 1 as usize].to_vec();
        let xi_com_vec = Keys::get_commitments_to_xi(&self.public.vss_scheme_vec);
        let x1_com = xi_com_vec[1];
        let g_w_i = Keys::update_commitments_to_xi(
            &x1_com,
            &self.public.vss_scheme_vec[1],
            index,
            &index_list,
        );
        assert_eq!(party2_message2.m_b_w.b_proof.pk.clone(), g_w_i);

        let delta = party1_sign_keys.phase2_delta_i(&[alpha].to_vec(), &[beta].to_vec());
        let sigma = party1_sign_keys.phase2_sigma_i(&[miu].to_vec(), &[ni].to_vec());

        let sign_message3 = SignMessage3 { delta };
        (sign_message3, sigma)
    }

    pub fn sign_fourth_message(decommit: SignDecommitPhase1) -> SignMessage4 {
        SignMessage4 { decommit }
    }

    pub fn sign_fifth_message(
        &self,
        message: BigInt,
        sigma: FE,
        party1_sign_keys: &SignKeys,
        party1_message4: SignMessage4,
        party1_message3: SignMessage3,
        party2_message3: SignMessage3,
        party2_message4: SignMessage4,
        party2_message2: SignMessage2,
        party2_message1: SignMessage1,
    ) -> (
        SignMessage5,
        Phase5ADecom1,
        HomoELGamalProof,
        LocalSignature,
        GE,
    ) {
        let delta_slice = &[party1_message3.delta, party2_message3.delta];
        let delta_inv = SignKeys::phase3_reconstruct_delta(&delta_slice.to_vec());

        let b_proof = party2_message2.m_b_gamma.b_proof;
        let R = SignKeys::phase4(
            &delta_inv,
            &[&b_proof].to_vec(),
            [party2_message4.decommit.clone()].to_vec(),
            &[party2_message1.com].to_vec(),
        )
        .expect("bad gamma_i decommit");
        let R = R + party1_message4.decommit.g_gamma_i * &delta_inv;

        let local_sig = LocalSignature::phase5_local_sig(
            &party1_sign_keys.k_i,
            &message,
            &R,
            &sigma,
            &self.public.q,
        );

        let (phase5_com, phase_5a_decom, helgamal_proof) = local_sig.phase5a_broadcast_5b_zkproof();
        let sign_message5 = SignMessage5 { phase5_com };
        (sign_message5, phase_5a_decom, helgamal_proof, local_sig, R)
    }

    pub fn sign_sixth_message(
        phase_5a_decom: Phase5ADecom1,
        helgamal_proof: HomoELGamalProof,
    ) -> SignMessage6 {
        SignMessage6 {
            phase_5a_decom,
            helgamal_proof,
        }
    }

    pub fn sign_seventh_message(
        party1_message6: SignMessage6,
        party2_message6: SignMessage6,
        party2_message5: SignMessage5,
        local_sig: &LocalSignature,
        R: GE,
    ) -> (SignMessage7, Phase5DDecom2) {
        let (phase5_com2, phase_5d_decom2) = local_sig
            .phase5c(
                &[party2_message6.phase_5a_decom].to_vec(),
                &[party2_message5.phase5_com].to_vec(),
                &[party2_message6.helgamal_proof].to_vec(),
                &party1_message6.phase_5a_decom.V_i,
                &R,
            )
            .expect("error phase5");

        let sign_message7 = SignMessage7 { phase5_com2 };
        (sign_message7, phase_5d_decom2)
    }

    pub fn sign_eighth_message(phase_5d_decom2: Phase5DDecom2) -> SignMessage8 {
        SignMessage8 { phase_5d_decom2 }
    }

    pub fn sign_ninth_message(
        party1_message6: SignMessage6,
        party2_message6: SignMessage6,
        party1_message7: SignMessage7,
        party2_message7: SignMessage7,
        party1_message8: SignMessage8,
        party2_message8: SignMessage8,
        local_sig: &LocalSignature,
    ) -> SignMessage9 {
        let phase5a_decom_slice = [
            party1_message6.phase_5a_decom,
            party2_message6.phase_5a_decom,
        ];
        let phase5d_com_slice = [party1_message7.phase5_com2, party2_message7.phase5_com2];
        let phase5d_decom_slice = [
            party1_message8.phase_5d_decom2,
            party2_message8.phase_5d_decom2,
        ];

        let s_i = local_sig
            .phase5d(
                &phase5d_decom_slice.to_vec(),
                &phase5d_com_slice.to_vec(),
                &phase5a_decom_slice.to_vec(),
            )
            .expect("bad com 5d");

        let sign_message9 = SignMessage9 { s_i };
        sign_message9
    }

    pub fn output_signature(party2_message9: SignMessage9, local_sig: LocalSignature) -> Signature {
        let message9_vec = [party2_message9.s_i].to_vec();
        let sig = local_sig
            .output_signature(&message9_vec)
            .expect("verification failed");
        sig
    }

    pub fn rotation_first_message(
        &self,
        cf: &Rotation,
    ) -> (KeyGenMessage1, Keys, KeyGenDecommitMessage1) {
        let party1_keys = PartyPrivate::refresh_private_key(&self.private, &cf.rotation, 1);
        let (bc_i, decom_i) = party1_keys.phase1_broadcast_phase3_proof_of_correct_key();
        let party1_message1 = KeyGenMessage1 { bc_i };
        (party1_message1, party1_keys, decom_i)
    }

    pub fn rotation_second_message(decom_i: KeyGenDecommitMessage1) -> KeyGenMessage2 {
        KeyGenMessage2 { decom_i }
    }

    pub fn rotation_third_message(
        &self,
        party1_keys: &Keys,
        party1_message1: KeyGenMessage1,
        party2_message1: KeyGenMessage1,
        party1_message2: KeyGenMessage2,
        party2_message2: KeyGenMessage2,
    ) -> (KeyGenMessage3, FE, Vec<GE>, Vec<EncryptionKey>) {
        // make sure rotation of counter party is correct:
        let y_sum_new = party1_keys.y_i.clone() + party2_message2.decom_i.y_i.clone();
        assert_eq!(y_sum_new, self.public.q.clone());
        MasterKey1::key_gen_third_message(
            party1_keys,
            party1_message1,
            party2_message1,
            party1_message2,
            party2_message2,
        )
    }

    pub fn rotation_fourth_message(
        party1_keys: &Keys,
        party1_message3: KeyGenMessage3,
        party2_message3: KeyGenMessage3,
        party1_ss_share_0: FE,
        y_vec: &Vec<GE>,
    ) -> (KeyGenMessage4, SharedKeys, Vec<VerifiableSS>) {
        MasterKey1::key_gen_fourth_message(
            party1_keys,
            party1_message3,
            party2_message3,
            party1_ss_share_0,
            y_vec,
        )
    }

    pub fn rotate_master_key(
        &self,
        party1_message4: KeyGenMessage4,
        party2_message4: KeyGenMessage4,
        y_vec: Vec<GE>,
        party1_keys: Keys,
        party1_shared_keys: SharedKeys,
        vss_vec: Vec<VerifiableSS>,
        paillier_enc_vec: Vec<EncryptionKey>,
    ) -> Self {
        MasterKey1::set_master_key(
            party1_message4,
            party2_message4,
            y_vec,
            party1_keys,
            party1_shared_keys,
            vss_vec,
            paillier_enc_vec,
            &self.chain_code,
        )
    }

    pub fn get_child(&self, location_in_hir: Vec<BigInt>) -> MasterKey1 {
        let (public_key_new_child, f_l_new, cc_new) =
            hd_key(location_in_hir, &self.public.q, &self.chain_code);

        // optimize!
        let g: GE = ECPoint::generator();
        let com_zero_new = self.public.vss_scheme_vec[0].commitments[0] + g * f_l_new;
        let mut com_iter_unchanged = self.public.vss_scheme_vec[0].commitments.iter();
        let _ = com_iter_unchanged.next().unwrap();
        let com_vec_new = (0..self.public.vss_scheme_vec[1].commitments.len())
            .map(|i| {
                if i == 0 {
                    com_zero_new
                } else {
                    com_iter_unchanged.next().unwrap().clone()
                }
            })
            .collect::<Vec<GE>>();
        let new_vss = VerifiableSS {
            parameters: self.public.vss_scheme_vec[0].parameters.clone(),
            commitments: com_vec_new,
        };
        let new_vss_vec = [new_vss, self.public.vss_scheme_vec[1].clone()];

        let master_key_public = MasterKeyPublic {
            q: public_key_new_child,
            vss_scheme_vec: new_vss_vec.to_vec(),
            paillier_key_vec: self.public.paillier_key_vec.clone(),
        };

        let master_key_private = self.private.update_private_key(&f_l_new, &f_l_new);

        let master_key1 = MasterKey1 {
            public: master_key_public,
            private: master_key_private,
            chain_code: cc_new.bytes_compressed_to_big_int(),
        };
        master_key1
    }
}
