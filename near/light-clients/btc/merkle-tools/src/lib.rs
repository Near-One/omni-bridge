pub use btc_types::hash::{double_sha256, H256};

pub fn merkle_proof_calculator(tx_hashes: Vec<H256>, transaction_position: usize) -> Vec<H256> {
    let mut transaction_position = transaction_position;
    let mut merkle_proof = Vec::new();
    let mut current_hashes = tx_hashes;

    while current_hashes.len() > 1 {
        if current_hashes.len() % 2 == 1 {
            current_hashes.push(current_hashes[current_hashes.len() - 1].clone())
        }

        if transaction_position % 2 == 1 {
            merkle_proof.push(current_hashes[transaction_position - 1].clone());
        } else {
            merkle_proof.push(current_hashes[transaction_position + 1].clone());
        }

        let mut new_hashes = Vec::new();

        for i in (0..current_hashes.len() - 1).step_by(2) {
            new_hashes.push(compute_hash(&current_hashes[i], &current_hashes[i + 1]));
        }

        current_hashes = new_hashes;
        transaction_position /= 2;
    }

    merkle_proof
}

pub fn compute_root_from_merkle_proof(
    transaction_hash: H256,
    transaction_position: usize,
    merkle_proof: &Vec<H256>,
) -> H256 {
    let mut current_hash = transaction_hash;
    let mut current_position = transaction_position;

    for proof_hash in merkle_proof {
        if current_position % 2 == 0 {
            current_hash = compute_hash(&current_hash, proof_hash);
        } else {
            current_hash = compute_hash(proof_hash, &current_hash);
        }
        current_position /= 2;
    }

    current_hash
}

fn compute_hash(first_tx_hash: &H256, second_tx_hash: &H256) -> H256 {
    let mut concat_inputs = Vec::with_capacity(64);
    concat_inputs.extend(first_tx_hash.0);
    concat_inputs.extend(second_tx_hash.0);

    double_sha256(&concat_inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_hex(hex: &str) -> H256 {
        hex.parse().unwrap()
    }

    // Hash pairs of items recursively until a single value is obtained
    fn merkle_root_calculator(hash_list: &Vec<H256>) -> H256 {
        if hash_list.len() == 1 {
            return hash_list[0].clone();
        }

        let mut new_hash_list = Vec::new();

        // Process pairs. For odd length, the last is skipped
        for i in (0..hash_list.len() - 1).step_by(2) {
            new_hash_list.push(compute_hash(&hash_list[i], &hash_list[i + 1]));
        }

        // If list length is odd, we must hash a last item twice
        if hash_list.len() % 2 == 1 {
            new_hash_list.push(compute_hash(
                &hash_list[hash_list.len() - 1],
                &hash_list[hash_list.len() - 1],
            ));
        }

        merkle_root_calculator(&new_hash_list)
    }

    #[test]
    fn test_merkle_root_calculation() {
        let tx_hashes = vec![
            decode_hex("18afbf37d136ff62644b231fcde72f1fb8edd04a798fb00cb06360da635da275"),
            decode_hex("30b19832a5f4b952e151de77d96139987492becc8b6e1e914c4103cfbb06c01e"),
            decode_hex("b94ed12902e35b29dd53cf25e665b4d0bc92f22adbc383ad90566584902b061d"),
            decode_hex("1920e5d8a10018dc65308bb4d1f11d30b5406c6499688443bfcd1ef364206b14"),
            decode_hex("048f3897c16bdc59ec1187aa080a4b4aa5ec1afcb4b776cf8b8a214b01990a7b"),
            decode_hex("266a660e2be5f2fdf41ae21d5a29c4db6270b2686dfe3902bd2dd3bca3626d7c"),
            decode_hex("17c3b888226ce70908303eaecb88ba02aa5ab858fade8576261b1203c6885528"),
            decode_hex("8a06d54b8b411e99b7e4d60c330b8cde4feb23d62edfc25047c4d837dfb5b253"),
        ];

        let expected_merkle_root =
            decode_hex("7c8708d1f517caf3082d95cf1f6ced11a009318338e720ecee58a2b4e643d56a");
        let calculated_merkle_root = merkle_root_calculator(&tx_hashes);
        assert_eq!(calculated_merkle_root, expected_merkle_root);
    }

    #[test]
    fn test_merkle_proof_calculation() {
        let tx_hashes = vec![
            decode_hex("18afbf37d136ff62644b231fcde72f1fb8edd04a798fb00cb06360da635da275"),
            decode_hex("30b19832a5f4b952e151de77d96139987492becc8b6e1e914c4103cfbb06c01e"),
            decode_hex("b94ed12902e35b29dd53cf25e665b4d0bc92f22adbc383ad90566584902b061d"),
            decode_hex("1920e5d8a10018dc65308bb4d1f11d30b5406c6499688443bfcd1ef364206b14"),
            decode_hex("048f3897c16bdc59ec1187aa080a4b4aa5ec1afcb4b776cf8b8a214b01990a7b"),
            decode_hex("266a660e2be5f2fdf41ae21d5a29c4db6270b2686dfe3902bd2dd3bca3626d7c"),
            decode_hex("17c3b888226ce70908303eaecb88ba02aa5ab858fade8576261b1203c6885528"),
            decode_hex("8a06d54b8b411e99b7e4d60c330b8cde4feb23d62edfc25047c4d837dfb5b253"),
        ];

        let calculated_merkle_proof = merkle_proof_calculator(tx_hashes, 0);
        assert_eq!(calculated_merkle_proof.len(), 3);
    }

    #[test]
    fn test_merkle_proof_verification() {
        let tx_hashes = vec![
            decode_hex("18afbf37d136ff62644b231fcde72f1fb8edd04a798fb00cb06360da635da275"),
            decode_hex("30b19832a5f4b952e151de77d96139987492becc8b6e1e914c4103cfbb06c01e"),
            decode_hex("b94ed12902e35b29dd53cf25e665b4d0bc92f22adbc383ad90566584902b061d"),
            decode_hex("1920e5d8a10018dc65308bb4d1f11d30b5406c6499688443bfcd1ef364206b14"),
            decode_hex("048f3897c16bdc59ec1187aa080a4b4aa5ec1afcb4b776cf8b8a214b01990a7b"),
            decode_hex("266a660e2be5f2fdf41ae21d5a29c4db6270b2686dfe3902bd2dd3bca3626d7c"),
            decode_hex("17c3b888226ce70908303eaecb88ba02aa5ab858fade8576261b1203c6885528"),
            decode_hex("8a06d54b8b411e99b7e4d60c330b8cde4feb23d62edfc25047c4d837dfb5b253"),
        ];

        let calculated_merkle_root = merkle_root_calculator(&tx_hashes);
        let calculated_merkle_proof = merkle_proof_calculator(tx_hashes, 0);

        let computed_root_from_merkle_proof = compute_root_from_merkle_proof(
            decode_hex("18afbf37d136ff62644b231fcde72f1fb8edd04a798fb00cb06360da635da275"),
            0,
            &calculated_merkle_proof,
        );
        assert_eq!(computed_root_from_merkle_proof, calculated_merkle_root);
    }

    #[test]
    fn test_merkle_proof_verification_odd() {
        let tx_hashes = vec![
            decode_hex("18afbf37d136ff62644b231fcde72f1fb8edd04a798fb00cb06360da635da275"),
            decode_hex("30b19832a5f4b952e151de77d96139987492becc8b6e1e914c4103cfbb06c01e"),
            decode_hex("b94ed12902e35b29dd53cf25e665b4d0bc92f22adbc383ad90566584902b061d"),
            decode_hex("1920e5d8a10018dc65308bb4d1f11d30b5406c6499688443bfcd1ef364206b14"),
            decode_hex("048f3897c16bdc59ec1187aa080a4b4aa5ec1afcb4b776cf8b8a214b01990a7b"),
        ];

        let calculated_merkle_root = merkle_root_calculator(&tx_hashes);
        let calculated_merkle_proof = merkle_proof_calculator(tx_hashes, 4);

        let computed_root_from_merkle_proof = compute_root_from_merkle_proof(
            decode_hex("048f3897c16bdc59ec1187aa080a4b4aa5ec1afcb4b776cf8b8a214b01990a7b"),
            4,
            &calculated_merkle_proof,
        );
        assert_eq!(computed_root_from_merkle_proof, calculated_merkle_root);
    }
}
