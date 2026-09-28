// Compile the actual patched upstream consumer library, including its tests.
#![allow(dead_code)]
#[path = "../../buzz-test-client/src/lib.rs"]
mod consumer;
