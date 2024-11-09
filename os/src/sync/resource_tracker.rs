use alloc::vec;
use alloc::vec::Vec;

/// resource tracker
pub struct ResourceTracker {
    enabled: bool,
    available: Vec<isize>,
    allocation: Vec<Vec<isize>>,
    need: Vec<Vec<isize>>,
}

impl ResourceTracker {
    /// create a new resource tracker
    pub fn new() -> Self {
        Self {
            enabled: false,
            available: Vec::new(),
            allocation: Vec::new(),
            need: Vec::new(),
        }
    }
    /// enable or disable the resource tracker
    pub fn enable(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    /// add a resource
    pub fn add_resource(&mut self, res_id: usize, count: isize) {
        if !self.enabled {
            return;
        }

        while self.available.len() <= res_id {
            self.available.push(0);
        }
        self.available[res_id] = count;

        for vec in self.allocation.iter_mut().chain(self.need.iter_mut()) {
            while vec.len() <= res_id {
                vec.push(0);
            }
        }

        for tid in 0..self.allocation.len() {
            if res_id < self.need[tid].len() {
                self.need[tid][res_id] = self.available[res_id] - self.allocation[tid][res_id];
            }
        }
    }
    /// resize the task
    pub fn resize_task(&mut self, tid: usize) {
        if !self.enabled {
            return;
        }

        let res_count = self.available.len();
        while self.allocation.len() <= tid {
            self.allocation.push(vec![0; res_count]);
            self.need.push(vec![0; res_count]);
        }
    }
    /// request a resource
    pub fn request_resource(&mut self, tid: usize, res_id: usize, count: isize) -> bool {
        if !self.enabled {
            return true;
        }

        self.resize_task(tid);

        if res_id >= self.available.len() || self.available[res_id] < count {
            return false;
        }

        self.available[res_id] -= count;
        self.allocation[tid][res_id] += count;
        self.need[tid][res_id] -= count;

        let is_safe = self.check_safe_state();

        if !is_safe {
            self.available[res_id] += count;
            self.allocation[tid][res_id] -= count;
            self.need[tid][res_id] += count;
        }

        is_safe
    }
    /// release a resource
    pub fn release_resource(&mut self, tid: usize, res_id: usize, count: isize) {
        if !self.enabled {
            return;
        }

        self.available[res_id] += count;
        self.allocation[tid][res_id] -= count;
        self.need[tid][res_id] += count;
    }
    /// check if the current state is safe
    fn check_safe_state(&self) -> bool {
        let n_tasks = self.allocation.len();
        let n_resources = self.available.len();

        let mut work = self.available.clone();
        let mut finish = vec![false; n_tasks];

        loop {
            let mut found = false;
            for tid in 0..n_tasks {
                if !finish[tid] {
                    let can_complete = (0..n_resources)
                        .all(|rid| self.need[tid][rid] <= work[rid]);

                    if can_complete {
                        finish[tid] = true;
                        for rid in 0..n_resources {
                            work[rid] += self.allocation[tid][rid];
                        }
                        found = true;
                    }
                }
            }
            if !found {
                break;
            }
        }

        finish.iter().all(|&x| x)
    }
}
