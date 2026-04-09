![Redis-rs Project Poster](./redis.png)
# Redis-like in Rust

## Overview
This is a learning project in Rust to build a minimal in-memory key-value store inspired by Redis.  
The goal is to practice systems programming concepts: UNSAFE RUST, manual memory management, data structures, TCP networking, and multithreading.  
Based on [Build Your Own Redis](https://build-your-own.org/redis/).

### The Foundation
This project isn't just a wrapper; it's a deep-dive into the internals of Redis:
- **Intrusive Structures:** We don't store data *in* the map; we embed the map *into* our data. This minimizes pointers and enhances cache locality.
- **Pointer-to-Pointer Lookup:** Our lookup returns an indirect pointer `**Node`, enabling clean $O(1)$ deletions without searching for the predecessor.
- **Non-blocking I/O:** Built on top of an event loop to handle concurrent connections efficiently.
- **Memory Control:** Full ownership of the memory lifecycle via manual allocations and explicit `drop_in_place` calls.

---
## Features

- TCP server with simple protocol (GET / SET / DEL)  
- Intrusive Hash Map for key storage  
- Set and B-Tree data structures  
- TTL (automatic key expiration)  
- Multithreaded request handling  

---
## Progress

- Chapters:
  - ✅ 1                
  - ✅ 2               
  - ✅ 3                
  - ✅ 4                 
  - ✅ 5                  
  - ✅ 6                   
  - ✅ 7.1                  
  - ✅ 7.2              
  - ✅ 8.1                     
  - ✅ 8.2                            
  - ✅ 8.2                            
  - ⬜ 9                            
  - ⬜ 10                                   
  - ⬜ 11                                   
  - ⬜ 12                                          
  - ⬜ 13                                   
  - ⬜ 14                            
 
---
## TODO

- ✅ TCP server/client 
- ✅ Basic GET / SET / DEL
- ✅ Primitive protocol Request/Response
- ✅ Hash Map logic  
- ✅ Write tests for HMap/HTable
- ✅ Improve GET/SET/DEL commands for HMap/HTable
- ⬜ Deserialization/serialization
- ⬜ Add B-Tree
- ⬜ Add Sorted Set
- ⬜ Timers and timeout
- ⬜ Implement TTL (automatic key expiration)  
- ⬜ Add multithreading (worker pool / sharding)  
- ⬜ Write tests for each data structure  
- ⬜ Run mini-benchmarks (ops/sec, latency)  
- ⬜ Complete README with architecture and benchmark results  

---

## Project Structure (example)
src/                              
├─ main.rs # TCP server                              
├─ hashtable.rs # Intrusive Hash Map                              
//future                              
├─ btree.rs # B-Tree                              
├─ set.rs # Set                              
├─ ttl.rs # TTL mechanism                              
├─ threading.rs # Multithreading                              
└─ utils.rs # Helper functions                              

---

## Next Steps (2–3 weeks)
1. Finish Redis in Rust                                
3. Write tests and mini-benchmarks                                
4. Publish progress on LinkedIn/GitHub with architecture explanation                              
