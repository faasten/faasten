use std::collections::{BTreeMap};
use serde_json::json;

#[derive(Clone, Debug)]
pub struct Metrics {
    pub start_tsp: u64,
    pub end_tsp: u64,
    pub num_drop: u32,  // number of dropped requests
    pub num_complete: u32,  // number of requests completed
    pub num_evict: u32, // number of evictions
    pub vm_mem_size: BTreeMap<usize, usize>,
    pub boot_tsp: BTreeMap<usize, Vec<u64>>, // key is vm_id, value is boot timestamp
    pub evict_tsp: BTreeMap<usize, Vec<u64>>,
    pub req_rsp_tsp: BTreeMap<usize, Vec<u64>> // key is vm_id, value is request send time and response receive time
}

impl Metrics {
    pub fn new() -> Metrics {
        return Metrics {
            start_tsp:0,
            end_tsp:0,
            num_drop: 0,
            num_complete: 0,
            num_evict: 0,
            vm_mem_size: BTreeMap::new(),
            boot_tsp: BTreeMap::new(),
            evict_tsp: BTreeMap::new(),
            req_rsp_tsp: BTreeMap::new(),
        }

    }

    pub fn to_json(&self) -> serde_json::value::Value {
        return json!({
            "number of vms created": self.vm_mem_size.len(),
            "vm memory sizes": self.vm_mem_size,
            "number of requests completed": self.num_complete,
            "number of requests dropped": self.num_drop,
            "number of evictions": self.num_evict,
            "boot timestamps": self.boot_tsp,
            "request/response timestamps":self.req_rsp_tsp,
            "eviction timestamps": self.evict_tsp,
        });
    }
}
