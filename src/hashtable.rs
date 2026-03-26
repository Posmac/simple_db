use std::ptr::NonNull;

pub type LookUpFunction = fn(&Bucket, &Bucket) -> bool;

type NodePtr = NonNull<HNode>;
type Bucket = Option<NodePtr>;

const MAX_LOAD_FACTOR: usize = 8;
const REHASHING_WORK_SIZE: usize = 128;
const DEFAULT_HASH_SIZE: usize = 2;

#[macro_export]
macro_rules! container_of {
    ($ptr:expr, $type:path, $member:ident) => {{
        //try to find an offset from the start of our $type for our $member
        let offset = std::mem::offset_of!($type, $member);
        //cast $ptr to u8(!)
        let pointer = $ptr as *const u8;
        //return the start of out data $type
        pointer.sub(offset) as *const $type
    }};
}

#[macro_export]
macro_rules! container_of_mut {
    ($ptr:expr, $type:path, $member:ident) => {{
        //try to find an offset from the start of our $type for our $member
        let offset = std::mem::offset_of!($type, $member);
        //cast $ptr to u8(!)
        let pointer = $ptr as *mut u8;
        //return the start of out data $type
        pointer.sub(offset) as *mut $type
    }};
}

#[derive(Debug)]
#[repr(C)]
pub struct Entry {
    pub key: String,
    pub val: String,
    pub node: HNode,
}

//only this one is public
#[derive(Debug)]
#[repr(C)]
pub struct HMap {
    current: HTable,
    older: HTable,
    migrate_pos: usize,
}

#[derive(Debug)]
#[repr(C)]
pub struct HNode {
    pub next: Bucket, //Option<NonNull<HNode>>;
    pub hcode: usize,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct HTable {
    buckets: NonNull<Bucket>, //NonNull<Option<NonNull<HNode>>>;
    mask: usize,
    size: usize,
}

unsafe impl Send for HMap {}
unsafe impl Sync for HMap {}

//FNV Hash
pub fn str_hash(data: &[u8], len: usize) -> usize {
    let mut h = 0x811C9DC5;
    for i in 0..len {
        h = (h + data[i] as usize) * 0x01000193;
    }

    h
}

pub fn entry_eq(lhs: &Bucket, rhs: &Bucket) -> bool {
    unsafe {
        let lhs = container_of!(lhs.as_ref().unwrap().as_ptr(), Entry, node);
        let rhs = container_of!(rhs.as_ref().unwrap().as_ptr(), Entry, node);
        return (*lhs).key == (*rhs).key;
    }
}

impl HMap {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_HASH_SIZE)
    }

    pub fn with_capacity(size: usize) -> Self {
        assert!(
            size > 0 && (size & (size - 1)) == 0,
            "capacity must be power of two and non zero"
        );
        Self {
            current: HTable::with_capacity(size),
            older: HTable::with_capacity(DEFAULT_HASH_SIZE),
            migrate_pos: 0,
        }
    }

    pub fn lookup(&mut self, key: Bucket, f: LookUpFunction) -> *mut Bucket {
        self.help_rehashing();
        let mut from = self.current.lookup(key, f);
        if from.is_null() {
            from = self.older.lookup(key, f);
        }

        match from.is_null() {
            true => std::ptr::null_mut::<Bucket>(),
            false => from,
        }
    }

    pub fn insert(&mut self, node: NodePtr) {
        self.current.insert(node);
        let rehash_trashold = self.current.mask * MAX_LOAD_FACTOR;
        if self.current.size >= rehash_trashold {
            self.trigger_rehashing();
        }
        self.help_rehashing();
    }

    pub fn delete(&mut self, key: Bucket, f: LookUpFunction) -> Bucket {
        self.help_rehashing();
        let from = self.current.lookup(key, f);
        if !from.is_null() {
            return self.current.detach(from);
        }

        let from_old = self.older.lookup(key, f);
        if !from_old.is_null() {
            return self.older.detach(from_old);
        }
        None
    }

    fn trigger_rehashing(&mut self) {
        self.older = self.current.clone();
        let new = HTable::with_capacity((self.current.mask + 1) * 2);
        self.current = new;
        self.migrate_pos = 0;
    }

    fn help_rehashing(&mut self) {
        let mut nwork: usize = 0;

        while nwork < REHASHING_WORK_SIZE && self.older.size > 0 {
            unsafe {
                let from = self.older.buckets.as_ptr().add(self.migrate_pos);

                if from.is_null() {
                    self.migrate_pos += 1;
                    continue;
                }
                let detached = self.older.detach(from);
                self.current.insert(detached.unwrap());
                nwork += 1;
            }
        }

        if self.older.size == 0 {
            self.older = HTable::new();
        }
    }
}

impl Drop for HTable {
    fn drop(&mut self) {
        unsafe {
            if !self.buckets.as_ptr().is_null() {
                libc::free(self.buckets.as_ptr() as *mut libc::c_void);
            }
        }
    }
}

impl HTable {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_HASH_SIZE)
    }

    pub fn with_capacity(size: usize) -> Self {
        assert!(
            size > 0 && (size & (size - 1)) == 0,
            "capacity must be power of two and non zero"
        );

        //INFO: Unsafe
        let void_ptr = unsafe { libc::calloc(size, std::mem::size_of::<Bucket>()) };
        assert!(!void_ptr.is_null(), "Allocatin failed");

        let buckets_vec_ptr = void_ptr as *mut Bucket;

        Self {
            //INFO: Unsafe
            buckets: unsafe { NonNull::new_unchecked(buckets_vec_ptr) },
            mask: size - 1,
            size: 0,
        }
    }

    pub fn insert(&mut self, mut node: NodePtr) {
        //INFO: Unsafe
        unsafe {
            let node_ptr_ref = node.as_mut();
            let pos = node_ptr_ref.hcode & self.mask;

            let bucket_tail = self.buckets.as_ptr().add(pos);
            node_ptr_ref.next = *bucket_tail;

            *bucket_tail = Some(node);
            self.size += 1;
        }
    }

    pub fn lookup(&self, key: Bucket, f: LookUpFunction) -> *mut Bucket {
        if self.size == 0 {
            return std::ptr::null_mut::<Bucket>();
        }

        if key.is_none() {
            return std::ptr::null_mut::<Bucket>();
        }

        unsafe {
            let hcode = key.unwrap().as_ref().hcode;
            let pos = hcode & self.mask;
            let mut start = self.buckets.as_ptr().add(pos); //start of the bucket

            //get NonNull<HNode> from start if exists, else return null
            while let Some(mut cur_ptr) = *start {
                let cur = cur_ptr.as_mut(); //get mutable reference to &mut HNode

                //if found, return pointer to HNode from prev node (return address of its parent node)
                if cur.hcode == hcode && f(&Some(cur_ptr), &key) {
                    return start;
                }

                //next
                start = &mut cur.next as *mut Bucket;
            }
        }

        return std::ptr::null_mut::<Bucket>();
    }

    //Buckets Array:
    // [ pos ] ---> [ Узел A ]
    //                | next | ---> [ Узел B ]
    //                                | next | ---> [ Узел C ]
    //                                                | next | ---> None
    //     // LOOKUP ВЕРНУЛ ЭТОТ АДРЕС
    //                 |
    //                 v
    // [ Узел A ]    [адрес]
    //   | hcode |   |      |
    //   | next  | = [ 0xBB ]  <--- (это указатель на Узел B)
    //   |_______|
    //
    //
    // [ Узел A ]
    //   | hcode |
    //   | next  | = [ 0xCC ]  -----.
    //   |_______|                  |
    //                              |  (Узел B пропущен!)
    // [ Узел B ] <--- (извлечен)    |
    //   | next  | = [ 0xCC ]       |
    //   |_______|                  |
    //               v              v
    //           [ Узел C ] <-------'
    //             | next | ---> None
    //
    pub fn detach(&mut self, from: *mut Bucket) -> Bucket {
        unsafe {
            let node_to_remove = (*from).unwrap();
            *from = node_to_remove.as_ref().next;
            self.size -= 1;
            Some(node_to_remove)
        }
    }
}

//INFO: use cargo test -- --nocapture
pub mod tests {

    //     use std::{
    //         alloc::{Layout, alloc},
    //         ptr::NonNull,
    //     };

    //     use crate::hashtable::{DEFAULT_HASH_SIZE, HMap, HNode, HTable, str_hash};

    //     #[test]
    //     #[should_panic(expected = "capacity must be power of two and non zero")]
    //     fn test_table_zero_capacity() {
    //         let zero_table = HMap::with_capacity(0);
    //         let not_power_of_two_table = HMap::with_capacity(3);
    //     }

    //     #[test]
    //     pub fn test_creation() {
    //         let default = HMap::new();

    //         println!("Info: {:#?}", &default);

    //         assert!(
    //             default.current.size == 0 && default.current.mask == DEFAULT_HASH_SIZE - 1,
    //             "New current hashtable size != 0"
    //         );
    //         assert!(
    //             default.older.size == 0 && default.older.mask == DEFAULT_HASH_SIZE - 1,
    //             "New older hashtable size != 0"
    //         );
    //     }

    //     #[test]
    //     pub fn test_basic() {
    //         let mut default = HMap::new();
    //         // println!("Inf: {:#?}", &default);

    //         let data = vec![112u8; 1];
    //         let hash = str_hash(&data, data.len());

    //         unsafe {
    //             let layout = Layout::new::<HNode>();
    //             let raw = alloc(layout) as *mut HNode;

    //             if raw.is_null() {
    //                 panic!("Memory allocation failed");
    //             }

    //             let new_node = HNode {
    //                 next: None,
    //                 hcode: hash,
    //             };

    //             std::ptr::write(raw, new_node);

    //             default.insert(NonNull::new_unchecked(raw));
    //         }

    //         // let node = default.lookup(key, f);
    //         // println!("Inf after: {:#?}", &default);
    //     }

    //     #[test]
    //     pub fn test_rehash() {}
}
