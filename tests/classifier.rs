#[cfg(test)]
mod tests {
    use sv::messages::Tx;
    use sv::messages::TxOut;
    use sv::script::Script;
    use crate::blockchain::classifier::TransactionClassifier;

    #[test]
    fn test_classify_run() {
        let classifier = TransactionClassifier::new();
        let mut tx = Tx::new();
        let script = Script::new_op_return(&b"run://test".to_vec());
        tx.outputs.push(TxOut { value: 0, lock_script: script });
        assert_eq!(classifier.classify(&tx), "RUN");
    }
}
