use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

// Consider that you invoke a command with different arguments several times
// If you select some items each time, how will skim remember it?
// => Well, we'll give each invocation a number, i.e. RUN_NUM
// What if you invoke the same command and same arguments twice?
// => We use NUM_MAP to specify the same run number.
lazy_static! {
    static ref RUN_NUM: AtomicU32 = AtomicU32::new(0);
    static ref SEQ: AtomicU32 = AtomicU32::new(1);
    static ref NUM_MAP: Mutex<HashMap<String, u32>> = {
        let mut m = HashMap::new();
        m.insert("".to_string(), 0);
        Mutex::new(m)
    };
}

pub fn current_run_num() -> u32 {
    RUN_NUM.load(Ordering::SeqCst)
}

pub fn mark_new_run(query: &str) -> u32 {
    let mut map = NUM_MAP.lock().expect("failed to lock NUM_MAP");
    let query = query.to_string();
    let run_num = *map.entry(query).or_insert_with(|| SEQ.fetch_add(1, Ordering::SeqCst));
    RUN_NUM.store(run_num, Ordering::SeqCst);
    run_num
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        assert_eq!(0, current_run_num());
        mark_new_run("a");
        assert_eq!(1, current_run_num());
        mark_new_run("b");
        assert_eq!(2, current_run_num());
        mark_new_run("a");
        assert_eq!(1, current_run_num());
        mark_new_run("");
        assert_eq!(0, current_run_num());
    }
}
