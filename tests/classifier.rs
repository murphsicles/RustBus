#[cfg(test)]
mod tests {
    use sv::messages::Tx;
    use sv::messages::TxOut;
    use sv::script::Script;
    use sv::script::op_codes::OP_RETURN;
    #[cfg(test)]
    use rustbus::blockchain::classifier::TransactionClassifier;

    #[test]
    fn test_classify_run() {
        let classifier = TransactionClassifier::new();
        let mut tx = Tx::default();
        let data = b"run://test";
        let mut script = Script::new();
        script.append(OP_RETURN); // 0x6a
        script.append_data(data); // Pushdata opcodes + data
        tx.outputs.push(TxOut { satoshis: 0, lock_script: script });
        assert_eq!(classifier.classify(&tx), "RUN");
    }
}
