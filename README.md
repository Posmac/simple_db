![Redis-rs Project Poster](./redis.png)
# Redis-like in Rust

## Overview
This is a learning project in Rust to build a minimal in-memory key-value store inspired by Redis.  
The goal is to practice systems programming concepts: UNSAFE RUST, manual memory management, data structures, TCP networking, and multithreading.  
Based on [Build Your Own Redis](https://build-your-own.org/redis/).

---
## Features

- TCP server with simple protocol (GET / SET / DEL)  
- Intrusive Hash Map for key storage  
- Set and B-Tree data structures  
- TTL (automatic key expiration)  
- Multithreaded request handling  

---
## Progress

- Completed up to **Chapter 8** of the BYOR guide  
- Implemented so far:
  - TCP server ✅
  - Protocol ✅  
  - Basic GET / SET commands ✅  
  - Hash Map storage logic ✅  
 
---
## TODO

- ✅ TCP server  
- ✅ Basic GET / SET / DEL 
- ✅ Hash Map logic  
- ✅ Key/value serialization
- ⬜ Improve DEL command
- ⬜ Improve Set operations  
- ⬜ Add B-Tree for ordered data  
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

1. Finish core commands (ADD/SET/DEL) using more complex data structures                                
2. Add TTL and simple multithreading                                
3. Write tests and mini-benchmarks                                
4. Publish progress on LinkedIn/GitHub with architecture explanation                              
