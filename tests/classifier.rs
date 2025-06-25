#[cfg(test)]
mod tests {
    use sv::messages::Tx;
    use sv::messages::TxOut;
    use sv::script::Script;
    use sv::script::op_codes::OP_RETURN;
    #[cfg(test)]
    use rustbus::blockchain::classifier::TransactionClassifier;

    // Tests the TransactionClassifier's ability to identify a transaction with an OP_RETURN script
    #[test]
    fn test_classify_run() {
        // Initialize the classifier, which loads protocol regex patterns (e.g., "run://")
        let classifier = TransactionClassifier::new();

        // Create a new transaction with default values (version, inputs, outputs, lock_time)
        let mut tx = Tx::default();

        // Define OP_RETURN data to test the "RUN" protocol
        let data = b"run://test";

        // Construct an OP_RETURN script: [0x6a, data...]
        let mut script = Script::new(); // Create an empty script (Vec<u8>)
        script.append(OP_RETURN); // Append OP_RETURN opcode (0x6a)
        script.append_slice(data); // Append raw data (run://test)

        // Add an output with the OP_RETURN script and 0 satoshis
        tx.outputs.push(TxOut {
            satoshis: 0,
            lock_script: script,
        });

        // Verify the classifier identifies the transaction as "RUN" based on the OP_RETURN data
        assert_eq!(classifier.classify(&tx), "RUN");
    }
}
