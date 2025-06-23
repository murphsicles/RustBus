use sv::messages::Tx;
use sv::util::Serializable;

pub fn extract_op_return(tx: &Tx) -> Option<String> {
    tx.outputs.iter()
        .find(|out| {
            let script = &out.lock_script;
            script.0.len() > 0 && script.0[0] == 0x6a // OP_RETURN opcode
        })
        .map(|out| hex::encode(&out.lock_script.0))
}

pub trait TxExt {
    fn to_hex(&self) -> String;
}

impl TxExt for Tx {
    fn to_hex(&self) -> String {
        let mut bytes = Vec::new();
        self.write(&mut bytes).unwrap();
        hex::encode(&bytes)
    }
}
