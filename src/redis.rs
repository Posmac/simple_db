use crate::hashtable::{HMap, HNode};

pub struct GlobalData {
    pub db: HMap,
}

pub struct Entry {
    key: Vec<String>,
    val: Vec<String>,
    //intrusive approach
    node: HNode,
}
