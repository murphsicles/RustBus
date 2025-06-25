#[cfg(test)]
mod tests {
    use sv::messages::Tx;
    use sv::messages::TxOut;
    use sv::script::Script;
    #[cfg(test)]
    use rustbus::blockchain::classifier::TransactionClassifier;

    #[test]
    fn test_classify_run() {
        let classifier = TransactionClassifier::new();
        let mut tx = Tx::default();
        let data = b"run://test";
        let mut script_bytes = vec![0x6a, data.len() as u8]; // OP_RETURN, length
        script_bytes.extend_from_slice(data); // Append data
        let script = Script(script_bytes);
        tx.outputs.push(TxOut { satoshis: 0, lock_script: script });
        assert_eq!(classifier.classify(&tx), "RUN");
    }
}
