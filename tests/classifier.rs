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
        let script = Script::new_op_return(b"run://test".to_vec());
        tx.outputs.push(TxOut { satoshis: 0, lock_script: script });
        assert_eq!(classifier.classify(&tx), "RUN");
    }
}
