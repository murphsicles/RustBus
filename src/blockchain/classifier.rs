use regex::Regex;
use std::env;
use sv::messages::Tx;
use crate::utils::extract_op_return;
use hex;

// Defines a classifier for identifying transaction protocols based on OP_RETURN data
pub struct TransactionClassifier {
    // Stores protocol names and their corresponding regex patterns
    protocols: Vec<(String, Regex)>,
}

impl TransactionClassifier {
    // Initializes a new classifier with protocol patterns from environment or defaults
    pub fn new() -> Self {
        // Load protocol patterns from the PROTOCOLS environment variable (format: name:pattern;name:pattern)
        let protocols_str = env::var("PROTOCOLS").unwrap_or_default();
        let protocols = protocols_str
            .split(';')
            .filter_map(|p| {
                let parts: Vec<&str> = p.split(':').collect();
                if parts.len() == 2 {
                    // Parse each protocol as a name and regex pattern
                    Regex::new(parts[1]).ok().map(|re| (parts[0].to_string(), re))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // Define default protocols if none are specified in the environment
        let defaults = vec![
            ("RUN".to_string(), Regex::new(r"run://").unwrap()),
            ("MAP".to_string(), Regex::new(r"1PuQa7").unwrap()),
            ("B".to_string(), Regex::new(r"19HxigV4QyBv3tHpQVcUEQyq1pzZVdoAut").unwrap()),
            ("BCAT".to_string(), Regex::new(r"15PciHG22SNLQJXMoSUaWVi7WSqc7hCfva").unwrap()),
            ("AIP".to_string(), Regex::new(r"1J7Gm3UGv5R3vRjAf9nV7oJ3yF3nD4r93r").unwrap()),
            ("METANET".to_string(), Regex::new(r"1Meta").unwrap()),
            ("D".to_string(), Regex::new(r"19iG3WTYSsbyos3uJ733yK4zEioi1FesNU").unwrap()),
            ("TOKENIZED".to_string(), Regex::new(r"TKN").unwrap()),
        ];

        TransactionClassifier {
            // Use environment protocols if available, otherwise use defaults
            protocols: if protocols.is_empty() { defaults } else { protocols },
        }
    }

    // Classifies a transaction based on its OP_RETURN data
    pub fn classify(&self, tx: &Tx) -> String {
        // Extract OP_RETURN data from the transaction, if present
        if let Some(op_return) = extract_op_return(tx) {
            // Decode the hex-encoded script to its raw bytes
            if let Ok(decoded) = hex::decode(&op_return) {
                // Convert the decoded bytes to a string (lossy UTF-8 conversion)
                let decoded_str = String::from_utf8_lossy(&decoded);
                // Match the decoded string against protocol regex patterns
                for (protocol, regex) in &self.protocols {
                    if regex.is_match(&decoded_str) {
                        return protocol.clone();
                    }
                }
            }
        }
        // Return "STANDARD" if no protocol matches or no OP_RETURN data is found
        "STANDARD".to_string()
    }
}
