use std::ptr::NonNull;

pub type LookUpFunction = fn(&Bucket, &Bucket) -> bool;

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

type NodePtr = NonNull<HNode>;
type Bucket = Option<NodePtr>;

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

#[derive(Debug, Copy, Clone)]
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
            libc::free(self.buckets.as_ptr() as *mut libc::c_void);
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

        let buckets_vec_ptr = void_ptr as *mut Bucket; // При создании таблицы:

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

    use std::{
        alloc::{Layout, alloc, dealloc},
        ptr::{self, NonNull, null_mut},
    };

    use crate::hashtable::{
        DEFAULT_HASH_SIZE, Entry, HMap, HNode, HTable, NodePtr, entry_eq, str_hash,
    };

    pub fn create_node(key: &str, value: &str, hash: usize) -> NodePtr {
        let layout = Layout::new::<Entry>();
        unsafe {
            let raw = alloc(layout) as *mut Entry;

            if raw.is_null() {
                panic!("Memory allocation failed");
            }

            ptr::write(ptr::addr_of_mut!((*raw).key), key.to_string());
            ptr::write(ptr::addr_of_mut!((*raw).val), value.to_string());
            ptr::write(
                ptr::addr_of_mut!((*raw).node),
                HNode {
                    next: None,
                    hcode: hash,
                },
            );

            let node_ptr = std::ptr::addr_of_mut!((*raw).node);
            NonNull::new_unchecked(node_ptr)
        }
    }

    //double pointer ()
    pub fn get_entry_from_dp(ptr: *const Option<NodePtr>) -> *const Entry {
        unsafe {
            if let Some(node_ptr) = *ptr {
                let fentry = container_of!(node_ptr.as_ptr(), Entry, node);
                // println!("Найдено: fe={:#?}, e={:#?}", *fentry, *entry);
                return fentry;
            }
        }

        null_mut()
    }

    pub fn free_entry(node_ptr: NodePtr) {
        unsafe {
            let entry_ptr = container_of!(node_ptr.as_ptr(), Entry, node) as *mut Entry;
            ptr::drop_in_place(entry_ptr);
            let layout = Layout::new::<Entry>();
            dealloc(entry_ptr as *mut u8, layout);
        }
    }

    #[test]
    pub fn test_basic_insert_and_lookup_and_delete() {}

    /// Test if the map initializes with correct sizes and null pointers.
    #[test]
    fn test_initialization() {
        // TODO: Create a new HMap and assert that size is 0 and buckets are null-initialized.
        let default = HMap::new();
        assert!(
            default.current.size == 0 && default.current.mask == default.current.size - 1,
            "New current hashtable size != 0"
        );
        assert!(
            default.older.size == 0 && default.older.mask == default.current.size - 1,
            "New older hashtable size != 0"
        );
    }

    /// Test if the map initialization with capacity which is not power of two
    #[test]
    #[should_panic(expected = "capacity must be power of two and non zero")]
    fn test_panic_capacity_wrong() {
        // TODO: Create a zero cap table and table with the size of now power of 2
        let zero_table = HMap::with_capacity(0);
        let not_power_of_two_table = HMap::with_capacity(3);
    }

    /// Test basic insertion and retrieval of a single node.
    #[test]
    fn test_single_insert_and_lookup() {
        // TODO: Create one node, insert it, and verify lookup returns the correct pointer.
        // Remember to free memory at the end.
        let mut default = HMap::new();

        let data = vec![112u8; 1];
        let hash = str_hash(&data, data.len());

        let node = create_node("key_1", "val_1", hash);
        default.insert(node);

        unsafe {
            //lookup
            let entry = container_of!(node.as_ptr(), Entry, node);
            let lookup = default.lookup(Some(node), entry_eq);
            if !lookup.is_null() {
                let fentry = get_entry_from_dp(lookup);
                println!("LOOKUP: FE: {:#?}, E: {:#?}", *entry, *fentry);
            }

            //delete
            let node_to_free = default.delete(Some(node), entry_eq);
            if let Some(ptr) = node_to_free {
                let entry_to_free = &*container_of!(ptr.as_ptr(), Entry, node);
                println!("DELETED: {:#?}", entry_to_free);
                free_entry(ptr);
            } else {
                println!("Нода не найдена, удалять нечего");
            }
        }
    }

    /// Test handling of hash collisions (multiple nodes in the same bucket).
    #[test]
    fn test_collision_chains() {
        // TODO:
        // 1. Create 3 nodes with the SAME hcode (to force them into one bucket).
        // 2. Insert all three.
        // 3. Verify that lookup can find the "tail" (the first inserted node)
        //    by traversing the 'next' pointers.
        // 4. Verify size count is 3.
    }

    /// Test deleting a node that is at the head of a collision chain.
    #[test]
    fn test_delete_chain_head() {
        // TODO:
        // 1. Insert 2 nodes into the same bucket.
        // 2. Delete the last inserted node (the current head).
        // 3. Verify that the bucket now points to the remaining node.
    }

    /// Test deleting a node from the middle or end of a collision chain.
    #[test]
    fn test_delete_chain_middle_and_tail() {
        // TODO:
        // 1. Insert 3 nodes into the same bucket (A -> B -> C).
        // 2. Delete the middle node (B).
        // 3. Verify that A's 'next' now points to C.
        // 4. Delete the tail node (C).
        // 5. Verify that A's 'next' is now None.
    }

    /// Test lookup behavior when the key does not exist in the map.
    #[test]
    fn test_lookup_missing_key() {
        // TODO:
        // 1. Insert some nodes.
        // 2. Perform a lookup with a key/hcode that was never inserted.
        // 3. Assert that lookup returns null_mut().
    }

    /// Test re-inserting the same node pointer (logic check).
    #[test]
    fn test_duplicate_node_pointer() {
        // TODO:
        // 1. Insert a node.
        // 2. Insert the SAME node pointer again.
        // 3. Decide if your implementation handles this (usually size increases,
        //    but it might create a circular reference if not careful).
    }

    /// Stress test for memory leaks and pointer stability.
    #[test]
    fn test_stress_insert_delete() {
        // TODO:
        // 1. In a loop (e.g., 1000 iterations), insert nodes with different keys.
        // 2. In another loop, delete them all.
        // 3. Verify size returns to 0.
        // Run this test with 'cargo miri test' to detect leaks.
    }

    /// Test if lookup correctly checks both 'current' and 'older' tables during resizing.
    #[test]
    fn test_lookup_during_resizing() {
        // TODO:
        // 1. Manually move a node to the 'older' table.
        // 2. Perform a lookup via HMap.
        // 3. Verify that HMap finds it even if it's not in the 'current' table.
    }
}
