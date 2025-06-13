#[cfg(test)]
mod tests {
    use sv::messages::tx::Transaction;
    use sv::messages::tx_out::TxOut;
    use sv::script::Script;
    use crate::classifier::TransactionClassifier;
    #[test]
    fn test_classify_run() {
        let classifier = TransactionClassifier::new();
        let mut tx = Transaction::new(); // Adjust constructor if needed
        let script = Script::new_op_return(b"run://test"); // Adjust API
        tx.outputs.push(TxOut { value: 0, script }); // Adjust field names
        assert_eq!(classifier.classify(&tx), "RUN");
    }
}
