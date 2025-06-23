use sv::script::Opcode;
use sv::messages::Tx;
use sv::util::Serializable;

pub fn extract_op_return(tx: &Tx) -> Option<String> {
    tx.outputs.iter()
        .find(|out| {
            let script = &out.lock_script;
            script.len() > 0 && script.as_bytes()[0] == Opcode::OP_RETURN as u8
        })
        .map(|out| hex::encode(&out.lock_script.as_bytes()))
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
