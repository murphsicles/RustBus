#[cfg(test)]
mod tests {
    use rust_sv::{Transaction, Script, TxOut}; // Adjust based on actual API
    use crate::TransactionClassifier;

    #[test]
    fn test_classify_run() {
        let classifier = TransactionClassifier::new();
        let mut tx = Transaction::new(); // Adjust constructor if needed
        let script = Script::new_op_return(b"run://test"); // Adjust API
        tx.outputs.push(TxOut { value: 0, script }); // Adjust field names
        assert_eq!(classifier.classify(&tx), "RUN");
    }
}
