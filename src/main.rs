#![feature(generic_const_exprs)]
use circuit_definitions::circuit_definitions::recursion_layer::scheduler::ConcreteSchedulerCircuitBuilder;
use clap::Parser;
use colored::Colorize;
use std::fs::File;
use std::io::Read;

mod requests;

use boojum::{
    cs::implementations::{
        pow::NoPow, transcript::GoldilocksPoisedon2Transcript, verifier::VerificationKey,
    },
    field::goldilocks::{GoldilocksExt2, GoldilocksField},
};
use circuit_definitions::circuit_definitions::{
    base_layer::{BaseProofsTreeHasher, ZkSyncBaseLayerProof},
    recursion_layer::{ZkSyncRecursionLayerProof, ZkSyncRecursionLayerStorage},
};

#[derive(serde::Serialize, serde::Deserialize)]
pub enum FriProofWrapper {
    Base(ZkSyncBaseLayerProof),
    Recursive(ZkSyncRecursionLayerProof),
}

#[derive(Debug, Parser)]
#[command(author = "Matter Labs", version, about = "Boojum CLI verifier", long_about = None)]
struct Cli {
    #[arg(long)]
    // Path to the .bin file with the proof
    proof: Option<String>,
    #[arg(long)]
    // Batch number to check proof for
    batch: Option<usize>,
    #[arg(long, default_value = "mainnet")]
    // Batch number to check proof for
    network: String,
    #[arg(long)]
    // RPC endpoint to use to fetch L1 information
    l1_rpc: Option<String>,
}


/// Reads proof (in FriProofWrapper format) from a given bin file.
pub fn proof_from_file<T: for<'a> serde::Deserialize<'a>>(proof_path: &str) -> T {
    let mut file = File::open(proof_path).unwrap();
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).unwrap();

    let proof: T = bincode::deserialize(buffer.as_slice()).unwrap();
    proof
}

/// Verifies a given proof from "Scheduler" circuit.
pub fn verify_scheduler_proof(proof_path: &str) -> anyhow::Result<String> {
    let scheduler_key: ZkSyncRecursionLayerStorage<
        VerificationKey<GoldilocksField, BaseProofsTreeHasher>,
    > = serde_json::from_slice(include_bytes!("keys/verification_scheduler_key.json")).unwrap();

    let proof = proof_from_file(proof_path);
    if let FriProofWrapper::Recursive(proof) = proof {
        println!("Proof type: {}", proof.short_description().bold());
        let verifier_builder =
            ConcreteSchedulerCircuitBuilder::dyn_verifier_builder::<GoldilocksExt2>();

        let verifier = verifier_builder.create_verifier();
        let result = verifier.verify::<BaseProofsTreeHasher, GoldilocksPoisedon2Transcript, NoPow>(
            (),
            &scheduler_key.into_inner(),
            &proof.into_inner(),
        );
        if result {
            Ok("Pass".to_string())
        } else {
            anyhow::bail!("Invalid proof")
        }
    } else {
        anyhow::bail!("Invalid proof type")
    }
}

#[tokio::main]
async fn main() {
    let opt = Cli::parse();

    let batch_number = &opt.batch;
    let proof;
    let network = &opt.network;
    let l1_rpc = &opt.l1_rpc;

    if network.to_string() != "testnet" && network.to_string() != "mainnet" {
        println!("Invalid network name. Please use 'testnet' or 'mainnet'");
        return
    }

    if !batch_number.is_none() {

        let proof_response = requests::fetch_proof_from_storage(batch_number.unwrap(), network.to_string()).await;

        if let Err(_err) = proof_response {
            println!("{}", _err);
            return
        }

        if l1_rpc.is_none() {
            println!("Skipping building batch information from Ethereum as no RPC url was provided");
        } else {
            let tx_hash = requests::fetch_batch_info(batch_number.unwrap(), network.to_string()).await;
            if let Err(_err) = tx_hash {
                println!("{}", _err);
                return
            }
            let l1_batch_info = requests::fetch_l1_info(tx_hash.unwrap(), l1_rpc.clone().unwrap(), batch_number.unwrap(), network.to_string()).await;
            if let Err(_err) = l1_batch_info {
                println!("{}", _err);
                return
            }

            //TODO: Reform block header with proof information and compare with L1 information
        }
       
        proof = proof_response.unwrap()
    } else {
        proof = (&opt.proof).clone().unwrap();
    }
    
    let result = verify_scheduler_proof(&proof);

    println!(
        "Proof result: {}",
        if result.is_ok() {
            "PASS".green()
        } else {
            "FAIL".red()
        }
    );
}

#[cfg(test)]

mod test {
    use circuit_definitions::{
        circuit_definitions::{
            base_layer::ZkSyncBaseLayerStorage,
            recursion_layer::node_layer::ConcreteNodeLayerCircuitBuilder,
            verifier_builder::StorageApplicationVerifierBuilder,
        },
        ZkSyncDefaultRoundFunction,
    };

    use super::*;
    #[test]
    fn test_scheduler_proof() {
        verify_scheduler_proof("example_proofs/proof_52272951.bin").expect("FAILED");
    }
    #[test]

    fn test_basic_proof() {
        // '10' is the id of the 'Storage Application' circuit (which is the one for which we have the basic_proof.bin)
        let key_10: ZkSyncBaseLayerStorage<VerificationKey<GoldilocksField, BaseProofsTreeHasher>> =
            serde_json::from_slice(include_bytes!("keys/verification_basic_10_key.json")).unwrap();

        let proof: ZkSyncBaseLayerProof = proof_from_file("example_proofs/basic_proof.bin");

        println!("Proof type: {}", proof.short_description().bold());

        let verifier_builder = StorageApplicationVerifierBuilder::<
            GoldilocksField,
            ZkSyncDefaultRoundFunction,
        >::dyn_verifier_builder::<GoldilocksExt2>();
        let verifier = verifier_builder.create_verifier();

        let result = verifier.verify::<BaseProofsTreeHasher, GoldilocksPoisedon2Transcript, NoPow>(
            (),
            &key_10.into_inner(),
            &proof.into_inner(),
        );

        assert!(result, "Proof failed");
    }
    #[test]

    fn test_leaf_proof() {
        // '13' is the id of the Leaf for Events sorter.
        let leaf_13: ZkSyncRecursionLayerStorage<
            VerificationKey<GoldilocksField, BaseProofsTreeHasher>,
        > = serde_json::from_slice(include_bytes!("keys/verification_leaf_13_key.json")).unwrap();

        let proof: ZkSyncRecursionLayerProof = proof_from_file("example_proofs/leaf_proof.bin");
        println!("Proof type: {}", proof.short_description().bold());

        let verifier_builder =
            ConcreteNodeLayerCircuitBuilder::dyn_verifier_builder::<GoldilocksExt2>();

        let verifier = verifier_builder.create_verifier();
        let result = verifier.verify::<BaseProofsTreeHasher, GoldilocksPoisedon2Transcript, NoPow>(
            (),
            &leaf_13.into_inner(),
            &proof.into_inner(),
        );

        assert!(result, "Proof failed");
    }
    #[test]

    fn test_node_proof() {
        let node: ZkSyncRecursionLayerStorage<
            VerificationKey<GoldilocksField, BaseProofsTreeHasher>,
        > = serde_json::from_slice(include_bytes!("keys/verification_node_key.json")).unwrap();

        let proof: ZkSyncRecursionLayerProof = proof_from_file("example_proofs/node_proof.bin");
        println!("Proof type: {}", proof.short_description().bold());
        let verifier_builder =
            ConcreteNodeLayerCircuitBuilder::dyn_verifier_builder::<GoldilocksExt2>();

        let verifier = verifier_builder.create_verifier();
        let result = verifier.verify::<BaseProofsTreeHasher, GoldilocksPoisedon2Transcript, NoPow>(
            (),
            &node.into_inner(),
            &proof.into_inner(),
        );
        assert!(result, "Proof failed");
    }
}
