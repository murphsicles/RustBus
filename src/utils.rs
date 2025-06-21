use sv::messages::Tx;
use sv::util::Serializable;

pub fn extract_op_return(tx: &Tx) -> Option<String> {
    tx.outputs.iter()
        .find(|out| out.lock_script.is_op_return())
        .and_then(|out| Some(hex::encode(&out.lock_script.data)))
}

pub trait TxExt {
    fn to_hex(&self) -> String;
}

impl TxExt for Tx {
    fn to_hex(&self) -> String {
        let mut bytes = Vec::new();
        self.write(&mut bytes).unwrap();
        hex::encode(bytes)
    }
}
