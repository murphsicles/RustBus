use regex::Regex;
use std::env;
use sv::messages::Tx;
use crate::utils::extract_op_return;
use hex;

pub struct TransactionClassifier {
    protocols: Vec<(String, Regex)>,
}

impl TransactionClassifier {
    pub fn new() -> Self {
        let protocols_str = env::var("PROTOCOLS").unwrap_or_default();
        let protocols = protocols_str
            .split(';')
            .filter_map(|p| {
                let parts: Vec<&str> = p.split(':').collect();
                if parts.len() == 2 {
                    Regex::new(parts[1]).ok().map(|re| (parts[0].to_string(), re))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let defaults = vec![
            ("RUN".to_string(), Regex::new(r"run://").unwrap()),
            ("MAP".to_string(), Regex::new(r"1PuQa7").unwrap()),
            ("B".to_string(), Regex::new(r"19HxigV4QyBv3tHpQVcUEQyq1pzZVdoAut").unwrap()),
            ("BCAT".to_string(), Regex::new(r"15PciHG22SNLQJXMoSUaWVi7WSqc7hCfva").unwrap()),
            ("AIP".to_string(), Regex::new(r"1J7Gm3UGv5R3vRjAf9nV7oJ3yF3nD4r93r").unwrap()),
            ("METANET".to_string(), Regex::new(r"1Meta").unwrap()),
        ];
        TransactionClassifier {
            protocols: if protocols.is_empty() { defaults } else { protocols },
        }
    }

    pub fn classify(&self, tx: &Tx) -> String {
        if let Some(op_return) = extract_op_return(tx) {
            // Decode the hex string to get the actual data
            if let Ok(decoded) = hex::decode(&op_return) {
                let decoded_str = String::from_utf8_lossy(&decoded);
                for (protocol, regex) in &self.protocols {
                    if regex.is_match(&decoded_str) {
                        return protocol.clone();
                    }
                }
            }
        }
        "STANDARD".to_string()
    }
}
