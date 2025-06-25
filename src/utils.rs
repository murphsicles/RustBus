use sv::messages::Tx;

// Extracts OP_RETURN data from a transaction, if present
pub fn extract_op_return(tx: &Tx) -> Option<String> {
    // Iterate through transaction outputs
    for out in &tx.outputs {
        // Check if the output’s script starts with OP_RETURN (0x6a)
        if !out.lock_script.0.is_empty() && out.lock_script.0[0] == 0x6a {
            // Return the hex-encoded script (including 0x6a and data)
            return Some(hex::encode(&out.lock_script.0));
        }
    }
    // Return None if no OP_RETURN output is found
    None
}

// Extension trait to add hex encoding to transactions
pub trait TxExt {
    // Converts a transaction to its hex representation
    fn to_hex(&self) -> String;
}

impl TxExt for Tx {
    // Encodes the transaction to hex using serialization
    fn to_hex(&self) -> String {
        let mut bytes = Vec::new();
        self.write(&mut bytes).unwrap(); // Serialize transaction
        hex::encode(bytes) // Return hex string
    }
}
