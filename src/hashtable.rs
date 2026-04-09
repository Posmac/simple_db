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
pub fn str_hash(data: &[u8]) -> usize {
    let mut h: usize = 0x811C9DC5;
    for &byte in data {
        h = h.wrapping_add(byte as usize).wrapping_mul(0x01000193);
    }
    h
}

pub fn entry_eq(lhs: &Bucket, rhs: &Bucket) -> bool {
    unsafe {
        let lhs = container_of!(lhs.as_ref().unwrap().as_ptr(), Entry, node);
        let rhs = container_of!(rhs.as_ref().unwrap().as_ptr(), Entry, node);
        return (*lhs).key.eq(&(*rhs).key);
    }
}

pub fn entry_eq_kv(lhs: &Bucket, rhs: &Bucket) -> bool {
    unsafe {
        let lhs = container_of!(lhs.as_ref().unwrap().as_ptr(), Entry, node);
        let rhs = container_of!(rhs.as_ref().unwrap().as_ptr(), Entry, node);
        return (*lhs).key.eq(&(*rhs).key) && (*lhs).val.eq(&(*rhs).val);
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
        Bucket, DEFAULT_HASH_SIZE, Entry, HMap, HNode, HTable, NodePtr, entry_eq, entry_eq_kv,
        str_hash,
    };

    pub fn create_node(key: &str, value: &str) -> NodePtr {
        let hash = str_hash(key.as_bytes());
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

    fn clear_nodes_by_key(map: &mut HMap, template_node: Bucket, initial_size: usize) {
        println!("LOG: Starting chain cleanup {}...", map.current.size);
        let mut deleted_count = 0;

        unsafe {
            while let Some(ptr) = map.delete(template_node, entry_eq) {
                let entry = container_of!(ptr.as_ptr(), Entry, node);
                println!("LOG: Freeing node with value: '{}'", (*entry).val);
                free_entry(ptr);
                deleted_count += 1;
            }
        }

        assert_eq!(
            deleted_count, initial_size,
            "FAILED: Expected to delete {} nodes, but deleted {}",
            initial_size, deleted_count
        );
        assert_eq!(map.current.size, 0, "Table should be empty after cleanup");
    }

    /// Test if the map initializes with correct sizes and null pointers.
    #[test]
    fn test_initialization() {
        // TODO: Create a new HMap and assert that size is 0 and buckets are null-initialized.
        let default = HMap::new();
        assert!(default.current.size == 0, "New current hashtable size != 0");
        assert!(default.older.size == 0, "New older hashtable size != 0");
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
        let mut map = HMap::new();

        // 1. Preparation
        let key = "key_1";
        let val = "val_1";
        let node = create_node(key, val);

        // 2. Execution
        map.insert(node);
        println!("LOG: Inserted node with key='{}', val='{}'", key, val);

        unsafe {
            // 3. Lookup Verification
            let lookup = map.lookup(Some(node), entry_eq);

            assert!(
                !lookup.is_null(),
                "FAILED: Node with key '{}' should be found in the map",
                key
            );

            let fentry = get_entry_from_dp(lookup);
            println!("LOG: Lookup successful. Found Entry: {:?}", *fentry);

            assert_eq!(
                (*fentry).key,
                key,
                "FAILED: Key mismatch! Expected '{}', found '{}'",
                key,
                (*fentry).key
            );
            assert_eq!(
                (*fentry).val,
                val,
                "FAILED: Value mismatch! Expected '{}', found '{}'",
                val,
                (*fentry).val
            );

            // 4. Delete & Cleanup Verification
            let node_to_free = map.delete(Some(node), entry_eq);

            assert!(
                node_to_free.is_some(),
                "FAILED: Delete returned None, but node with key '{}' was expected",
                key
            );

            if let Some(ptr) = node_to_free {
                let entry_to_free = container_of!(ptr.as_ptr(), Entry, node);
                println!(
                    "LOG: Cleanup. Freeing entry with key: '{}'",
                    (*entry_to_free).key
                );

                // Physical memory cleanup
                free_entry(ptr);
            }

            // 5. Post-delete Verification
            let final_lookup = map.lookup(Some(node), entry_eq);
            assert!(
                final_lookup.is_null(),
                "FAILED: Node with key '{}' still exists after deletion",
                key
            );

            println!("SUCCESS: Single insert/lookup/delete cycle completed perfectly.");
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
        // 4. Verify size count is 3

        // 1. Create 3 nodes with the same keys
        let mut map = HMap::with_capacity(4);
        let common_key = "node_key_common";
        let node_1 = create_node(common_key, "value_1");
        let node_2 = create_node(common_key, "value_2");
        let node_3 = create_node(common_key, "value_3");

        // 2. Inserting (LIFO order in our bucket: 3 -> 2 -> 1)
        map.insert(node_1);
        map.insert(node_2);
        map.insert(node_3);

        println!(
            "LOG: Inserted 3 nodes with identical key '{}' into the same bucket.",
            common_key
        );
        assert_eq!(map.current.size, 3, "Table size should be exactly 3");

        unsafe {
            // 3. Try to find last inserted node (node_3)
            // lookup "common_key" should return node_3
            let lookup_ptr = map.lookup(Some(node_3), entry_eq);

            assert!(
                !lookup_ptr.is_null(),
                "FAILED: Could not find any node with key '{}'",
                common_key
            );

            let fentry = get_entry_from_dp(lookup_ptr);
            println!(
                "LOG: Found entry value: '{}' (Expected 'value_3' because it's the chain head)",
                (*fentry).val
            );

            assert_eq!(
                (*fentry).val,
                "value_3",
                "FAILED: Lookup should return the most recently inserted node (the head of the chain)"
            );

            // 4. Clear after yourself, please!
            clear_nodes_by_key(&mut map, Some(node_1), 3);

            println!("SUCCESS: Collision chain handled and cleaned up correctly.");
        }
    }

    /// Test deleting a node that is at the head of a collision chain.
    #[test]
    fn test_delete_chain_head() {
        // TODO:
        // 1. Insert 3 nodes into the same bucket.
        // 2. Delete the last inserted node (the current head).
        // 3. Verify that the bucket now points to the remaining node.

        // 1. Create 3 nodes
        let mut map = HMap::with_capacity(4);
        let key_a = "key_A";
        let key_b = "key_A";
        let key_c = "key_A";
        let node_1 = create_node(key_a, "value_1");
        let node_2 = create_node(key_b, "value_2");
        let node_3 = create_node(key_c, "value_3");

        // 2. Inserting (LIFO order in our bucket: 3 -> 2 -> 1)
        map.insert(node_1);
        map.insert(node_2);
        map.insert(node_3);

        println!(
            "LOG: Inserted 3 nodes with the next keys: {}, {}, {}",
            key_a, key_b, key_c
        );

        assert_eq!(map.current.size, 3, "Table size should be exactly 3");

        //3. Delete last inserted node(head)
        println!("LOG: Removing head: {}", key_c);
        let deleted = map.delete(Some(node_3), entry_eq);
        if let Some(d) = deleted {
            unsafe {
                let entry = container_of!(d.as_ptr(), Entry, node);
                println!(
                    "LOG: Freeing node with key:value: '{}':'{}'",
                    (*entry).key,
                    (*entry).val
                );
                free_entry(d);
            }
        };

        assert_eq!(map.current.size, 2, "Table size should be exactly 2");

        // 4. Clear after yourself, please!
        clear_nodes_by_key(&mut map, Some(node_1), 2);

        println!("SUCCESS: Delete chain head test succsesfull");
    }

    /// Test deleting a node from the middle or end of a collision chain.
    #[test]
    fn test_delete_chain_middle_and_tail() {
        // TODO:
        // 1. Insert 3 nodes into the same bucket (C -> B -> A).
        // 2. Delete the middle node (B).
        // 3. Verify that C's 'next' now points to A.
        // 4. Delete the tail node (A).
        // 5. Verify that C's 'next' is now None.

        // 1. Create 3 nodes
        let mut map = HMap::with_capacity(4);
        let key_a = "key_A";
        let key_b = "key_A";
        let key_c = "key_A";
        let node_1 = create_node(key_a, "value_1");
        let node_2 = create_node(key_b, "value_2");
        let node_3 = create_node(key_c, "value_3");

        // 2. Inserting (LIFO order in our bucket: 3 -> 2 -> 1)
        map.insert(node_1);
        map.insert(node_2);
        map.insert(node_3);

        println!(
            "LOG: Inserted 3 nodes with the next keys: {}, {}, {}",
            key_a, key_b, key_c
        );

        //3. Delete middle inserted node
        println!("LOG: Removing middle: {}", key_b);
        let deleted = map.delete(Some(node_2), entry_eq_kv);
        if let Some(d) = deleted {
            unsafe {
                let entry = container_of!(d.as_ptr(), Entry, node);
                println!(
                    "LOG: Freeing node with key:value: '{}':'{}'",
                    (*entry).key,
                    (*entry).val
                );
                free_entry(d);
            }
        };

        assert_eq!(map.current.size, 2, "Table size should be exactly 2");

        //4. Get C node and check if it poins to A node
        let c_node = map.lookup(Some(node_3), entry_eq_kv);
        assert!(
            !c_node.is_null(),
            "FAILED: Could not find any node with key '{}'",
            key_c,
        );

        unsafe {
            if let Some(c) = *c_node {
                let ptr = c.as_ref().next;
                assert!(
                    ptr.is_some(),
                    "FAILED: Head should point to some valid tail, not none!"
                );
                assert!(
                    ptr.unwrap().as_ptr() == node_1.as_ptr(),
                    "FAILED: Head is not pointing at the expected tail; HEAD NEXT: {:?}, EXPECTED NEXT: {:?}",
                    (*(c.as_ptr())).next.unwrap(),
                    node_1
                );
                println!(
                    "LOG: C node points to A node: {:?} == {:?}",
                    ptr.unwrap().as_ptr(),
                    node_1.as_ptr()
                )
            }
        }

        //5. Delete tail node A
        println!("LOG: Removing middle: {}", key_a);
        let deleted = map.delete(Some(node_1), entry_eq_kv);
        if let Some(d) = deleted {
            unsafe {
                let entry = container_of!(d.as_ptr(), Entry, node);
                println!(
                    "LOG: Freeing node with key:value: '{}':'{}'",
                    (*entry).key,
                    (*entry).val
                );
                free_entry(d);
            }
        };

        assert_eq!(map.current.size, 1, "Table size should be exactly 2");

        //6. Get C node and check if it poins to NONE node
        let c_node = map.lookup(Some(node_3), entry_eq_kv);
        assert!(
            !c_node.is_null(),
            "FAILED: Could not find any node with key '{}'",
            key_c,
        );

        unsafe {
            if let Some(c) = *c_node {
                let ptr = c.as_ref().next;
                assert!(!ptr.is_some(), "FAILED: Head should point to NONE!");
                println!("LOG: C node points to NONE node: {:?} == NONE", ptr,)
            }
        }

        // 7. Clear after yourself, please!
        clear_nodes_by_key(&mut map, Some(node_3), 1);

        println!("SUCCESS: Delete chain head test succsesfull");
    }

    /// Test lookup behavior when the key does not exist in the map.
    #[test]
    fn test_lookup_missing_key() {
        // TODO:
        // 1. Insert some nodes.
        // 2. Perform a lookup with a key/hcode that was never inserted.
        // 3. Assert that lookup returns null_mut().

        // 1. Create 3 nodes
        let mut map = HMap::with_capacity(4);
        let key_a = "key_A";
        let key_b = "key_A";
        let key_c = "key_A";
        let node_1 = create_node(key_a, "value_1");
        let node_2 = create_node(key_b, "value_2");
        let node_3 = create_node(key_c, "value_3");

        // 2. Inserting (LIFO order in our bucket: 2 -> 1)
        map.insert(node_1);
        map.insert(node_2);

        println!(
            "LOG: Inserted 3 nodes with the next keys: {}, {}, {}",
            key_a, key_b, key_c
        );

        //3. Get NODE which wasnt inserted
        let c_node = map.lookup(Some(node_3), entry_eq_kv);
        assert!(
            c_node.is_null(),
            "FAILED: Find a node which wasnt inserted '{}'",
            key_c,
        );

        // 4. Clear after yourself, please!
        clear_nodes_by_key(&mut map, Some(node_1), 2);

        println!("SUCCESS: Lookup missing key");
    }

    /// Test re-inserting the same node pointer (logic check).
    /// This test proves that the HMap is "dumb" and doesn't check for
    /// self-referencing loops, leaving safety to the higher-level caller.
    #[test]
    fn test_duplicate_node_pointer_naive() {
        // 1. Setup: small capacity to target a specific bucket easily
        let mut map = HMap::with_capacity(4);
        let key = "naive_loop_key";
        let mut node = create_node(key, "value_1");

        // 2. First insertion:
        map.insert(node);
        assert_eq!(map.current.size, 1);

        // 3. Second insertion of the EXACT SAME node pointer.
        map.insert(node);

        // If the map is "dumb", it increments size without checking for physical identity
        assert_eq!(
            map.current.size, 2,
            "Map should naively increment size for duplicate pointers"
        );

        println!("LOG: Circular reference created via duplicate insertion.");

        // 4. Verification of the loop
        unsafe {
            // Accessing the bucket directly based on type: NonNull<Option<NonNull<HNode>>>
            let hcode = node.as_ref().hcode;
            let pos = (hcode & map.current.mask) as usize;

            // Offset to the specific bucket and dereference
            let bucket_ptr = map.current.buckets.as_ptr().add(pos);
            let head_option = *bucket_ptr; // Option<NonNull<HNode>>

            let first_node = head_option.expect("Bucket should not be empty");
            let second_node = first_node
                .as_ref()
                .next
                .expect("Loop: node.next should point to itself");

            println!("LOG: Pointer 1: {:p}", first_node.as_ptr());
            println!("LOG: Pointer 2: {:p}", second_node.as_ptr());

            assert_eq!(
                first_node, second_node,
                "Infinite loop detected: node points to itself!"
            );
        }

        // 5. Emergency Cleanup
        // We MUST break the cycle manually before the test ends,
        // otherwise a Drop implementation or a cleanup helper will hang forever.
        unsafe {
            node.as_mut().next = None;
            // Now it's safe to delete/free without infinite recursion
            let deleted = map.delete(Some(node), entry_eq);
            if let Some(d) = deleted {
                free_entry(d);
            }
        }
    }

    /// Stress test for memory leaks and pointer stability.
    // #[test]
    fn test_stress_insert_delete() {
        // TODO:
        // 1. In a loop (e.g., 1000 iterations), insert nodes with different keys.
        // 2. In another loop, delete them all.
        // 3. Verify size returns to 0.
        // Run this test with 'cargo miri test' to detect leaks.
    }

    /// Test if lookup correctly checks both 'current' and 'older' tables during resizing.
    // #[test]
    fn test_lookup_during_resizing() {
        // TODO:
        // 1. Manually move a node to the 'older' table.
        // 2. Perform a lookup via HMap.
        // 3. Verify that HMap finds it even if it's not in the 'current' table.
    }
}
