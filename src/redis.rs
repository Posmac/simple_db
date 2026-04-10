use std::sync::{LazyLock, Mutex};

use crate::{
    container_of_mut,
    hashtable::{HMap, HNode, NodePtr, create_node, entry_eq, free_entry},
    tvl::{ResponseCode, out_nil, out_str},
};

pub static GLOBAL_TABLE: LazyLock<Mutex<HMap>> = LazyLock::new(|| Mutex::new(HMap::new()));

pub struct Entry {
    pub key: String,
    pub val: String,
    //intrusive approach
    pub node: HNode,
}

pub fn do_get(cmd: &[String], out: &mut Vec<u8>) {
    let key = &cmd[1];

    let v = String::default();
    let entry: NodePtr = create_node(key, v.as_str());

    unsafe {
        let mut table = GLOBAL_TABLE.lock().unwrap();
        let lookup = table.lookup(Some(entry), entry_eq);

        if !lookup.is_null() {
            if let Some(ptr) = *lookup {
                let container = container_of_mut!(ptr.as_ptr(), Entry, node);
                out_str(
                    out,
                    &(*container).val.to_string(),
                    ResponseCode::GetExisting,
                );
            }
        } else {
            out_nil(out, ResponseCode::GetNothing);
        }
    };

    free_entry(entry);
}

pub fn do_set(cmd: &[String], out: &mut Vec<u8>) {
    let key = &cmd[1];

    let v = String::default();
    let entry: NodePtr = create_node(key, v.as_str());

    unsafe {
        let mut table = GLOBAL_TABLE.lock().unwrap();
        let lookup = table.lookup(Some(entry), entry_eq);

        if !lookup.is_null() {
            if let Some(ptr) = *lookup {
                let container = container_of_mut!(ptr.as_ptr(), Entry, node);
                (*container).val = cmd[2].to_string();
                out_nil(out, ResponseCode::SetExisting);
            }
        } else {
            let new_entry = create_node(key, &cmd[2]);
            table.insert(new_entry);
            out_nil(out, ResponseCode::SetNew);
        }
    };

    free_entry(entry);
}

pub fn do_del(cmd: &[String], out: &mut Vec<u8>) {
    let key = &cmd[1];

    let v = String::default();
    let entry: NodePtr = create_node(key, v.as_str());

    let mut table = GLOBAL_TABLE.lock().unwrap();
    let node = table.delete(Some(entry), entry_eq);

    if let Some(ptr) = node {
        out_nil(out, ResponseCode::DelOk);
        free_entry(ptr);
    } else {
        out_nil(out, ResponseCode::DelNothing);
    }

    free_entry(entry);

    // resp
}
